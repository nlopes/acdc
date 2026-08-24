//! PDF converter for `AsciiDoc` documents.
//!
//! This converter renders the acdc AST into Typst markup and delegates themed
//! preamble generation, asset resolution, font loading, and PDF compilation to
//! the shared `acdc-pdf-*` crates.
//!
//! Passthrough blocks render as unframed monospace text. Their content is
//! always escaped data and is never interpreted as Typst source.

use std::{
    cell::Cell,
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Write as _,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::SubsFlags;
use acdc_converters_core::{
    BackendTraits, Converter, Diagnostics, InlineTextTransform, Options, PrettyDuration,
    icon::{IconMode, image_source as icon_image_source},
    media::resolve_target,
    visitor::Visitor,
    xref::XrefGuard,
};
use acdc_parser::{
    Author, Block, BlockMetadata, CaptionKind, DelimitedBlock, DelimitedBlockType, Document,
    DocumentAttributes, InlineMacro, InlineNode, ListItem, Reference, SafeMode, Source, Table,
};
use acdc_pdf_images::{
    Error as ImageError, ImageMap, ResolveConfig, ResolveFailure, SourcePolicy, resolve,
};
use acdc_pdf_render::{RenderConfig, render_pdf};
use acdc_pdf_theme::Theme;
use acdc_pdf_typst::{
    DocumentLocale, DocumentMetadata, EmitOptions, Error as TypstError, Writer, preamble,
};
mod converter;
mod error;
mod index;
mod pdf_visitor;
mod visitor;

pub use acdc_pdf_typst::PageSize;
pub use error::Error;

/// Intrinsic traits for the PDF backend.
pub(crate) const BACKEND_TRAITS: BackendTraits =
    BackendTraits::new("pdf", "html", "pdf", ".pdf").with_htmlsyntax("html");

use pdf_visitor::{PdfVisitor, builtin_icon_glyph};

const MAX_THEME_FILE_BYTES: usize = 1024 * 1024;
const TYPST_RAW_SIZE_EM: f64 = 0.8;
const UNBREAKABLE_HELPER: &str = r"#let _acdc_unbreakable(body) = layout(size => {
  let kept = block(
    width: 100%,
    breakable: false,
    above: 0pt,
    below: 0pt,
    body,
  )
  if measure(kept, width: size.width).height <= size.height {
    kept
  } else {
    body
  }
})

";

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
    /// Cross-reference targets keyed by id, cloned from `Document::references`.
    /// Shared so a resolved target's reference text can be borrowed while the
    /// visitor renders it.
    references: Rc<HashMap<&'a str, Reference<'a>>>,
    /// Keeps a cross-reference inside a resolved target's text from recursing.
    pub(crate) xref_guard: XrefGuard,
    /// Fallback counter for example captions the parser did not number.
    pub(crate) example_counter: Rc<Cell<u32>>,
    /// Fallback counter for figure captions the parser did not number.
    pub(crate) figure_counter: Rc<Cell<u32>>,
    /// Fallback counter for listing and source captions the parser did not number.
    pub(crate) listing_counter: Rc<Cell<u32>>,
    /// Fallback counter for table captions the parser did not number.
    pub(crate) table_counter: Rc<Cell<u32>>,
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
        let preparation = collect_pdf_preparation(doc);
        self.emit_typst_source(
            doc,
            &assets,
            &theme,
            &emit_options,
            &preparation,
            diagnostics,
        )
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
        let preparation = collect_pdf_preparation(doc);
        Self::report_unsupported_font_icons(&preparation.unsupported_font_icons, diagnostics);
        let mut assets =
            self.resolve_images(doc, &preparation, source_file, spool.path(), diagnostics)?;
        let resolved_document_image_count = assets.images().count();
        let font_dirs = self.pdf_options.font_dirs.clone();
        let logo = self.resolve_logo(&mut assets, spool.path(), diagnostics)?;
        let asset_duration = asset_start.elapsed();

        let emit_start = Instant::now();
        let emit_options = self.emit_options(doc, logo, &font_dirs, diagnostics);
        let typst = self.emit_typst_source(
            doc,
            &assets,
            &theme,
            &emit_options,
            &preparation,
            diagnostics,
        )?;
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
        preparation: &PdfPreparation,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<String, Error> {
        let mut processor = Processor::new(self.options.clone(), doc.attributes.clone())
            .with_pdf_options(self.pdf_options.clone());
        processor.references = Rc::new(doc.references.clone());
        // The parser numbers every caption it resolved. These counters only number a title it
        // could not — one on a caller-built block, or added after parsing — so they start past
        // every assigned ordinal instead of colliding with one. Assigning rather than raising
        // keeps a repeated render deterministic.
        processor
            .example_counter
            .set(doc.highest_caption_number(CaptionKind::Example));
        processor
            .figure_counter
            .set(doc.highest_caption_number(CaptionKind::Figure));
        processor
            .listing_counter
            .set(doc.highest_caption_number(CaptionKind::Listing));
        processor
            .table_counter
            .set(doc.highest_caption_number(CaptionKind::Table));
        let mut visitor = PdfVisitor::new(
            processor,
            assets,
            theme,
            page_width_points(emit_options.page),
            code_wrap_columns(theme, emit_options.page),
            doc.toc_entries.clone(),
            diagnostics.reborrow(),
        )
        .with_populated_index_sections(preparation.populated_index_sections.clone());
        preamble::write(&mut visitor.writer, theme, emit_options);
        if preparation.has_unbreakable_blocks {
            visitor.writer.raw(UNBREAKABLE_HELPER);
        }
        if preparation.has_autofit_blocks {
            write_autofit_helper(&mut visitor.writer, theme);
        }
        visitor.visit_document(doc)?;
        let mut source = visitor.writer.into_string();
        source.truncate(source.trim_end_matches('\n').len());
        source.push('\n');
        Ok(source)
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
        let metadata = document_metadata(doc);
        let running_header_title = self
            .pdf_options
            .title
            .clone()
            .or_else(|| metadata.title.clone());
        EmitOptions {
            metadata,
            locale: document_locale(doc, diagnostics),
            page: self.page_size(doc, diagnostics),
            plain: self.pdf_options.plain,
            brand_fonts: !font_dirs.is_empty(),
            running_header_title,
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
        preparation: &PdfPreparation,
        source_file: Option<&Path>,
        spool_dir: &Path,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<ImageMap, Error> {
        if preparation.image_urls.is_empty() {
            return Ok(ImageMap::new());
        }
        let base_dir = base_dir_for_source(source_file);
        let source_policy = image_source_policy(
            self.options.safe_mode(),
            doc.attributes.contains_key("allow-uri-read"),
        );
        let mut config = ResolveConfig::new(base_dir, spool_dir);
        config.source_policy = source_policy;
        let url_refs: Vec<&str> = preparation.image_urls.iter().map(String::as_str).collect();
        let resolved = resolve(&url_refs, &config);
        self.report_image_failures(preparation, resolved.failures, diagnostics)?;
        Ok(resolved.assets)
    }

    fn report_unsupported_font_icons(icon_names: &[String], diagnostics: &mut Diagnostics<'_>) {
        for icon_name in icon_names {
            diagnostics.warn_with_advice(
                format!("{icon_name} is not a valid icon name in the built-in icon set"),
                "The PDF will use the icon's alternative text. Use a supported built-in icon or switch to image icon mode and provide the icon file.",
            );
        }
    }

    fn report_image_failures(
        &self,
        preparation: &PdfPreparation,
        failures: Vec<ResolveFailure>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Error> {
        if failures.is_empty() {
            return Ok(());
        }
        if self.pdf_options.strict_assets {
            return self.report_asset_failures(
                "image",
                "render fallback text for that image",
                failures,
                diagnostics,
            );
        }

        let failures_by_url = failures
            .iter()
            .map(|failure| (failure.url.as_str(), &failure.error))
            .collect::<HashMap<_, _>>();
        for failure in &failures {
            if preparation.ordinary_image_urls.contains(&failure.url) {
                diagnostics.warn_with_advice(
                    format!(
                        "image {} could not be embedded: {}",
                        failure.url, failure.error
                    ),
                    "The PDF will render fallback text for that image.",
                );
            }
        }
        for icon in &preparation.image_icons {
            if let Some(error) = failures_by_url.get(icon.source.as_str()) {
                diagnostics.warn_with_advice(
                    format!(
                        "image icon for '{}' not found or not readable at {}: {error}",
                        icon.target, icon.source
                    ),
                    format!("The PDF will render [{}] instead.", icon.target),
                );
            }
        }
        Ok(())
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

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "validated theme dimensions are clamped before conversion"
)]
fn code_wrap_columns(theme: &Theme, page: PageSize) -> usize {
    const CM_TO_PT: f64 = 72.0 / 2.54;
    const MONOSPACE_CELL_WIDTH_EM: f64 = 0.6;

    let page_width_pt = page_width_points(page);
    let content_width_pt = page_width_pt
        - 2.0 * theme.spacing.margin_x_cm * CM_TO_PT
        - 2.0 * theme.spacing.code_pad_pt;
    let cell_width_pt =
        theme.typography.body_size_pt * theme.typography.code_size_em * MONOSPACE_CELL_WIDTH_EM;
    (content_width_pt / cell_width_pt)
        .floor()
        .clamp(20.0, 160.0) as usize
}

fn write_autofit_helper(writer: &mut Writer, theme: &Theme) {
    let code_measure_size = theme.typography.code_size_em / TYPST_RAW_SIZE_EM;
    let minimum_scale = theme.typography.code_min_size_em / theme.typography.code_size_em;
    let code_padding = 2.0 * theme.spacing.code_pad_pt;
    let _ = writeln!(
        writer,
        r#"#let _acdc_autofit_code(source, body, language: none, extra-width: 0em) = layout(size => {{
  let available = calc.max(0pt, size.width - {code_padding}pt)
  let decoration-width = measure(h(extra-width)).width
  let widest = source.split("\n").map(line => {{
    let code = if language == none {{ raw(line) }} else {{ raw(line, lang: language) }}
    measure(text(size: {code_measure_size}em, code)).width + decoration-width
  }}).fold(0pt, calc.max)
  let scale = if widest > available {{ available / widest }} else {{ 1.0 }}
  text(size: calc.max({minimum_scale}, scale) * 1em, body)
}})
"#,
    );
}

