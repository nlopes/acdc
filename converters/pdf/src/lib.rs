//! PDF converter for `AsciiDoc` documents.
//!
//! This converter renders the acdc AST into Typst markup and delegates themed
//! preamble generation, asset resolution, font loading, and PDF compilation to
//! the shared `acdc-pdf-*` crates.

#[cfg(feature = "pre-spec-subs")]
use std::{cell::Cell, rc::Rc};
use std::{
    collections::BTreeSet,
    fmt::Write as _,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::SubsFlags;
use acdc_converters_core::{
    Diagnostics, Options, PrettyDuration, inlines_to_string, visitor::Visitor,
};
use acdc_parser::{
    Block, DelimitedBlockType, Document, DocumentAttributes, InlineMacro, InlineNode, ListItem,
    SafeMode, Source, Table, TableRow,
};
use acdc_pdf_images::{
    Error as ImageError, ImageMap, ResolveConfig, ResolveFailure, SourcePolicy, resolve,
};
use acdc_pdf_render::{RenderConfig, render_pdf};
use acdc_pdf_theme::Theme;
use acdc_pdf_typst::{EmitOptions, preamble};

mod converter;
mod error;
mod pdf_visitor;
mod visitor;

pub use acdc_pdf_typst::PageSize;
pub use error::Error;

use pdf_visitor::PdfVisitor;

const MAX_THEME_FILE_BYTES: usize = 1024 * 1024;

/// PDF-specific conversion options.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default)]
pub struct PdfOptions {
    /// Explicitly trusted directories to search for sfnt fonts (`ttf`, `otf`, `ttc`, `otc`).
    pub font_dirs: Vec<PathBuf>,
    /// Optional header logo. Resolved relative to the current working directory.
    pub logo: Option<PathBuf>,
    /// Optional running-header title. Defaults to the document title when absent.
    pub title: Option<String>,
    /// Optional diagonal watermark stamped on every page.
    pub watermark: Option<String>,
    /// Optional footer timestamp shown with the watermark metadata.
    pub watermark_timestamp: Option<String>,
    /// Optional page size override. Document `:pdf-page-size:` is used next.
    pub page: Option<PageSize>,
    /// Optional theme YAML file. Defaults to the bundled neutral theme.
    pub theme: Option<PathBuf>,
    /// Strip page background, header, and footer chrome.
    pub plain: bool,
    /// Emit an automatic table of contents when the document does not set `:toc:`.
    pub toc: bool,
    /// Treat asset resolution failures as hard errors.
    pub strict_assets: bool,
    /// Also write the generated Typst markup to this path for debugging.
    pub emit_typst: Option<PathBuf>,
}

/// PDF converter processor.
#[derive(Clone, Debug)]
pub struct Processor<'a> {
    options: Options,
    document_attributes: DocumentAttributes<'a>,
    pdf_options: PdfOptions,
    #[cfg(feature = "pre-spec-subs")]
    pub(crate) current_subs: Rc<Cell<SubsFlags>>,
}

impl Processor<'_> {
    /// Override PDF-specific conversion options.
    #[must_use]
    pub fn with_pdf_options(mut self, pdf_options: PdfOptions) -> Self {
        self.pdf_options = pdf_options;
        self
    }

    #[cfg(test)]
    fn convert_to_typst_source(
        &self,
        doc: &Document<'_>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<String, Error> {
        let theme = self.load_theme()?;
        let assets = ImageMap::new();
        let font_dirs = self.pdf_options.font_dirs.clone();
        let emit_options = self.emit_options(doc, None, &font_dirs, diagnostics);
        self.emit_typst_source(doc, &assets, &theme, &emit_options, diagnostics)
    }

    pub(crate) fn options(&self) -> &Options {
        &self.options
    }

    pub(crate) fn document_attributes(&self) -> &DocumentAttributes<'_> {
        &self.document_attributes
    }

    pub(crate) fn pdf_options(&self) -> &PdfOptions {
        &self.pdf_options
    }

    pub(crate) fn render_document(
        &self,
        doc: &Document<'_>,
        source_file: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<RenderedPdf, Error> {
        let theme = self.load_theme()?;

        // Validated image snapshots are spooled here so their bytes are read on
        // demand and cannot change between validation and rendering. The
        // directory is removed after rendering has consumed the snapshots.
        let spool = tempfile::Builder::new()
            .prefix("acdc-pdf-images-")
            .tempdir()?;

        let asset_start = Instant::now();
        let image_urls = collect_image_urls(doc);
        let mut assets =
            self.resolve_images(doc, &image_urls, source_file, spool.path(), diagnostics)?;
        let resolved_document_image_count = assets.images().count();
        let font_dirs = self.pdf_options.font_dirs.clone();
        let logo = self.resolve_logo(&mut assets, spool.path(), diagnostics)?;
        let asset_duration = asset_start.elapsed();

        let emit_start = Instant::now();
        let emit_options = self.emit_options(doc, logo, &font_dirs, diagnostics);
        let typst = self.emit_typst_source(doc, &assets, &theme, &emit_options, diagnostics)?;
        self.write_debug_typst(&typst)?;
        let emit_duration = emit_start.elapsed();

        let render_start = Instant::now();
        let rendered = render_pdf(&typst, &assets, &RenderConfig { font_dirs })?;
        let render_duration = render_start.elapsed();
        for warning in rendered.warnings {
            diagnostics.warn(format!("Typst warning: {warning}"));
        }

        Ok(RenderedPdf {
            pdf: rendered.pdf,
            resolved_document_image_count,
            timings: PdfTimings {
                assets: asset_duration,
                emit: emit_duration,
                render: render_duration,
            },
        })
    }

    fn emit_typst_source(
        &self,
        doc: &Document<'_>,
        assets: &ImageMap,
        theme: &Theme,
        emit_options: &EmitOptions,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<String, Error> {
        let processor = Processor {
            options: self.options.clone(),
            document_attributes: doc.attributes.clone(),
            pdf_options: self.pdf_options.clone(),
            #[cfg(feature = "pre-spec-subs")]
            current_subs: Rc::new(Cell::new(SubsFlags::all())),
        };
        let mut visitor = PdfVisitor::new(
            processor,
            assets,
            !doc.toc_entries.is_empty(),
            diagnostics.reborrow(),
        );
        preamble::write(&mut visitor.writer, theme, emit_options);
        visitor.visit_document(doc)?;
        Ok(visitor.writer.into_string())
    }

    fn load_theme(&self) -> Result<Theme, Error> {
        let Some(path) = &self.pdf_options.theme else {
            return Ok(Theme::default());
        };
        let yaml = read_theme_file(path)?;
        Theme::from_yaml_str(&yaml).map_err(|source| Error::ThemeParse {
            path: path.clone(),
            source,
        })
    }

    fn emit_options(
        &self,
        doc: &Document<'_>,
        logo: Option<String>,
        font_dirs: &[PathBuf],
        diagnostics: &mut Diagnostics<'_>,
    ) -> EmitOptions {
        EmitOptions {
            page: self.page_size(doc, diagnostics),
            plain: self.pdf_options.plain,
            brand_fonts: !font_dirs.is_empty(),
            title: self
                .pdf_options
                .title
                .clone()
                .or_else(|| document_title(doc)),
            logo,
            watermark: self.pdf_options.watermark.clone(),
            watermark_timestamp: self.pdf_options.watermark_timestamp.clone(),
        }
    }

    fn page_size(&self, doc: &Document<'_>, diagnostics: &mut Diagnostics<'_>) -> PageSize {
        if let Some(page) = self.pdf_options.page {
            return page;
        }
        let Some(value) = doc.attributes.get_string("pdf-page-size") else {
            return PageSize::A4;
        };
        match value.as_ref().to_ascii_lowercase().as_str() {
            "a4" => PageSize::A4,
            "letter" | "us-letter" => PageSize::Letter,
            other => {
                diagnostics.warn_with_advice(
                    format!("unsupported PDF page size '{other}', using A4"),
                    "Use `--page a4`, `--page letter`, or set `:pdf-page-size:` to `a4` or `letter`.",
                );
                PageSize::A4
            }
        }
    }

    fn resolve_images(
        &self,
        doc: &Document<'_>,
        image_urls: &[String],
        source_file: Option<&Path>,
        spool_dir: &Path,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<ImageMap, Error> {
        if image_urls.is_empty() {
            return Ok(ImageMap::new());
        }
        let base_dir = base_dir_for_source(source_file);
        let source_policy = image_source_policy(
            self.options.safe_mode(),
            doc.attributes.contains_key("allow-uri-read"),
        );
        let mut config = ResolveConfig::new(base_dir, spool_dir);
        config.source_policy = source_policy;
        let url_refs: Vec<&str> = image_urls.iter().map(String::as_str).collect();
        let resolved = resolve(&url_refs, &config);
        self.report_asset_failures(
            "image",
            "render fallback text for that image",
            resolved.failures,
            diagnostics,
        )?;
        Ok(resolved.assets)
    }

    fn resolve_logo(
        &self,
        assets: &mut ImageMap,
        spool_dir: &Path,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<Option<String>, Error> {
        let Some(logo_path) = &self.pdf_options.logo else {
            return Ok(None);
        };
        let url = logo_path.to_string_lossy();
        // The logo is an explicit converter option rather than a document
        // reference, so safe mode does not block it.
        let config = ResolveConfig::new(
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            spool_dir,
        );
        let mut resolved = resolve(&[url.as_ref()], &config);
        if let Some(image) = resolved.assets.get(url.as_ref()) {
            let virtual_path = image.virtual_path.clone();
            assets.extend(resolved.assets);
            return Ok(Some(virtual_path));
        }

        if resolved.failures.is_empty() {
            resolved.failures.push(ResolveFailure {
                url: url.into_owned(),
                error: ImageError::UnknownFormat,
            });
        }
        self.report_asset_failures(
            "logo",
            "omit the header logo",
            resolved.failures,
            diagnostics,
        )?;
        Ok(None)
    }

    fn report_asset_failures(
        &self,
        kind: &str,
        fallback: &str,
        failures: Vec<ResolveFailure>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Error> {
        if failures.is_empty() {
            return Ok(());
        }
        if self.pdf_options.strict_assets {
            let details = failures
                .iter()
                .map(|failure| format!("  {}: {}", failure.url, failure.error))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(Error::AssetResolution(format!(
                "failed to resolve {} {kind}(s):\n{details}",
                failures.len(),
            )));
        }
        for failure in failures {
            diagnostics.warn_with_advice(
                format!(
                    "{kind} {} could not be embedded: {}",
                    failure.url, failure.error
                ),
                format!("The PDF will {fallback}."),
            );
        }
        Ok(())
    }

    fn write_debug_typst(&self, typst: &str) -> Result<(), Error> {
        let Some(path) = &self.pdf_options.emit_typst else {
            return Ok(());
        };
        std::fs::write(path, typst).map_err(|source| Error::TypstWrite {
            path: path.clone(),
            source,
        })
    }
}

fn read_theme_file(path: &Path) -> Result<String, Error> {
    let file = File::open(path).map_err(|source| Error::ThemeRead {
        path: path.to_path_buf(),
        source,
    })?;
    let metadata = file.metadata().map_err(|source| Error::ThemeRead {
        path: path.to_path_buf(),
        source,
    })?;
    let read_limit = u64::try_from(MAX_THEME_FILE_BYTES).unwrap_or(u64::MAX);
    if metadata.len() > read_limit {
        return Err(Error::ThemeTooLarge {
            path: path.to_path_buf(),
            limit: MAX_THEME_FILE_BYTES,
            actual: Some(metadata.len()),
        });
    }

    let mut yaml = String::new();
    file.take(read_limit.saturating_add(1))
        .read_to_string(&mut yaml)
        .map_err(|source| Error::ThemeRead {
            path: path.to_path_buf(),
            source,
        })?;
    if yaml.len() > MAX_THEME_FILE_BYTES {
        return Err(Error::ThemeTooLarge {
            path: path.to_path_buf(),
            limit: MAX_THEME_FILE_BYTES,
            actual: None,
        });
    }
    Ok(yaml)
}

pub(crate) struct RenderedPdf {
    pub(crate) pdf: Vec<u8>,
    pub(crate) resolved_document_image_count: usize,
    pub(crate) timings: PdfTimings,
}

pub(crate) struct PdfTimings {
    assets: Duration,
    emit: Duration,
    render: Duration,
}

impl PdfTimings {
    pub(crate) fn write_report(&self, resolved_document_image_count: usize) {
        eprintln!(
            "  Resolved {resolved_document_image_count} document PDF image(s) in {}",
            self.assets.pretty_print()
        );
        eprintln!("  Emitted Typst markup in {}", self.emit.pretty_print());
        eprintln!("  Rendered PDF in {}", self.render.pretty_print());
    }
}

fn image_source_policy(safe_mode: SafeMode, allow_uri_read: bool) -> SourcePolicy {
    match safe_mode {
        SafeMode::Unsafe => SourcePolicy::Unrestricted,
        SafeMode::Safe => SourcePolicy::Confined {
            allow_network: true,
        },
        SafeMode::Server => SourcePolicy::Confined {
            allow_network: allow_uri_read,
        },
        SafeMode::Secure => SourcePolicy::DenyAll,
    }
}

fn base_dir_for_source(source_file: Option<&Path>) -> PathBuf {
    source_file
        .and_then(Path::parent)
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(
            || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            Path::to_path_buf,
        )
}

fn document_title(doc: &Document<'_>) -> Option<String> {
    let title = inlines_to_string(doc.header.as_ref()?.title.as_ref());
    (!title.is_empty()).then_some(title)
}

fn collect_image_urls(doc: &Document<'_>) -> Vec<String> {
    let mut urls = BTreeSet::new();
    if let Some(header) = &doc.header {
        collect_inline_images(header.title.as_ref(), &mut urls);
    }
    collect_block_images(&doc.blocks, &mut urls);
    urls.into_iter().collect()
}

fn collect_block_images(blocks: &[Block<'_>], urls: &mut BTreeSet<String>) {
    for block in blocks {
        match block {
            Block::Section(section) => {
                collect_inline_images(section.title.as_ref(), urls);
                collect_block_images(&section.content, urls);
            }
            Block::Paragraph(paragraph) => {
                collect_inline_images(paragraph.title.as_ref(), urls);
                collect_inline_images(&paragraph.content, urls);
            }
            Block::DelimitedBlock(block) => {
                collect_inline_images(block.title.as_ref(), urls);
                collect_delimited_block_images(&block.inner, urls);
            }
            Block::OrderedList(list) => {
                collect_inline_images(list.title.as_ref(), urls);
                for item in &list.items {
                    collect_list_item_images(item, urls);
                }
            }
            Block::UnorderedList(list) => {
                collect_inline_images(list.title.as_ref(), urls);
                for item in &list.items {
                    collect_list_item_images(item, urls);
                }
            }
            Block::DescriptionList(list) => {
                collect_inline_images(list.title.as_ref(), urls);
                for item in &list.items {
                    collect_inline_images(&item.term, urls);
                    collect_inline_images(&item.principal_text, urls);
                    collect_block_images(&item.description, urls);
                }
            }
            Block::CalloutList(list) => {
                collect_inline_images(list.title.as_ref(), urls);
                for item in &list.items {
                    collect_inline_images(&item.principal, urls);
                    collect_block_images(&item.blocks, urls);
                }
            }
            Block::Admonition(admonition) => {
                collect_inline_images(admonition.title.as_ref(), urls);
                collect_block_images(&admonition.blocks, urls);
            }
            Block::Image(image) => {
                collect_inline_images(image.title.as_ref(), urls);
                collect_source(&image.source, urls);
            }
            Block::DiscreteHeader(header) => collect_inline_images(header.title.as_ref(), urls),
            Block::Audio(audio) => collect_inline_images(audio.title.as_ref(), urls),
            Block::Video(video) => collect_inline_images(video.title.as_ref(), urls),
            Block::TableOfContents(_)
            | Block::DocumentAttribute(_)
            | Block::ThematicBreak(_)
            | Block::PageBreak(_)
            | Block::Comment(_)
            | _ => {}
        }
    }
}

fn collect_delimited_block_images(block: &DelimitedBlockType<'_>, urls: &mut BTreeSet<String>) {
    match block {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => collect_block_images(blocks, urls),
        DelimitedBlockType::DelimitedTable(table) => collect_table_images(table, urls),
        DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | DelimitedBlockType::DelimitedStem(_)
        | _ => {}
    }
}

fn collect_table_images(table: &Table<'_>, urls: &mut BTreeSet<String>) {
    for row in table
        .header
        .iter()
        .chain(table.rows.iter())
        .chain(table.footer.iter())
    {
        collect_table_row_images(row, urls);
    }
}

fn collect_table_row_images(row: &TableRow<'_>, urls: &mut BTreeSet<String>) {
    for column in &row.columns {
        collect_block_images(&column.content, urls);
    }
}

fn collect_list_item_images(item: &ListItem<'_>, urls: &mut BTreeSet<String>) {
    collect_inline_images(&item.principal, urls);
    collect_block_images(&item.blocks, urls);
}

fn collect_inline_images(nodes: &[InlineNode<'_>], urls: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            InlineNode::BoldText(text) => collect_inline_images(&text.content, urls),
            InlineNode::ItalicText(text) => collect_inline_images(&text.content, urls),
            InlineNode::MonospaceText(text) => collect_inline_images(&text.content, urls),
            InlineNode::HighlightText(text) => collect_inline_images(&text.content, urls),
            InlineNode::SubscriptText(text) => collect_inline_images(&text.content, urls),
            InlineNode::SuperscriptText(text) => collect_inline_images(&text.content, urls),
            InlineNode::CurvedQuotationText(text) => collect_inline_images(&text.content, urls),
            InlineNode::CurvedApostropheText(text) => collect_inline_images(&text.content, urls),
            InlineNode::Macro(InlineMacro::Image(image)) => collect_source(&image.source, urls),
            InlineNode::Macro(InlineMacro::Footnote(footnote)) => {
                collect_inline_images(&footnote.content, urls);
            }
            InlineNode::Macro(InlineMacro::Url(url)) => collect_inline_images(&url.text, urls),
            InlineNode::Macro(InlineMacro::Link(link)) => collect_inline_images(&link.text, urls),
            InlineNode::Macro(InlineMacro::Mailto(mailto)) => {
                collect_inline_images(&mailto.text, urls);
            }
            InlineNode::Macro(InlineMacro::CrossReference(xref)) => {
                collect_inline_images(&xref.text, urls);
            }
            InlineNode::PlainText(_)
            | InlineNode::RawText(_)
            | InlineNode::VerbatimText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::Macro(_)
            | InlineNode::CalloutRef(_)
            | _ => {}
        }
    }
}

fn collect_source(source: &Source<'_>, urls: &mut BTreeSet<String>) {
    urls.insert(source.to_string());
}

fn encode_label(value: &str) -> String {
    let mut out = String::with_capacity(value.len().saturating_mul(2).saturating_add(3));
    out.push_str("id");
    if value.is_empty() {
        return out;
    }
    out.push('-');
    for byte in value.bytes() {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use acdc_converters_core::{Converter, WarningSource};
    use tempfile::NamedTempFile;

    use super::*;

    const DEFAULT_THEME_YAML: &str =
        include_str!("../../../acdc-pdf-theme/assets/theme/default.yaml");

    #[test]
    fn labels_are_typst_safe_and_collision_resistant() {
        assert_eq!(encode_label(""), "id");
        assert_eq!(encode_label("a.b"), "id-612e62");
        assert_ne!(encode_label("a.b"), encode_label("a/b"));
        assert_ne!(encode_label("é"), encode_label("è"));
        assert!(
            encode_label("punctuation / and unicode 🦀")
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        );
    }

    #[test]
    fn pdf_safe_modes_map_to_image_source_policy() {
        assert_eq!(
            image_source_policy(SafeMode::Unsafe, false),
            SourcePolicy::Unrestricted
        );
        assert_eq!(
            image_source_policy(SafeMode::Safe, false),
            SourcePolicy::Confined {
                allow_network: true
            }
        );
        assert_eq!(
            image_source_policy(SafeMode::Server, false),
            SourcePolicy::Confined {
                allow_network: false
            }
        );
        assert_eq!(
            image_source_policy(SafeMode::Server, true),
            SourcePolicy::Confined {
                allow_network: true
            }
        );
        assert_eq!(
            image_source_policy(SafeMode::Secure, true),
            SourcePolicy::DenyAll
        );
    }

    #[test]
    fn oversized_theme_file_is_rejected_with_path_and_size()
    -> Result<(), Box<dyn std::error::Error>> {
        let theme_file = NamedTempFile::new()?;
        let oversized = u64::try_from(MAX_THEME_FILE_BYTES)?.saturating_add(1);
        theme_file.as_file().set_len(oversized)?;

        let Err(Error::ThemeTooLarge {
            path,
            limit,
            actual,
        }) = read_theme_file(theme_file.path())
        else {
            return Err(std::io::Error::other("oversized theme unexpectedly accepted").into());
        };
        assert_eq!(path, theme_file.path());
        assert_eq!(limit, MAX_THEME_FILE_BYTES);
        assert_eq!(actual, Some(oversized));
        Ok(())
    }

    #[test]
    fn hostile_theme_colour_is_rejected_before_emission() -> Result<(), Box<dyn std::error::Error>>
    {
        let theme_file = NamedTempFile::new()?;
        let yaml = DEFAULT_THEME_YAML.replacen(
            r##"page_bg: "#ffffff""##,
            r#"page_bg: '") #undefined_function() //'"#,
            1,
        );
        std::fs::write(theme_file.path(), yaml)?;
        let parsed = acdc_parser::parse("A paragraph.\n", &acdc_parser::Options::default())?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone())
            .with_pdf_options(PdfOptions {
                theme: Some(theme_file.path().to_path_buf()),
                ..PdfOptions::default()
            });
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let Err(Error::ThemeParse { path, .. }) =
            processor.convert_to_typst_source(parsed.document(), &mut diagnostics)
        else {
            return Err(std::io::Error::other("hostile colour unexpectedly accepted").into());
        };
        assert_eq!(path, theme_file.path());
        Ok(())
    }

    #[test]
    fn hostile_theme_font_name_remains_literal_typst_data() -> Result<(), Box<dyn std::error::Error>>
    {
        let hostile = r#"Acme"), size: 1pt)#undefined_function()//"#;
        let fallback = format!(r#"fallback: [{hostile:?}, "IBM Plex Serif"]"#);
        let yaml = DEFAULT_THEME_YAML.replacen(r#"fallback: ["IBM Plex Serif"]"#, &fallback, 1);
        let theme_file = NamedTempFile::new()?;
        std::fs::write(theme_file.path(), yaml)?;
        let parsed = acdc_parser::parse("A paragraph.\n", &acdc_parser::Options::default())?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone())
            .with_pdf_options(PdfOptions {
                theme: Some(theme_file.path().to_path_buf()),
                ..PdfOptions::default()
            });
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;
        assert!(
            typst.contains(r#""Acme\"), size: 1pt)#undefined_function()//", "IBM Plex Serif""#)
        );

        // If the quote in the family name closed its string, the deliberately
        // undefined function above would make compilation fail. A valid PDF is
        // therefore an end-to-end assertion that the payload remained data.
        let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;
        assert!(rendered.pdf.starts_with(b"%PDF-"));
        Ok(())
    }

    #[test]
    fn converts_simple_document_to_typst_source() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Title\n\n== Section\n\nA *bold* link:https://example.com[link].\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;

        assert!(typst.contains("#set page("));
        assert!(typst.contains("#heading(level: 1)"));
        assert!(typst.contains("#strong["));
        assert!(typst.contains("#link(\"https://example.com\")"));
        Ok(())
    }

    #[test]
    fn stem_content_is_escaped_literal_text_with_warnings() -> Result<(), Box<dyn std::error::Error>>
    {
        let input =
            "stem:[#panic() $ x \\\\ path]\n\n[stem]\n++++\n#panic() $ [y] \\\\ path\n++++\n";
        let parsed = acdc_parser::parse(input, &acdc_parser::Options::default())?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;
        let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;
        assert!(
            typst.contains(r##"#text("#panic() $ x \\ path")"##),
            "{typst}"
        );
        assert!(
            typst.contains(r##"#text("#panic() $ [y] \\\\ path")"##),
            "{typst}"
        );
        assert!(!typst.contains("$ #panic()"));
        assert!(rendered.pdf.starts_with(b"%PDF-"));
        assert_eq!(warnings.len(), 4);
        assert!(
            warnings
                .iter()
                .all(|warning| warning.message.contains("stem content"))
        );
        Ok(())
    }

    #[test]
    fn image_collection_matches_rendered_titles_and_skips_verbatim_content()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= image:header.png[] Header\n\n. image:paragraph-title.png[]\nParagraph image:body.png[] and image:body.png[] again.\n\n.List image:list-title.png[]\n* item\n\n== image:section.png[] Section\n\n.Block image:block-title.png[]\n....\nimage:literal.png[]\n....\n\n////\nimage:comment.png[]\n////\n",
            &acdc_parser::Options::default(),
        )?;

        assert_eq!(
            collect_image_urls(parsed.document()),
            vec![
                "block-title.png",
                "body.png",
                "list-title.png",
                "paragraph-title.png",
                "section.png",
            ]
        );
        Ok(())
    }

    #[test]
    fn substitutions_control_typography_in_paragraphs_and_verbatim_blocks()
    -> Result<(), Box<dyn std::error::Error>> {
        let none = typst_source("[subs=none]\n(C) ... -> -- *bold*\n")?;
        assert!(none.contains(r#"#text("(C) ... -> -- *bold*")"#));

        let subtractive = typst_source("[subs=-replacements]\n(C) ... -> -- *bold*\n")?;
        assert!(subtractive.contains(r#"#text("(C) ... -> -- ")#strong["#));

        let default_listing = typst_source("....\n(C) ... -> -- *bold*\n....\n")?;
        assert!(default_listing.contains(r#"#raw(block: true, "(C) ... -> -- *bold*")"#));

        let additive_listing =
            typst_source("[subs=+replacements]\n....\n(C) ... -> -- *bold*\n....\n")?;
        assert!(additive_listing.contains('©'));
        assert!(additive_listing.contains('…'));
        assert!(additive_listing.contains('→'));
        assert!(additive_listing.contains("*bold*"));
        Ok(())
    }

    #[test]
    fn styles_only_declared_table_headers() -> Result<(), Box<dyn std::error::Error>> {
        let header = typst_source(
            "[cols=\"1,1\",options=\"header\"]\n|===\n|Name |Value\n|one |two\n|===\n",
        )?;
        assert_eq!(header.matches("#tableheader[").count(), 2);

        let without_header = typst_source("[cols=\"1,1\"]\n|===\n|one |two\n|===\n")?;
        assert!(!without_header.contains("#tableheader["));
        Ok(())
    }

    #[test]
    fn renders_simple_pdf_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Title\n\n== Section\n\nA paragraph.\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;

        assert!(rendered.pdf.starts_with(b"%PDF-"));
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        assert!(!pdf.get_pages().is_empty());
        Ok(())
    }

    #[test]
    fn toc_configuration_controls_outline_placement_title_and_depth()
    -> Result<(), Box<dyn std::error::Error>> {
        let automatic = typst_source(
            "= Title\n:toc:\n:toc-title: Contents\n:toclevels: 1\n\nIntro.\n\n== Section\n\n=== Child\n",
        )?;
        let title_position = automatic
            .find("#align(center)")
            .ok_or_else(|| std::io::Error::other("missing document title"))?;
        let toc_position = automatic
            .find("#outline(title: \"Contents\", depth: 1)")
            .ok_or_else(|| std::io::Error::other("missing configured outline"))?;
        let intro_position = automatic
            .find("#text(\"Intro.\")")
            .ok_or_else(|| std::io::Error::other("missing preamble"))?;
        assert!(title_position < toc_position && toc_position < intro_position);

        let macro_toc =
            typst_source("= Title\n:toc: macro\n\nIntro.\n\ntoc::[]\n\ntoc::[]\n\n== Section\n")?;
        let macro_position = macro_toc
            .find("#outline(")
            .ok_or_else(|| std::io::Error::other("missing macro outline"))?;
        let macro_intro_position = macro_toc
            .find("#text(\"Intro.\")")
            .ok_or_else(|| std::io::Error::other("missing macro preamble"))?;
        assert!(macro_intro_position < macro_position);
        assert_eq!(macro_toc.matches("#outline(").count(), 1);

        let missing_macro = typst_source("= Title\n:toc: macro\n\n== Section\n")?;
        assert!(!missing_macro.contains("#outline("));

        let preamble =
            typst_source("= Title\n:toc: preamble\n\nIntro.\n\n== Section\n\nA paragraph.\n")?;
        let preamble_toc = preamble
            .find("#outline(")
            .ok_or_else(|| std::io::Error::other("missing preamble outline"))?;
        let preamble_intro = preamble
            .find("#text(\"Intro.\")")
            .ok_or_else(|| std::io::Error::other("missing preamble content"))?;
        let preamble_section = preamble
            .find("#heading(level: 1)")
            .ok_or_else(|| std::io::Error::other("missing section"))?;
        assert!(preamble_intro < preamble_toc && preamble_toc < preamble_section);

        let no_sections = typst_source("= Title\n:toc:\n\nA paragraph.\n")?;
        assert!(!no_sections.contains("#outline("));

        let blank_title = typst_source("= Title\n:toc:\n:toc-title:\n\n== Section\n")?;
        assert!(
            blank_title.contains("#outline(title: none, depth: 2)"),
            "{blank_title}"
        );
        Ok(())
    }

    #[test]
    fn label_encoding_keeps_distinct_cross_references_compilable()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Title\n\n[id=\"a.b\"]\n== Dot\n\nSee xref:a.b[].\n\n[id=\"a/b\"]\n== Slash\n\nSee xref:a/b[].\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;
        assert!(typst.contains("<id-612e62>"), "{typst}");
        assert!(typst.contains("<id-612f62>"), "{typst}");
        let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;
        assert!(rendered.pdf.starts_with(b"%PDF-"));
        Ok(())
    }

    #[test]
    fn renders_configured_toc_pdf() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Title\n:toc:\n:toc-title: Contents\n:toclevels: 1\n\nIntro.\n\n== Section\n\n=== Child\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;
        assert!(rendered.pdf.starts_with(b"%PDF-"));
        Ok(())
    }

    #[test]
    fn logo_failure_advice_describes_omission() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse("A paragraph.\n", &acdc_parser::Options::default())?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone())
            .with_pdf_options(PdfOptions {
                logo: Some(PathBuf::from("missing-pdf-logo.png")),
                ..PdfOptions::default()
            });
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        {
            let mut diagnostics = Diagnostics::new(&source, &mut warnings);
            let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;
            assert!(rendered.pdf.starts_with(b"%PDF-"));
        }

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().and_then(|warning| warning.advice()),
            Some("The PDF will omit the header logo.")
        );
        Ok(())
    }

    #[test]
    fn timing_count_includes_only_resolved_document_images()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        std::fs::write(
            dir.path().join("resolved.png"),
            include_bytes!("../../terminal/images/simple.adoc.png"),
        )?;
        let parsed = acdc_parser::parse(
            "image::resolved.png[]\n\nimage::missing.png[]\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let rendered = processor.render_document(
            parsed.document(),
            Some(&dir.path().join("document.adoc")),
            &mut diagnostics,
        )?;
        assert_eq!(rendered.resolved_document_image_count, 1);
        assert_eq!(warnings.len(), 1);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn ambient_font_directories_are_not_read() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir()?;
        let fonts = dir.path().join("fonts");
        std::fs::create_dir(&fonts)?;
        std::fs::set_permissions(&fonts, std::fs::Permissions::from_mode(0o000))?;
        let parsed = acdc_parser::parse("A paragraph.\n", &acdc_parser::Options::default())?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let source_file = dir.path().join("document.adoc");

        let result =
            processor.render_document(parsed.document(), Some(&source_file), &mut diagnostics);
        std::fs::set_permissions(&fonts, std::fs::Permissions::from_mode(0o755))?;
        assert!(result?.pdf.starts_with(b"%PDF-"));
        Ok(())
    }

    fn typst_source(input: &str) -> Result<String, Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(input, &acdc_parser::Options::default())?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        Ok(processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?)
    }
}