const fn page_width_points(page: PageSize) -> f64 {
    match page {
        PageSize::A4 => 595.276,
        PageSize::Letter => 612.0,
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

fn document_metadata(doc: &Document<'_>) -> DocumentMetadata {
    let title = doc
        .header
        .as_ref()
        .and_then(|header| {
            let transform = InlineTextTransform::default().decode_char_refs(true);
            let mut title = transform.to_string(header.title.as_ref());
            if let Some(subtitle) = &header.subtitle {
                let subtitle = transform.to_string(subtitle.as_ref());
                if !subtitle.is_empty() {
                    title.push_str(": ");
                    title.push_str(&subtitle);
                }
            }
            (!title.is_empty()).then_some(title)
        })
        .or_else(|| {
            doc.attributes
                .get_string("untitled-label")
                .map(std::borrow::Cow::into_owned)
        });
    let mut authors = doc
        .header
        .iter()
        .flat_map(|header| &header.authors)
        .map(author_name)
        .collect::<Vec<_>>();
    if authors.is_empty()
        && let Some(author) = ["authors", "author"]
            .into_iter()
            .find_map(|name| doc.attributes.get_string(name))
    {
        authors.push(author.into_owned());
    }

    DocumentMetadata {
        title,
        authors,
        description: metadata_attribute(&doc.attributes, "subject"),
        keywords: metadata_attribute(&doc.attributes, "keywords"),
    }
}

fn document_locale(doc: &Document<'_>, diagnostics: &mut Diagnostics<'_>) -> DocumentLocale {
    let Some(value) = doc.attributes.get_string("lang") else {
        return DocumentLocale::default();
    };
    if value.is_empty() {
        return DocumentLocale::default();
    }
    let error = match parse_document_locale(&value) {
        Ok(locale) => return locale,
        Err(error) => error,
    };

    diagnostics.warn_with_advice(
        format!("{error}; using English for PDF text"),
        "Set `:lang:` to a two- or three-letter language code, optionally followed by `-` or `_` and a two-letter region, for example `pt-BR`.",
    );
    DocumentLocale::default()
}

fn parse_document_locale(value: &str) -> Result<DocumentLocale, TypstError> {
    let (language, region) = value
        .split_once(['-', '_'])
        .map_or((value, None), |(language, region)| (language, Some(region)));
    DocumentLocale::try_from_codes(language, region)
}

fn metadata_attribute(attributes: &DocumentAttributes<'_>, name: &str) -> Option<String> {
    attributes.get_string(name).map_or_else(
        || {
            attributes
                .get(name)
                .is_some_and(|value| matches!(value, acdc_parser::AttributeValue::Bool(true)))
                .then(String::new)
        },
        |value| Some(value.into_owned()),
    )
}

fn author_name(author: &Author<'_>) -> String {
    [
        Some(author.first_name),
        author.middle_name,
        Some(author.last_name),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" ")
}

fn collect_pdf_preparation(doc: &Document<'_>) -> PdfPreparation {
    let mut preparation = PdfPreparation::default();
    let context = PreparationContext {
        attributes: &doc.attributes,
        icon_mode: IconMode::from(&doc.attributes),
    };
    if let Some(header) = &doc.header {
        collect_inline_preparation(header.title.as_ref(), &context, &mut preparation);
        if let Some(subtitle) = &header.subtitle {
            collect_inline_preparation(subtitle.as_ref(), &context, &mut preparation);
        }
    }
    collect_block_preparation(&doc.blocks, &context, &mut preparation);
    preparation
}

struct PreparationContext<'attributes, 'source> {
    attributes: &'attributes DocumentAttributes<'source>,
    icon_mode: IconMode,
}

#[derive(Default)]
struct PdfPreparation {
    image_urls: BTreeSet<String>,
    ordinary_image_urls: BTreeSet<String>,
    image_icons: Vec<ImageIconReference>,
    unsupported_font_icons: Vec<String>,
    has_index_terms: bool,
    has_unbreakable_blocks: bool,
    has_autofit_blocks: bool,
    populated_index_sections: HashSet<String>,
}

struct ImageIconReference {
    source: String,
    target: String,
}

impl PdfPreparation {
    fn insert_image(&mut self, source: String) {
        self.image_urls.insert(source.clone());
        self.ordinary_image_urls.insert(source);
    }

    fn insert_image_icon(&mut self, source: String, target: &Source<'_>) {
        self.image_urls.insert(source.clone());
        self.image_icons.push(ImageIconReference {
            source,
            target: target.to_string(),
        });
    }
}

fn collect_block_preparation(
    blocks: &[Block<'_>],
    context: &PreparationContext<'_, '_>,
    preparation: &mut PdfPreparation,
) {
    for block in blocks {
        match block {
            Block::Section(section) => {
                if section.kind == acdc_parser::SectionKind::Index {
                    collect_inline_preparation(section.title.as_ref(), context, preparation);
                    if preparation.has_index_terms {
                        preparation.populated_index_sections.insert(
                            acdc_parser::Section::generate_id_string(
                                &section.metadata,
                                section.title.as_ref(),
                            ),
                        );
                    }
                    continue;
                }
                collect_inline_preparation(section.title.as_ref(), context, preparation);
                collect_block_preparation(&section.content, context, preparation);
            }
            Block::Paragraph(paragraph) => {
                preparation.has_unbreakable_blocks |= is_unbreakable_paragraph(paragraph);
                preparation.has_autofit_blocks |=
                    is_autofit_paragraph(paragraph, context.attributes);
                collect_inline_preparation(paragraph.title.as_ref(), context, preparation);
                collect_metadata_preparation(&paragraph.metadata, context, preparation);
                collect_inline_preparation(&paragraph.content, context, preparation);
            }
            Block::DelimitedBlock(block) => {
                preparation.has_unbreakable_blocks |= is_unbreakable_delimited_block(block);
                preparation.has_autofit_blocks |=
                    is_autofit_delimited_block(block, context.attributes);
                collect_inline_preparation(block.title.as_ref(), context, preparation);
                collect_metadata_preparation(&block.metadata, context, preparation);
                collect_delimited_block_preparation(&block.inner, context, preparation);
            }
            Block::OrderedList(list) => {
                collect_inline_preparation(list.title.as_ref(), context, preparation);
                for item in &list.items {
                    collect_list_item_preparation(item, context, preparation);
                }
            }
            Block::UnorderedList(list) => {
                collect_inline_preparation(list.title.as_ref(), context, preparation);
                for item in &list.items {
                    collect_list_item_preparation(item, context, preparation);
                }
            }
            Block::DescriptionList(list) => {
                collect_inline_preparation(list.title.as_ref(), context, preparation);
                for item in &list.items {
                    collect_inline_preparation(&item.term, context, preparation);
                    collect_inline_preparation(&item.principal_text, context, preparation);
                    collect_block_preparation(&item.description, context, preparation);
                }
            }
            Block::CalloutList(list) => {
                collect_inline_preparation(list.title.as_ref(), context, preparation);
                for item in &list.items {
                    collect_inline_preparation(&item.principal, context, preparation);
                    collect_block_preparation(&item.blocks, context, preparation);
                }
            }
            Block::Admonition(admonition) => {
                preparation.has_unbreakable_blocks |=
                    admonition.metadata.options.contains(&"unbreakable");
                collect_inline_preparation(admonition.title.as_ref(), context, preparation);
                collect_block_preparation(&admonition.blocks, context, preparation);
            }
            Block::Image(image) => {
                collect_inline_preparation(image.title.as_ref(), context, preparation);
                preparation.insert_image(resolve_target(
                    &image.source.to_string(),
                    context.attributes,
                ));
            }
            Block::DiscreteHeader(header) => {
                collect_inline_preparation(header.title.as_ref(), context, preparation);
            }
            Block::Audio(audio) => {
                collect_inline_preparation(audio.title.as_ref(), context, preparation);
            }
            Block::Video(video) => {
                collect_inline_preparation(video.title.as_ref(), context, preparation);
                if let Some(poster) = video
                    .metadata
                    .attributes
                    .get_string("poster")
                    .filter(|poster| !poster.is_empty())
                {
                    preparation.insert_image(resolve_target(&poster, context.attributes));
                }
            }
            Block::TableOfContents(_)
            | Block::DocumentAttribute(_)
            | Block::ThematicBreak(_)
            | Block::PageBreak(_)
            | Block::Comment(_)
            | _ => {}
        }
    }
}

fn is_unbreakable_paragraph(paragraph: &acdc_parser::Paragraph<'_>) -> bool {
    paragraph.metadata.options.contains(&"unbreakable")
        && matches!(
            paragraph.metadata.style,
            Some("example" | "listing" | "literal" | "quote" | "sidebar" | "source" | "verse")
        )
}

fn is_unbreakable_delimited_block(block: &DelimitedBlock<'_>) -> bool {
    block.metadata.options.contains(&"unbreakable")
        && matches!(
            block.inner,
            DelimitedBlockType::DelimitedExample(_)
                | DelimitedBlockType::DelimitedListing(_)
                | DelimitedBlockType::DelimitedLiteral(_)
                | DelimitedBlockType::DelimitedOpen(_)
                | DelimitedBlockType::DelimitedQuote(_)
                | DelimitedBlockType::DelimitedSidebar(_)
                | DelimitedBlockType::DelimitedStem(_)
                | DelimitedBlockType::DelimitedTable(_)
                | DelimitedBlockType::DelimitedVerse(_)
        )
}

fn is_autofit_paragraph(
    paragraph: &acdc_parser::Paragraph<'_>,
    attributes: &DocumentAttributes<'_>,
) -> bool {
    matches!(
        paragraph.metadata.style,
        Some("listing" | "literal" | "source")
    ) && has_autofit_option(&paragraph.metadata, attributes)
}

fn is_autofit_delimited_block(
    block: &DelimitedBlock<'_>,
    attributes: &DocumentAttributes<'_>,
) -> bool {
    matches!(
        block.inner,
        DelimitedBlockType::DelimitedListing(_) | DelimitedBlockType::DelimitedLiteral(_)
    ) && has_autofit_option(&block.metadata, attributes)
}

fn has_autofit_option(metadata: &BlockMetadata<'_>, attributes: &DocumentAttributes<'_>) -> bool {
    metadata.options.contains(&"autofit") || attributes.contains_key("autofit-option")
}

fn collect_delimited_block_preparation(
    block: &DelimitedBlockType<'_>,
    context: &PreparationContext<'_, '_>,
    preparation: &mut PdfPreparation,
) {
    match block {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            collect_block_preparation(blocks, context, preparation);
        }
        DelimitedBlockType::DelimitedTable(table) => {
            collect_table_preparation(table, context, preparation);
        }
        DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | DelimitedBlockType::DelimitedStem(_)
        | _ => {}
    }
}

fn collect_table_preparation(
    table: &Table<'_>,
    context: &PreparationContext<'_, '_>,
    preparation: &mut PdfPreparation,
) {
    for row in table
        .header
        .iter()
        .chain(table.rows.iter())
        .chain(table.footer.iter())
    {
        for column in &row.columns {
            collect_block_preparation(&column.content, context, preparation);
        }
    }
}

fn collect_list_item_preparation(
    item: &ListItem<'_>,
    context: &PreparationContext<'_, '_>,
    preparation: &mut PdfPreparation,
) {
    collect_inline_preparation(&item.principal, context, preparation);
    collect_block_preparation(&item.blocks, context, preparation);
}

fn collect_metadata_preparation(
    metadata: &BlockMetadata<'_>,
    context: &PreparationContext<'_, '_>,
    preparation: &mut PdfPreparation,
) {
    if let Some(attribution) = &metadata.attribution {
        collect_inline_preparation(attribution, context, preparation);
    }
    if let Some(citetitle) = &metadata.citetitle {
        collect_inline_preparation(citetitle, context, preparation);
    }
}

fn collect_inline_preparation(
    nodes: &[InlineNode<'_>],
    context: &PreparationContext<'_, '_>,
    preparation: &mut PdfPreparation,
) {
    for node in nodes {
        match node {
            InlineNode::BoldText(text) => {
                collect_inline_preparation(&text.content, context, preparation);
            }
            InlineNode::ItalicText(text) => {
                collect_inline_preparation(&text.content, context, preparation);
            }
            InlineNode::MonospaceText(text) => {
                collect_inline_preparation(&text.content, context, preparation);
            }
            InlineNode::HighlightText(text) => {
                collect_inline_preparation(&text.content, context, preparation);
            }
            InlineNode::SubscriptText(text) => {
                collect_inline_preparation(&text.content, context, preparation);
            }
            InlineNode::SuperscriptText(text) => {
                collect_inline_preparation(&text.content, context, preparation);
            }
            InlineNode::CurvedQuotationText(text) => {
                collect_inline_preparation(&text.content, context, preparation);
            }
            InlineNode::CurvedApostropheText(text) => {
                collect_inline_preparation(&text.content, context, preparation);
            }
            InlineNode::Macro(InlineMacro::Image(image)) => {
                preparation.insert_image(resolve_target(
                    &image.source.to_string(),
                    context.attributes,
                ));
            }
            InlineNode::Macro(InlineMacro::Icon(icon)) => match context.icon_mode {
                IconMode::Image => preparation.insert_image_icon(
                    icon_image_source(context.attributes, &icon.target),
                    &icon.target,
                ),
                IconMode::Font => {
                    let icon_name = icon.target.to_string();
                    if builtin_icon_glyph(&icon_name).is_none() {
                        preparation.unsupported_font_icons.push(icon_name);
                    }
                }
                IconMode::Text | _ => {}
            },
            InlineNode::Macro(InlineMacro::Footnote(footnote)) => {
                collect_inline_preparation(&footnote.content, context, preparation);
            }
            InlineNode::Macro(InlineMacro::Url(url)) => {
                collect_inline_preparation(&url.text, context, preparation);
            }
            InlineNode::Macro(InlineMacro::Link(link)) => {
                collect_inline_preparation(&link.text, context, preparation);
            }
            InlineNode::Macro(InlineMacro::Mailto(mailto)) => {
                collect_inline_preparation(&mailto.text, context, preparation);
            }
            InlineNode::Macro(InlineMacro::CrossReference(xref)) => {
                collect_inline_preparation(&xref.text, context, preparation);
            }
            InlineNode::Macro(InlineMacro::IndexTerm(term)) => {
                preparation.has_index_terms = true;
                collect_inline_preparation(term.term(), context, preparation);
                if let Some(secondary) = term.secondary() {
                    collect_inline_preparation(secondary, context, preparation);
                }
                if let Some(tertiary) = term.tertiary() {
                    collect_inline_preparation(tertiary, context, preparation);
                }
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

fn encode_bibliography_reference_label(value: &str) -> String {
    format!("bibref-{}", encode_label(value))
}

fn encode_footnote_label(value: &str) -> String {
    encode_label(&format!("footnote:{value}"))
}

#[cfg(test)]
mod tests {
    use acdc_converters_core::{Converter, Warning, WarningSource};
    use acdc_parser::{DelimitedBlock, Image, Location, Paragraph, Plain, Title};
    use tempfile::NamedTempFile;

    use super::*;

    fn title(content: &'static str) -> Title<'static> {
        Title::new(vec![InlineNode::PlainText(Plain {
            content,
            location: Location::default(),
            escaped: false,
        })])
    }

    fn render_warnings(input: &str) -> Result<Vec<Warning>, Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(input, &acdc_parser::Options::default())?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        {
            let mut diagnostics = Diagnostics::new(&source, &mut warnings);
            let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;
            assert!(rendered.pdf.starts_with(b"%PDF-"));
        }
        Ok(warnings)
    }

    fn rendered_page_count(input: &str) -> Result<usize, Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(input, &acdc_parser::Options::default())?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;
        let rendered = render_pdf(&typst, &ImageMap::new(), &RenderConfig::default())?;
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;

        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(pdf.get_pages().len())
    }

    fn rendered_page_texts(input: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(input, &acdc_parser::Options::default())?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone())
            .with_pdf_options(PdfOptions {
                plain: true,
                ..PdfOptions::default()
            });
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        let pages = pdf
            .get_pages()
            .into_keys()
            .map(|page| pdf.extract_text(&[page]))
            .collect::<Result<Vec<_>, _>>()?;

        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(pages)
    }

    #[test]
    fn ordered_list_reversed_option_reverses_typst_enum() -> Result<(), Box<dyn std::error::Error>>
    {
        let parsed = acdc_parser::parse(
            "[%reversed,start=5]\n. Five\n. Four\n. Three\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;

        assert!(typst.contains(", start: 5, reversed: true)"));
        let rendered = render_pdf(&typst, &ImageMap::new(), &RenderConfig::default())?;
        assert!(rendered.pdf.starts_with(b"%PDF-"));
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn page_break_always_option_controls_blank_pages() -> Result<(), Box<dyn std::error::Error>> {
        let default_breaks = "= Default page breaks\n\nFirst page.\n\n<<<\n\n<<<\n\nSecond page.\n";
        let forced_break =
            "= Forced page break\n\nFirst page.\n\n<<<\n\n[%always]\n<<<\n\nSecond page.\n";
        let separated_breaks = "= Separated page breaks\n\nFirst page.\n\n<<<\n\nSecond page.\n\n[%always]\n<<<\n\nThird page.\n";

        let parsed = acdc_parser::parse(forced_break, &acdc_parser::Options::default())?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;

        assert!(
            typst.contains("#pagebreak(weak: true)\n\n#pagebreak()\n#pagebreak()"),
            "{typst}"
        );

        assert_eq!(rendered_page_count(default_breaks)?, 2);
        assert_eq!(rendered_page_count(forced_break)?, 3);
        assert_eq!(rendered_page_count(separated_breaks)?, 3);
        Ok(())
    }

    #[test]
    fn unbreakable_option_wraps_only_supported_pdf_block_contexts()
    -> Result<(), Box<dyn std::error::Error>> {
        let supported = acdc_parser::parse(
            "[options=unbreakable]\n----\nlisting\n----\n\n[source%unbreakable]\n----\nsource\n----\n\n[%unbreakable]\n....\nliteral\n....\n\n[%unbreakable]\n====\nexample\n====\n\n[%collapsible%unbreakable]\n====\ncollapsible example\n====\n\n[%unbreakable]\n--\nopen\n--\n\n[quote%unbreakable]\n____\nquote\n____\n\n[verse%unbreakable]\n____\nverse\n____\n\n[%unbreakable]\n****\nsidebar\n****\n\n[stem%unbreakable]\n++++\nx\n++++\n\n[%unbreakable]\n|===\n|table\n|===\n\n[%unbreakable]\nNOTE: admonition\n\n[source%unbreakable]\nsource paragraph\n\n[listing%unbreakable]\nlisting paragraph\n\n[literal%unbreakable]\nliteral paragraph\n\n[example%unbreakable]\nexample paragraph\n\n[quote%unbreakable]\nquote paragraph\n\n[verse%unbreakable]\nverse paragraph\n\n[sidebar%unbreakable]\nsidebar paragraph\n",
            &acdc_parser::Options::default(),
        )?;
        let unsupported = acdc_parser::parse(
            ":unbreakable-option:\n\n[%unbreakable]\nparagraph\n\n[%unbreakable]\n. list item\n\n[pass%unbreakable]\n++++\npassthrough\n++++\n\n----\nlisting without a local option\n----\n",
            &acdc_parser::Options::default(),
        )?;

        let render = |document: &Document<'_>| -> Result<String, Box<dyn std::error::Error>> {
            let processor = Processor::new(Options::default(), document.attributes.clone());
            let source = WarningSource::new("pdf");
            let mut warnings = Vec::new();
            let mut diagnostics = Diagnostics::new(&source, &mut warnings);
            let typst = processor.convert_to_typst_source(document, &mut diagnostics)?;
            Ok(typst)
        };

        let helper = "#let _acdc_unbreakable(body) = layout";
        let wrapper = "#_acdc_unbreakable[";
        let supported = render(supported.document())?;
        let unsupported = render(unsupported.document())?;

        assert!(supported.contains(helper));
        assert_eq!(supported.matches(wrapper).count(), 19);
        assert!(!unsupported.contains(helper));
        assert_eq!(unsupported.matches(wrapper).count(), 0);
        assert!(
            render_pdf(&supported, &ImageMap::new(), &RenderConfig::default())?
                .pdf
                .starts_with(b"%PDF-")
        );
        Ok(())
    }

    #[test]
    fn autofit_option_applies_only_to_listing_source_and_literal_contexts()
    -> Result<(), Box<dyn std::error::Error>> {
        let local = acdc_parser::parse(
            ".Listing caption\n[options=autofit]\n----\nlisting\n----\n\n[source%autofit]\n----\nsource\n----\n\n[%autofit]\n....\nliteral\n....\n\n[source%autofit]\nsource paragraph\n\n[listing%autofit]\nlisting paragraph\n\n[literal%autofit]\nliteral paragraph\n\n|===\na|\n[source%autofit]\n----\nnested source\n----\n|===\n",
            &acdc_parser::Options::default(),
        )?;
        let global = acdc_parser::parse(
            ":autofit-option:\n\n----\nlisting\n----\n\n[source]\n----\nsource\n----\n\n....\nliteral\n....\n\n[source]\nsource paragraph\n\n[listing]\nlisting paragraph\n\n[literal]\nliteral paragraph\n",
            &acdc_parser::Options::default(),
        )?;
        let unsupported = acdc_parser::parse(
            ":autofit-option:\n\nordinary paragraph\n\n[example]\nexample paragraph\n\n[quote]\nquote paragraph\n\n[verse]\nverse paragraph\n\n[sidebar]\nsidebar paragraph\n\n[pass]\n++++\npassthrough\n++++\n\n|===\n\nl|literal table cell\n|===\n",
            &acdc_parser::Options::default(),
        )?;

        let render = |document: &Document<'_>| -> Result<String, Box<dyn std::error::Error>> {
            let processor = Processor::new(Options::default(), document.attributes.clone());
            let source = WarningSource::new("pdf");
            let mut warnings = Vec::new();
            let mut diagnostics = Diagnostics::new(&source, &mut warnings);
            let typst = processor.convert_to_typst_source(document, &mut diagnostics)?;
            assert!(warnings.is_empty(), "{warnings:?}");
            Ok(typst)
        };

        let helper = "#let _acdc_autofit_code";
        let call = "#_acdc_autofit_code";
        let local = render(local.document())?;
        let global = render(global.document())?;
        let unsupported = render(unsupported.document())?;

        assert!(local.contains(helper), "{local}");
        assert_eq!(local.matches(call).count(), 7, "{local}");
        let caption = local
            .find("Listing caption")
            .ok_or_else(|| std::io::Error::other("missing listing caption in generated Typst"))?;
        let first_autofit = local
            .find(call)
            .ok_or_else(|| std::io::Error::other("missing autofit call in generated Typst"))?;
        assert!(caption < first_autofit, "{local}");
        assert!(global.contains(helper), "{global}");
        assert_eq!(global.matches(call).count(), 6, "{global}");
        assert_eq!(unsupported.matches(call).count(), 0, "{unsupported}");
        assert!(
            render_pdf(&local, &ImageMap::new(), &RenderConfig::default())?
                .pdf
                .starts_with(b"%PDF-")
        );
        assert!(
            render_pdf(&global, &ImageMap::new(), &RenderConfig::default())?
                .pdf
                .starts_with(b"%PDF-")
        );
        Ok(())
    }

    #[test]
    fn autofit_preserves_original_lines_and_keeps_minimum_size_fallback_breakable()
    -> Result<(), Box<dyn std::error::Error>> {
        let medium_line = format!("medium-{}-end", "0123456789".repeat(10));
        let oversized_line = format!("oversized-{}-end", "abcdefghij".repeat(40));
        let input = format!(
            ":source-highlighter: rouge\n\n[source%autofit,rust,linenums,start=98,highlight=99]\n----\n{medium_line} <1>\nshort line\nthird line\n----\n<1> Callout explanation.\n\n[%autofit]\n....\n{oversized_line}\n....\n"
        );
        let parsed = acdc_parser::parse(&input, &acdc_parser::Options::default())?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;

        assert!(typst.contains(&medium_line), "{typst}");
        assert!(typst.contains(&oversized_line), "{typst}");
        assert!(typst.contains("language: \"rust\""), "{typst}");
        assert!(typst.contains("extra-width: 2.6em"), "{typst}");
        assert!(typst.contains(&format!("{medium_line} (1)")), "{typst}");
        assert_eq!(typst.matches("#_acdc_autofit_code").count(), 2, "{typst}");
        assert!(
            render_pdf(&typst, &ImageMap::new(), &RenderConfig::default())?
                .pdf
                .starts_with(b"%PDF-")
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn autofit_uses_theme_code_sizes_and_padding() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "[%autofit]\n----\na code line that requires the autofit helper\n----\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let mut theme = Theme::default();
        theme.typography.code_size_em = 0.9;
        theme.typography.code_min_size_em = 0.45;
        theme.spacing.code_pad_pt = 12.5;
        let assets = ImageMap::new();
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let emit_options = processor.emit_options(parsed.document(), None, &[], &mut diagnostics);

        let typst = processor.emit_typst_source(
            parsed.document(),
            &assets,
            &theme,
            &emit_options,
            &collect_pdf_preparation(parsed.document()),
            &mut diagnostics,
        )?;

        assert!(typst.contains("text(size: 1.125em, fill:"), "{typst}");
        assert!(typst.contains("size.width - 25pt"), "{typst}");
        assert!(typst.contains("calc.max(0.5, scale)"), "{typst}");
        assert!(
            render_pdf(&typst, &assets, &RenderConfig::default())?
                .pdf
                .starts_with(b"%PDF-")
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn unbreakable_option_moves_fitting_listing_and_preserves_oversized_listing()
    -> Result<(), Box<dyn std::error::Error>> {
        let prefix = "= ACDC Move probe\n\nFILL01 filler paragraph.\n\nFILL02 filler paragraph.\n\nFILL03 filler paragraph.\n\nFILL04 filler paragraph.\n\nFILL05 filler paragraph.\n\nFILL06 filler paragraph.\n\nFILL07 filler paragraph.\n\nFILL08 filler paragraph.\n\nFILL09 filler paragraph.\n\nFILL10 filler paragraph.\n\nFILL11 filler paragraph.\n\nFILL12 filler paragraph.\n\nFILL13 filler paragraph.\n\nFILL14 filler paragraph.\n\nFILL15 filler paragraph.\n\nFILL16 filler paragraph.\n\nFILL17 filler paragraph.\n\nFILL18 filler paragraph.\n\nFILL19 filler paragraph.\n\nFILL20 filler paragraph.\n\n.Target caption\n[listing%unbreakable]\n----\n";
        let fitting = format!(
            "{prefix}CODE01 target line\nCODE02 target line\nCODE03 target line\nCODE04 target line\nCODE05 target line\nCODE06 target line\nCODE07 target line\nCODE08 target line\n----\n\nAFTER target.\n"
        );
        let fitting_pages = rendered_page_texts(&fitting)?;
        let first_page = fitting_pages
            .first()
            .ok_or_else(|| std::io::Error::other("missing first page"))?;
        let second_page = fitting_pages
            .get(1)
            .ok_or_else(|| std::io::Error::other("missing second page"))?;

        assert!(first_page.contains("FILL20"), "{fitting_pages:?}");
        assert!(!first_page.contains("Target caption"), "{fitting_pages:?}");
        assert!(second_page.contains("Target caption"), "{fitting_pages:?}");
        assert!(second_page.contains("CODE01"), "{fitting_pages:?}");
        assert!(second_page.contains("CODE08"), "{fitting_pages:?}");

        let mut oversized = String::from(prefix);
        for line in 1..=80 {
            let _ = writeln!(oversized, "CODE{line:02} target line");
        }
        oversized.push_str("----\n\nAFTER target.\n");
        let oversized_pages = rendered_page_texts(&oversized)?;
        let oversized_first_page = oversized_pages
            .first()
            .ok_or_else(|| std::io::Error::other("missing first page"))?;

        assert!(oversized_pages.len() > 1, "{oversized_pages:?}");
        assert!(
            oversized_first_page.contains("CODE01"),
            "{oversized_pages:?}"
        );
        assert!(
            oversized_pages
                .iter()
                .skip(1)
                .any(|page| page.contains("CODE80")),
            "{oversized_pages:?}"
        );
        Ok(())
    }

    #[test]
    fn unhandled_parser_block_warning_is_structured() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse("text", &acdc_parser::Options::default())?;
        let block = parsed
            .document()
            .blocks
            .first()
            .ok_or_else(|| std::io::Error::other("missing parsed block"))?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let assets = ImageMap::new();
        let theme = Theme::default();
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        {
            let diagnostics = Diagnostics::new(&source, &mut warnings);
            let mut visitor = PdfVisitor::new(
                processor,
                &assets,
                &theme,
                page_width_points(PageSize::A4),
                code_wrap_columns(&theme, PageSize::A4),
                Vec::new(),
                diagnostics,
            );
            visitor.visit_unhandled_block(block)?;
        }

        assert_eq!(warnings.len(), 1);
        let warning = warnings
            .first()
            .ok_or_else(|| std::io::Error::other("missing parser variant warning"))?;
        assert_eq!(
            warning.message,
            "an unsupported parser block variant was omitted from PDF output"
        );
        assert_eq!(
            warning.advice(),
            Some(
                "Use the HTML backend or Asciidoctor PDF for this document and report the unsupported construct."
            )
        );
        Ok(())
    }

    fn external_link_targets(pdf: &lopdf::Document) -> Vec<String> {
        let mut targets = pdf
            .objects
            .values()
            .filter_map(|object| {
                let link = object.as_dict().ok()?;
                if !matches!(link.get(b"Subtype"), Ok(lopdf::Object::Name(name)) if name == b"Link")
                {
                    return None;
                }
                let action = link.get(b"A").ok()?.as_dict().ok()?;
                let uri = action.get(b"URI").ok()?.as_str().ok()?;
                Some(String::from_utf8_lossy(uri).into_owned())
            })
            .collect::<Vec<_>>();
        targets.sort();
        targets
    }

    fn pdf_contains_outline_title(pdf: &lopdf::Document, expected: &str) -> bool {
        pdf.objects.values().any(|object| {
            let Ok(dictionary) = object.as_dict() else {
                return false;
            };
            let Ok(title) = dictionary.get(b"Title").and_then(lopdf::Object::as_str) else {
                return false;
            };
            String::from_utf8_lossy(title) == expected
        })
    }

    #[test]
    fn index_notitle_hides_heading_and_catalog_uses_default_columns()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Index columns\n\nVisible ((alpha)) and ((beta)).\n\n[index%notitle]\n== Hidden Index\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;

        assert!(!typst.contains("#heading(level: 1)[#text(\"Hidden Index\")]"));
        assert!(typst.contains(&format!(
            "#metadata(none) <{}>",
            encode_label("_hidden_index")
        )));
        assert!(typst.contains("#columns(2, gutter: 12pt)["));
        let rendered = render_pdf(&typst, &ImageMap::new(), &RenderConfig::default())?;
        assert!(rendered.pdf.starts_with(b"%PDF-"));
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn section_notitle_hides_only_the_body_heading() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Section metadata\n:toc:\n:sectnums:\n\nSee <<hidden-section>>.\n\n[#hidden-section%notitle]\n== Hidden *Section*\n\nHidden section body.\n\n=== Child Section\n\nChild body.\n\n[discrete#visible-discrete%notitle]\n=== Visible Discrete\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;
        let label = encode_label("hidden-section");

        assert!(
            typst.contains(&format!("#_acdc_toc_entry(<{label}>, 0,")),
            "{typst}"
        );
        assert!(typst.contains(&format!("#link(<{label}>)[")), "{typst}");
        assert!(
            typst.contains(&format!(
                "#place[#hide[#heading(level: 1)[#text(\"1. \")#text(\"Hidden \")#strong[#text(\"Section\")]] <{label}>]]"
            )),
            "{typst}"
        );
        assert!(
            !typst.contains(&format!("\n#heading(level: 1)[#text(\"1. \")#text(\"Hidden \")#strong[#text(\"Section\")]] <{label}>")),
            "{typst}"
        );
        assert!(
            typst.contains("#heading(level: 2)[#text(\"1.1. \")#text(\"Child Section\")]"),
            "{typst}"
        );
        assert!(
            typst.contains("#heading(level: 2, outlined: false)[#text(\"Visible Discrete\")]"),
            "{typst}"
        );

        let rendered = render_pdf(&typst, &ImageMap::new(), &RenderConfig::default())?;
        assert!(rendered.pdf.starts_with(b"%PDF-"));
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;
        let normalized_text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(
            normalized_text.matches("1. Hidden Section").count(),
            1,
            "{normalized_text}"
        );
        assert!(text.contains("Visible Discrete"), "{text}");

        assert!(pdf_contains_outline_title(&pdf, "1. Hidden Section"));
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn lead_role_and_automatic_preamble_lead_use_larger_text()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Lead paragraphs\n\nAutomatic lead paragraph.\n\nSecond preamble paragraph.\n\n== Section\n\nNormal section paragraph.\n\n[.lead]\nExplicit lead paragraph.\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;

        assert!(
            typst.contains("#text(size: 1.25em)[#text(\"Automatic lead paragraph.\")]"),
            "{typst}"
        );
        assert!(
            typst.contains("#text(size: 1.25em)[#text(\"Explicit lead paragraph.\")]"),
            "{typst}"
        );
        assert!(
            !typst.contains("#text(size: 1.25em)[#text(\"Second preamble paragraph.\")]"),
            "{typst}"
        );
        assert!(
            !typst.contains("#text(size: 1.25em)[#text(\"Normal section paragraph.\")]"),
            "{typst}"
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let rendered = render_pdf(&typst, &ImageMap::new(), &RenderConfig::default())?;
        assert!(rendered.pdf.starts_with(b"%PDF-"));

        let parsed = acdc_parser::parse(
            "= Existing role\n\n[.other]\nRole prevents automatic lead.\n\n== Section\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;

        assert!(
            !typst.contains("#text(size: 1.25em)[#text(\"Role prevents automatic lead.\")]"),
            "{typst}"
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn index_catalog_uses_custom_theme_columns_and_gap() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "Visible ((alpha)) and ((beta)).\n\n[index]\n== Index\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let mut theme = Theme::default();
        theme.index.columns = 3;
        theme.index.column_gap_pt = None;
        theme.typography.body_size_pt = 12.5;
        let assets = ImageMap::new();
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let emit_options = processor.emit_options(parsed.document(), None, &[], &mut diagnostics);

        let typst_with_default_gap = processor.emit_typst_source(
            parsed.document(),
            &assets,
            &theme,
            &emit_options,
            &collect_pdf_preparation(parsed.document()),
            &mut diagnostics,
        )?;
        assert!(typst_with_default_gap.contains("#columns(3, gutter: 12.5pt)["));

        theme.index.column_gap_pt = Some(18.5);
        let typst = processor.emit_typst_source(
            parsed.document(),
            &assets,
            &theme,
            &emit_options,
            &collect_pdf_preparation(parsed.document()),
            &mut diagnostics,
        )?;
        assert!(typst.contains("#columns(3, gutter: 18.5pt)["));
        let rendered = render_pdf(&typst, &assets, &RenderConfig::default())?;
        assert!(rendered.pdf.starts_with(b"%PDF-"));

        theme.index.columns = 1;
        let single_column_typst = processor.emit_typst_source(
            parsed.document(),
            &assets,
            &theme,
            &emit_options,
            &collect_pdf_preparation(parsed.document()),
            &mut diagnostics,
        )?;
        assert!(!single_column_typst.contains("#columns("));
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn print_media_index_uses_plain_unique_page_ranges() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Print index\n:media: print\n:index-pagenum-sequence-style: page\n\nFirst ((term)) and same-page ((term)).\n\n<<<\n\nSecond ((term)).\n\n<<<\n\nA page without the indexed term.\n\n<<<\n\nFourth ((term)).\n\n[index]\n== Index\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;

        assert!(typst.contains(
            "#_acdc_index_pages((<__indexterm-1>,<__indexterm-2>,<__indexterm-3>,<__indexterm-4>,), \"print\")"
        ));
        let rendered = render_pdf(&typst, &ImageMap::new(), &RenderConfig::default())?;
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;
        let normalized_text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(normalized_text.contains("T term , 1-2, 4"), "{text}");
        let link_annotations = pdf
            .objects
            .values()
            .filter(|object| {
                object.as_dict().is_ok_and(|dictionary| {
                    matches!(dictionary.get(b"Subtype"), Ok(lopdf::Object::Name(name)) if name == b"Link")
                })
            })
            .count();
        assert_eq!(link_annotations, 0);
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn audio_blocks_render_clickable_static_fallbacks() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Audio fallbacks\n:imagesdir: media library\n\n.Local episode\n[[local-audio]]\naudio::clips/demo episode.mp3[]\n\naudio::https://example.com/podcast episode.mp3[start=5,end=10,opts=\"autoplay,loop,nocontrols\"]\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;

        assert!(typst.contains(
            "#text(\"►\u{a0}\")#link(\"media%20library/clips/demo%20episode.mp3\")[#text(\"media%20library/clips/demo%20episode.mp3\")]#text(\" \")#emph[#text(\"(audio)\")]"
        ));
        assert!(typst.contains(
            "#text(\"►\u{a0}\")#link(\"https://example.com/podcast%20episode.mp3\")[#text(\"https://example.com/podcast%20episode.mp3\")]#text(\" \")#emph[#text(\"(audio)\")]"
        ));
        assert!(typst.contains("#imagecaption[#text(\"Local episode\")]"));
        assert_eq!(warnings.len(), 1);
        let warning = warnings
            .first()
            .ok_or_else(|| std::io::Error::other("missing static media fallback warning"))?;
        assert_eq!(
            warning.message,
            "interactive media playback is unavailable in static PDF output; rendering clickable source links",
        );
        assert_eq!(
            warning.advice(),
            Some("Use the HTML backend when in-document playback is required."),
        );

        let rendered = render_pdf(&typst, &ImageMap::new(), &RenderConfig::default())?;
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;
        let normalized_text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(normalized_text.contains("Audio fallbacks"), "{text}");
        assert!(
            normalized_text
                .contains("media%20library/clips/demo%20episode.mp3 (audio) Local episode"),
            "{text}",
        );
        assert!(
            normalized_text.contains("https://example.com/podcast%20episode.mp3 (audio)"),
            "{text}",
        );
        let targets = external_link_targets(&pdf);
        assert_eq!(
            targets,
            [
                "https://example.com/podcast%20episode.mp3".to_string(),
                "media%20library/clips/demo%20episode.mp3".to_string(),
            ]
        );
        Ok(())
    }

    #[test]
    fn video_blocks_preserve_clickable_static_sources() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Video fallbacks\n:imagesdir: media\n\n.Local formats\n[[local-video]]\nvideo::clips/demo clip.mp4,clips/demo clip.webm[]\n\nvideo::https://media.example.test/demo clip.mp4[]\n\nvideo::dQw4w9WgXcQ[youtube,start=10,end=20,opts=\"autoplay,loop\"]\n\nvideo::76979871[vimeo,start=10,opts=muted]\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;

        for (target, kind) in [
            ("media/clips/demo%20clip.mp4", "video"),
            ("media/clips/demo%20clip.webm", "video"),
            ("https://media.example.test/demo%20clip.mp4", "video"),
            (
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
                "YouTube video",
            ),
            ("https://vimeo.com/76979871", "Vimeo video"),
        ] {
            assert!(
                typst.contains(&format!(
                    "#link(\"{target}\")[#text(\"{target}\")]#text(\" \")#emph[#text(\"({kind})\")]"
                )),
                "missing {target} in generated Typst",
            );
        }
        assert!(!typst.contains("#t="));
        assert!(!typst.contains("&t=10"));
        assert!(typst.contains("#imagecaption[#text(\"Local formats\")]"));
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().map(|warning| warning.message.as_ref()),
            Some(
                "interactive media playback is unavailable in static PDF output; rendering clickable source links"
            ),
        );

        let rendered = render_pdf(&typst, &ImageMap::new(), &RenderConfig::default())?;
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        assert_eq!(
            external_link_targets(&pdf),
            [
                "https://media.example.test/demo%20clip.mp4".to_string(),
                "https://vimeo.com/76979871".to_string(),
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
                "media/clips/demo%20clip.mp4".to_string(),
                "media/clips/demo%20clip.webm".to_string(),
            ]
        );
        Ok(())
    }

    #[test]
    fn video_poster_is_a_linked_static_fallback() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let media_directory = directory.path().join("media library");
        std::fs::create_dir(&media_directory)?;
        std::fs::write(
            media_directory.join("poster file.svg"),
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="90"><rect width="160" height="90" fill="#334155"/></svg>"##,
        )?;
        let parsed = acdc_parser::parse(
            "= Video poster\n:imagesdir: media library\n\n.Poster title\n[[poster-video]]\nvideo::demo.mp4[poster=poster file.svg]\n",
            &acdc_parser::Options::default(),
        )?;
        let typst_path = directory.path().join("video-poster.typ");
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone())
            .with_pdf_options(PdfOptions {
                emit_typst: Some(typst_path.clone()),
                ..PdfOptions::default()
            });
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let rendered = processor.render_document(
            parsed.document(),
            Some(&directory.path().join("document.adoc")),
            &mut diagnostics,
        )?;

        assert_eq!(rendered.resolved_document_image_count, 1);
        let typst = std::fs::read_to_string(typst_path)?;
        assert!(
            typst.contains("alt: \"poster file\", destination: \"media%20library/demo.mp4\")"),
            "{typst}",
        );
        assert!(typst.contains("#imagecaption[#text(\"Poster title\")]"));
        assert!(!typst.contains("Figure 1."));
        assert!(!typst.contains("#text(\"demo.mp4\")"));
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().map(|warning| warning.message.as_ref()),
            Some(
                "interactive media playback is unavailable in static PDF output; rendering clickable source links"
            ),
        );

        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        assert_eq!(
            external_link_targets(&pdf),
            ["media%20library/demo.mp4".to_string()]
        );
        Ok(())
    }

    #[test]
    fn inline_icon_failures_warn_for_each_macro_occurrence()
    -> Result<(), Box<dyn std::error::Error>> {
        let font_warnings = render_warnings(
            ":icons: font\n\nicon:not-a-real-icon[] icon:another-missing[alt=Custom,size=2x,title=Title] icon:not-a-real-icon[] icon:heart[alt=Love,size=2x,title=Title]\n",
        )?;
        assert_eq!(font_warnings.len(), 3);
        assert_eq!(
            font_warnings
                .iter()
                .map(|warning| warning.message.as_ref())
                .collect::<Vec<_>>(),
            [
                "not-a-real-icon is not a valid icon name in the built-in icon set",
                "another-missing is not a valid icon name in the built-in icon set",
                "not-a-real-icon is not a valid icon name in the built-in icon set",
            ]
        );

        let directory = tempfile::tempdir()?;
        std::fs::write(
            directory.path().join("heart.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><path d="M0 0h10v10H0z"/></svg>"#,
        )?;
        let image_warnings = render_warnings(&format!(
            ":icons: svg\n:iconsdir: {}\n\nicon:not-a-real-icon[] icon:another-missing[alt=Custom,size=2x,title=Title] icon:not-a-real-icon[] icon:heart[alt=Love,size=2x,title=Title]\n",
            directory.path().display()
        ))?;
        assert_eq!(image_warnings.len(), 3);
        assert!(
            image_warnings
                .iter()
                .all(|warning| { warning.message.starts_with("image icon for '") })
        );
        assert_eq!(
            image_warnings
                .iter()
                .map(|warning| {
                    if warning.message.contains("'not-a-real-icon'") {
                        Some("not-a-real-icon")
                    } else if warning.message.contains("'another-missing'") {
                        Some("another-missing")
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>(),
            [
                Some("not-a-real-icon"),
                Some("another-missing"),
                Some("not-a-real-icon"),
            ]
        );

        let text_warnings = render_warnings(
            "icon:not-a-real-icon[] icon:not-a-real-icon[alt=Custom,size=2x,title=Title]\n",
        )?;
        assert!(text_warnings.is_empty(), "{text_warnings:?}");

        let attribution_warnings = render_warnings(
            ":icons: font\n\n[quote, 'icon:not-a-real-icon[]', 'icon:another-missing[]']\n____\nQuoted text.\n____\n",
        )?;
        assert_eq!(
            attribution_warnings
                .iter()
                .map(|warning| warning.message.as_ref())
                .collect::<Vec<_>>(),
            [
                "not-a-real-icon is not a valid icon name in the built-in icon set",
                "another-missing is not a valid icon name in the built-in icon set",
            ]
        );
        Ok(())
    }

    #[test]
    fn caller_supplied_example_titles_keep_the_default_caption()
    -> Result<(), Box<dyn std::error::Error>> {
        let caller_built = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(Vec::new()),
            "====",
            Location::default(),
        )
        .with_title(title("Caller-built"));

        let parsed = acdc_parser::parse(
            "[example]\n====\nContent.\n====\n",
            &acdc_parser::Options::default(),
        )?;
        let Some(Block::DelimitedBlock(mut title_added)) =
            parsed.document().blocks.first().cloned()
        else {
            return Err(std::io::Error::other("expected a delimited example").into());
        };
        title_added.title = title("Added later");

        let mut document = Document::default();
        document.attributes = parsed.document().attributes.clone();
        document.blocks = vec![
            Block::DelimitedBlock(caller_built),
            Block::DelimitedBlock(title_added),
        ];
        let processor = Processor::new(Options::default(), document.attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let rendered = processor.render_document(&document, None, &mut diagnostics)?;
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;

        assert!(text.contains("Example 1. Caller-built"), "{text}");
        assert!(text.contains("Example 2. Added later"), "{text}");
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn caller_built_listing_and_styled_paragraph_titles_take_captions()
    -> Result<(), Box<dyn std::error::Error>> {
        // A block built through the API carries no resolved caption, so the converter
        // classifies it with the parser's own rules rather than dropping its caption.
        let listing = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(Vec::new()),
            "----",
            Location::default(),
        )
        .with_title(title("Built listing"));
        let mut styled_paragraph = Paragraph::new(Vec::new(), Location::default());
        styled_paragraph.title = title("Built source paragraph");
        styled_paragraph.metadata.style = Some("source");

        let mut document = Document::default();
        document
            .attributes
            .set("listing-caption".into(), "Listing".into());
        document.blocks = vec![
            Block::DelimitedBlock(listing),
            Block::Paragraph(styled_paragraph),
        ];
        let processor = Processor::new(Options::default(), document.attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let rendered = processor.render_document(&document, None, &mut diagnostics)?;
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;

        assert!(text.contains("Listing 1. Built listing"), "{text}");
        assert!(text.contains("Listing 2. Built source paragraph"), "{text}");
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn caller_built_table_titles_continue_table_numbering() -> Result<(), Box<dyn std::error::Error>>
    {
        let parsed = acdc_parser::parse(
            ".Parsed table\n|===\n|Cell\n|===\n",
            &acdc_parser::Options::default(),
        )?;
        let Some(Block::DelimitedBlock(parsed_table)) = parsed.document().blocks.first() else {
            return Err(std::io::Error::other("expected a delimited table").into());
        };
        let caller_built =
            DelimitedBlock::new(parsed_table.inner.clone(), "|===", Location::default())
                .with_title(title("Caller-built table"));

        let mut document = Document::default();
        document.attributes = parsed.document().attributes.clone();
        document.blocks = vec![
            Block::DelimitedBlock(parsed_table.clone()),
            Block::DelimitedBlock(caller_built),
        ];
        let processor = Processor::new(Options::default(), document.attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let rendered = processor.render_document(&document, None, &mut diagnostics)?;
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;

        assert!(text.contains("Table 1. Parsed table"), "{text}");
        assert!(text.contains("Table 2. Caller-built table"), "{text}");
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn caller_built_figure_titles_continue_figure_numbering()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            ".Parsed figure\nimage::missing-one.svg[First]\n",
            &acdc_parser::Options::default(),
        )?;
        let caller_built = Image::new(Source::Name("missing-two.svg"), Location::default())
            .with_title(title("Caller-built figure"));
        let Some(parsed_figure) = parsed.document().blocks.first().cloned() else {
            return Err(std::io::Error::other("expected a block image").into());
        };

        let mut document = Document::default();
        document.attributes = parsed.document().attributes.clone();
        document.blocks = vec![parsed_figure, Block::Image(caller_built)];
        let processor = Processor::new(Options::default(), document.attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let rendered = processor.render_document(&document, None, &mut diagnostics)?;
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;

        assert!(text.contains("Figure 1. Parsed figure"), "{text}");
        assert!(text.contains("Figure 2. Caller-built figure"), "{text}");
        Ok(())
    }

    #[test]
    fn captioned_block_xrefs_honor_source_order_styles() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Caption cross-references\n:figure-caption: BeforeFigure\n:table-caption: BeforeTable\n:xrefstyle: short\n\nForward short: <<figure-target>> and <<table-target>>.\n\n:xrefstyle: full\n\nForward full: <<figure-target>> and <<table-target>>.\n\n:xrefstyle: basic\n\nBasic: <<figure-target>> and <<table-target>>.\n\n:figure-caption: TargetFigure\n:table-caption: TargetTable\n\n[[figure-target]]\n.A figure title\nimage::missing-reference-image.svg[Missing]\n\n[[table-target]]\n.A table title\n|===\n|Cell\n|===\n\n:figure-caption: AfterFigure\n:table-caption: AfterTable\n:xrefstyle: short\n\nBackward short: <<figure-target>> and <<table-target>>.\n\n:xrefstyle: full\n\nBackward full: <<figure-target>> and <<table-target>>.\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;
        let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut failures = Vec::new();
        for expected in [
            "Forward short: TargetFigure 1 and BeforeTable 1",
            "Forward full: TargetFigure 1, “A figure title” and BeforeTable 1, “A table title”",
            "Basic: A figure title and A table title",
            "Backward short: TargetFigure 1 and AfterTable 1",
            "Backward full: TargetFigure 1, “A figure title” and AfterTable 1, “A table title”",
        ] {
            if !normalized.contains(expected) {
                failures.push(format!("expected {expected:?} in {text:?}"));
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
        Ok(())
    }

    #[test]
    fn captions_and_cross_reference_text_preserve_inline_formatting()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Formatted captions and references\n:subject-name: Ada\n:xrefstyle: basic\n\nBasic: <<table-target>>.\n\nExplicit: xref:table-target[Own *bold* _italic_ `mono` {subject-name} link:https://example.com[link]].\n\n:xrefstyle: full\n\nFull: <<table-target>>.\n\n[[table-target]]\n.Caption *bold* _italic_ `mono` {subject-name}\n|===\n|Cell\n|===\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;
        for expected in [
            "#link(<id-7461626c652d746172676574>)[#text(\"Caption \")#strong[#text(\"bold\")]#text(\" \")#emph[#text(\"italic\")]#text(\" \")#raw(\"mono\")#text(\" Ada\")]",
            "#link(<id-7461626c652d746172676574>)[#text(\"Own \")#strong[#text(\"bold\")]#text(\" \")#emph[#text(\"italic\")]#text(\" \")#raw(\"mono\")#text(\" Ada \")#link(\"https://example.com\")[#text(\"link\")]]",
            "#link(<id-7461626c652d746172676574>)[#text(\"Table 1\")#text(\", “\")#text(\"Caption \")#strong[#text(\"bold\")]#text(\" \")#emph[#text(\"italic\")]#text(\" \")#raw(\"mono\")#text(\" Ada\")#text(\"”\")]",
            "#blocktitle[#text(\"Table 1. \")#text(\"Caption \")#strong[#text(\"bold\")]#text(\" \")#emph[#text(\"italic\")]#text(\" \")#raw(\"mono\")#text(\" Ada\")]",
        ] {
            assert!(typst.contains(expected), "expected {expected:?} in {typst}");
        }

        let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;
        let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for expected in [
            "Basic: Caption bold italic mono Ada",
            "Explicit: Own bold italic mono Ada link",
            "Full: Table 1, “Caption bold italic mono Ada”",
            "Table 1. Caption bold italic mono Ada",
        ] {
            assert!(
                normalized.contains(expected),
                "expected {expected:?} in {text:?}"
            );
        }
        Ok(())
    }

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
    fn explicit_running_header_title_does_not_replace_document_metadata()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Main *Title*: Subtitle _Part_\n\nBody.\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone())
            .with_pdf_options(PdfOptions {
                title: Some("Explicit Header".to_owned()),
                ..PdfOptions::default()
            });
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let options = processor.emit_options(parsed.document(), None, &[], &mut diagnostics);

        assert_eq!(
            options.running_header_title.as_deref(),
            Some("Explicit Header")
        );
        assert_eq!(
            options.metadata.title.as_deref(),
            Some("Main Title: Subtitle Part")
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn document_language_maps_to_typst_language_and_region() {
        for (value, language, region) in [
            ("pt", "pt", None),
            ("PT-br", "pt", Some("BR")),
            ("pt_BR", "pt", Some("BR")),
            ("eng-US", "eng", Some("US")),
        ] {
            assert_eq!(
                parse_document_locale(value),
                DocumentLocale::try_from_codes(language, region),
            );
        }

        assert_eq!(
            parse_document_locale("english"),
            Err(TypstError::InvalidLanguage {
                value: "english".to_owned(),
            }),
        );
        for (value, region) in [
            ("pt-BRA", "BRA"),
            ("pt-BR-extra", "BR-extra"),
            ("pt_BR-extra", "BR-extra"),
        ] {
            assert_eq!(
                parse_document_locale(value),
                Err(TypstError::InvalidRegion {
                    value: region.to_owned(),
                }),
            );
        }
    }

    #[test]
    fn unsupported_document_language_warns_and_uses_english()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Invalid Language\n:lang: english\n\nBody.\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let options = processor.emit_options(parsed.document(), None, &[], &mut diagnostics);

        assert_eq!(options.locale, DocumentLocale::default());
        assert_eq!(warnings.len(), 1);
        let warning = warnings
            .first()
            .ok_or_else(|| std::io::Error::other("missing document language warning"))?;
        assert_eq!(
            warning.message,
            "invalid language code `english`: expected two or three ASCII letters; using English for PDF text",
        );
        assert_eq!(
            warning.advice(),
            Some(
                "Set `:lang:` to a two- or three-letter language code, optionally followed by `-` or `_` and a two-letter region, for example `pt-BR`."
            )
        );
        Ok(())
    }

    #[test]
    fn named_footnote_references_preserve_parser_assigned_numbers()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "Alpha marker.footnote:alpha[Alpha note.]\n\nBeta marker.footnote:beta[Beta note.]\n\nAlpha repeat.footnote:alpha[].\n\nBeta repeat.footnote:beta[].\n",
            &acdc_parser::Options::default(),
        )?;
        assert_eq!(
            parsed
                .document()
                .footnotes
                .iter()
                .map(|footnote| footnote.number)
                .collect::<Vec<_>>(),
            [1, 2]
        );

        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;

        assert!(rendered.pdf.starts_with(b"%PDF-"));
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;
        let lines = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();

        let alpha_marker = lines
            .iter()
            .position(|line| *line == "Alpha marker.")
            .ok_or_else(|| std::io::Error::other("missing alpha marker"))?;
        let beta_marker = lines
            .iter()
            .position(|line| *line == "Beta marker.")
            .ok_or_else(|| std::io::Error::other("missing beta marker"))?;
        let alpha_repeat = lines
            .iter()
            .position(|line| *line == "Alpha repeat.")
            .ok_or_else(|| std::io::Error::other("missing alpha repeat"))?;
        let beta_repeat = lines
            .iter()
            .position(|line| *line == "Beta repeat.")
            .ok_or_else(|| std::io::Error::other("missing beta repeat"))?;
        assert_eq!(lines.get(alpha_marker + 1), Some(&"1"), "{text}");
        assert_eq!(lines.get(beta_marker + 1), Some(&"2"), "{text}");
        assert_eq!(lines.get(alpha_repeat + 1), Some(&"1"), "{text}");
        assert_eq!(lines.get(beta_repeat + 1), Some(&"2"), "{text}");

        let alpha_note = lines
            .iter()
            .position(|line| *line == "Alpha note.")
            .ok_or_else(|| std::io::Error::other("missing alpha footnote"))?;
        let beta_note = lines
            .iter()
            .position(|line| *line == "Beta note.")
            .ok_or_else(|| std::io::Error::other("missing beta footnote"))?;
        assert_eq!(
            alpha_note.checked_sub(1).and_then(|index| lines.get(index)),
            Some(&"1"),
            "{text}"
        );
        assert_eq!(
            beta_note.checked_sub(1).and_then(|index| lines.get(index)),
            Some(&"2"),
            "{text}"
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn covers_unnamed_and_named_footnote_reference_matrix() -> Result<(), Box<dyn std::error::Error>>
    {
        let parsed = acdc_parser::parse(
            "Unnamed marker.footnote:[Anonymous note.]\n\nSingle definition.footnote:single[Single note.]\n\nSingle reference.footnote:single[].\n\nMultiple definition.footnote:multiple[Multiple note.]\n\nMultiple reference one.footnote:multiple[].\n\nMultiple reference two.footnote:multiple[].\n",
            &acdc_parser::Options::default(),
        )?;
        assert_eq!(
            parsed
                .document()
                .footnotes
                .iter()
                .map(|footnote| footnote.id)
                .collect::<Vec<_>>(),
            [None, Some("single"), Some("multiple")]
        );

        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;
        assert!(rendered.pdf.starts_with(b"%PDF-"));
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;
        for expected in [
            "Anonymous note.",
            "Single note.",
            "Multiple note.",
            "Single reference.",
            "Multiple reference one.",
            "Multiple reference two.",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
    }

    #[test]
    fn supports_footnotes_in_formatted_text_table_cells_titles_and_list_items()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Document\n\n== Heading footnote:title[Title note.]\n\nA *formatted footnote:formatted[Formatted note.]*.\n\n|===\n|Cell footnote:cell[Cell note.]\n|===\n\n* Item footnote:list[List note.]\n\nReferences footnote:title[], footnote:formatted[], footnote:cell[], and footnote:list[].\n",
            &acdc_parser::Options::default(),
        )?;
        assert_eq!(
            parsed
                .document()
                .footnotes
                .iter()
                .map(|footnote| (footnote.id, footnote.number))
                .collect::<Vec<_>>(),
            [
                (Some("title"), 1),
                (Some("formatted"), 2),
                (Some("cell"), 3),
                (Some("list"), 4)
            ]
        );

        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;
        assert!(rendered.pdf.starts_with(b"%PDF-"));
        let pdf = lopdf::Document::load_mem(&rendered.pdf)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;
        for expected in [
            "Heading",
            "formatted",
            "Cell",
            "Item",
            "Title note.",
            "Formatted note.",
            "Cell note.",
            "List note.",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        assert!(warnings.is_empty(), "{warnings:?}");
        Ok(())
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
    fn invalid_custom_theme_reports_its_path() -> Result<(), Box<dyn std::error::Error>> {
        let theme_file = NamedTempFile::new()?;
        std::fs::write(theme_file.path(), "palette: [")?;
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
            return Err(std::io::Error::other("invalid theme unexpectedly accepted").into());
        };
        assert_eq!(path, theme_file.path());
        Ok(())
    }

    #[test]
    fn emitted_typst_ends_with_one_newline() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse("A paragraph.\n", &acdc_parser::Options::default())?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;

        assert!(typst.ends_with('\n'));
        assert!(!typst.ends_with("\n\n"));
        Ok(())
    }

    #[test]
    fn block_image_float_fallback_warns_for_each_block_image()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "image::one.svg[float=left]\n\nimage::two.svg[float=right]\n\nimage::three.svg[float=invalid]\n\nBefore image:four.svg[float=left] after.\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let _ = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;

        assert_eq!(warnings.len(), 2);
        for warning in warnings {
            assert_eq!(
                warning.message,
                "block image side wrapping is not yet supported by the PDF backend, rendering the image on the requested side with following content below it",
            );
            assert_eq!(
                warning.advice(),
                Some(
                    "Use the HTML backend or Asciidoctor PDF for this feature until PDF backend support is added."
                ),
            );
        }
        Ok(())
    }

    #[test]
    fn hostile_theme_font_name_remains_literal_typst_data() -> Result<(), Box<dyn std::error::Error>>
    {
        let hostile = r#"Acme"), size: 1pt)#undefined_function()//"#;
        let mut theme = Theme::default();
        theme
            .typography
            .body_font
            .fallback
            .insert(0, hostile.to_owned());
        let parsed = acdc_parser::parse("A paragraph.\n", &acdc_parser::Options::default())?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);

        let assets = ImageMap::new();
        let emit_options = processor.emit_options(parsed.document(), None, &[], &mut diagnostics);
        let typst = processor.emit_typst_source(
            parsed.document(),
            &assets,
            &theme,
            &emit_options,
            &collect_pdf_preparation(parsed.document()),
            &mut diagnostics,
        )?;
        assert!(
            typst.contains(r#""Acme\"), size: 1pt)#undefined_function()//", "IBM Plex Serif""#)
        );

        // If the quote in the family name closed its string, the deliberately
        // undefined function above would make compilation fail. A valid PDF is
        // therefore an end-to-end assertion that the payload remained data.
        let rendered = render_pdf(&typst, &assets, &RenderConfig::default())?;
        assert!(rendered.pdf.starts_with(b"%PDF-"));
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

        let rendered = processor.render_document(parsed.document(), None, &mut diagnostics)?;
        assert!(rendered.pdf.starts_with(b"%PDF-"));
        assert_eq!(warnings.len(), 2);
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
            "= Header: image:subtitle.png[] Subtitle\n:imagesdir: media library\n\n. image:paragraph-title.png[]\nParagraph image:body.png[] and image:body.png[] again.\n\n.List image:list-title.png[]\n* item\n\n== image:section.png[] Section\n\n.Block image:block-title.png[]\n....\nimage:literal.png[]\n....\n\n////\nimage:comment.png[]\n////\n",
            &acdc_parser::Options::default(),
        )?;

        assert_eq!(
            collect_pdf_preparation(parsed.document())
                .image_urls
                .into_iter()
                .collect::<Vec<_>>(),
            vec![
                "media%20library/block-title.png",
                "media%20library/body.png",
                "media%20library/list-title.png",
                "media%20library/paragraph-title.png",
                "media%20library/section.png",
                "media%20library/subtitle.png",
            ]
        );
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
}
