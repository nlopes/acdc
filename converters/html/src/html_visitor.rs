//! Visitor implementation for HTML conversion.

use std::{convert::Infallible, io::Write, path::Path, rc::Rc};

use acdc_converters_core::{
    Diagnostics, TraversalContext, document_attribute_text,
    substitutions::{TextBoundaries, effective_subs},
    visitor::BlockVisitor,
};

use acdc_parser::{
    CaptionKind, ColumnStyle, NORMAL, Substitution,
    ast::{
        AttributeValueRef, AttributesRef, BlockKind, BlockRef, CaptionRef, ColumnFormatRef,
        ContentRef, DocumentAttributesRef, DocumentRef, FootnoteDefinitionRef, HeaderRef,
        ParagraphRef, SectionRef, StructuralRef, TableRef,
    },
};

use crate::{Converter, Error, HtmlVariant, RenderOptions, STYLESDIR_DEFAULT, docinfo::DocInfo};

const DEPRECATED_HIGHLIGHT_ATTRIBUTES: [(&str, &str); 2] = [
    ("syntect-css", "highlight-css"),
    ("syntect-style", "highlight-style"),
];

fn displayed_attribute_text(attributes: DocumentAttributesRef<'_>, name: &str) -> Option<String> {
    attributes
        .get(name)
        .filter(|value| value.text().is_some())
        .map(crate::inlines::render_attribute_text)
}

fn link_css<W: Write>(
    writer: &mut W,
    attributes: DocumentAttributesRef<'_>,
    default_filename: &str,
) -> Result<(), Error> {
    // Link to external stylesheet
    let stylesdir = attributes
        .get("stylesdir")
        .and_then(|value| value.text())
        .unwrap_or(STYLESDIR_DEFAULT);

    let stylesheet = attributes
        .get("stylesheet")
        .and_then(|value| value.text())
        .filter(|value| !value.is_empty())
        .unwrap_or(default_filename);

    writeln!(
        writer,
        r#"<link rel="stylesheet" href="{}/{}">"#,
        crate::inlines::escape_attribute(stylesdir.trim_end_matches('/')),
        crate::inlines::escape_attribute(stylesheet)
    )?;

    // Add supplementary styles for stem blocks
    writeln!(
        writer,
        "<style>
.stemblock .content {{
  text-align: center;
}}
</style>"
    )?;
    Ok(())
}

/// Try to read a custom stylesheet from disk based on `stylesheet` and `stylesdir` attributes.
///
/// Returns `Some(contents)` if a custom stylesheet is specified and readable,
/// `None` otherwise (falls back to default CSS).
fn resolve_custom_css(
    attributes: DocumentAttributesRef<'_>,
    source_dir: Option<&Path>,
    diagnostics: &mut Diagnostics<'_>,
) -> Option<String> {
    let stylesheet = attributes
        .get("stylesheet")
        .and_then(|value| value.text())
        .filter(|value| !value.is_empty())?;

    let stylesdir = attributes
        .get("stylesdir")
        .and_then(|value| value.text())
        .unwrap_or(STYLESDIR_DEFAULT);

    let path = if Path::new(stylesdir).is_absolute() {
        std::path::PathBuf::from(stylesdir).join(stylesheet)
    } else {
        let base = source_dir.unwrap_or_else(|| Path::new("."));
        base.join(stylesdir).join(stylesheet)
    };

    match std::fs::read_to_string(&path) {
        Ok(contents) => Some(contents),
        Err(e) => {
            diagnostics.warn_with_advice(
                format!(
                    "could not read custom stylesheet {}, falling back to default: {e}",
                    path.display()
                ),
                crate::STYLESHEET_ADVICE,
            );
            None
        }
    }
}

/// The `MathJax` loader script URL acdc references when `:stem:` is set. Exposed
/// so an embedded-mode consumer (no `<head>` is generated) can load the same
/// `MathJax` build on their own page.
pub const MATHJAX_LOADER_URL: &str = "https://cdn.jsdelivr.net/npm/mathjax@4/tex-mml-chtml.js";

/// The base `MathJax` configuration script. Standalone output adds an `eqnums`
/// override when requested. Embedded consumers can set `MathJax.tex.tags` after
/// this script and before loading `MathJax`. The base script's CSP `script-src`
/// hash is [`MATHJAX_CONFIG_CSP_HASH`].
pub const MATHJAX_CONFIG_SCRIPT: &str = concat!(
    "<script>",
    include_str!("../static/mathjax-config.js"),
    "</script>"
);

/// CSP `script-src` source (sha256) for [`MATHJAX_CONFIG_SCRIPT`]'s inline code,
/// so a host can allowlist it without `'unsafe-inline'`. Computed in `build.rs`
/// as the sha256 of `static/mathjax-config.js`, so it always matches the embedded
/// script.
pub const MATHJAX_CONFIG_CSP_HASH: &str = env!("ACDC_MATHJAX_CONFIG_CSP_HASH");

fn mathjax_numbering_script(
    attributes: DocumentAttributesRef<'_>,
) -> Option<(&'static str, &'static str)> {
    match document_attribute_text(attributes.get("eqnums")) {
        Some("" | "AMS") => Some((
            include_str!("../static/mathjax-eqnums-ams.js"),
            env!("ACDC_MATHJAX_EQNUMS_AMS_CSP_HASH"),
        )),
        Some("all") => Some((
            include_str!("../static/mathjax-eqnums-all.js"),
            env!("ACDC_MATHJAX_EQNUMS_ALL_CSP_HASH"),
        )),
        _ => None,
    }
}

fn add_mathjax<W: Write>(writer: &mut W, numbering: Option<&str>) -> Result<(), Error> {
    writeln!(writer, "{MATHJAX_CONFIG_SCRIPT}")?;
    if let Some(script) = numbering {
        writeln!(writer, "<script>{script}</script>")?;
    }
    writeln!(
        writer,
        r#"<script defer src="{MATHJAX_LOADER_URL}"></script>"#
    )?;
    Ok(())
}

/// Return whether the document enables STEM in its header, body, or a nested
/// `AsciiDoc` cell. Embedded consumers can use this to enable math rendering.
#[must_use]
pub fn uses_stem(document: DocumentRef<'_>) -> bool {
    let resources = HeadResources::collect(document);
    resources.stem || resources.nested_stem
}

struct HeadResources {
    stem: bool,
    font_icons: bool,
    nested_stem: bool,
}

impl HeadResources {
    fn collect(document: DocumentRef<'_>) -> Self {
        let mut traversal = TraversalContext::new(document.attributes());
        let mut resources = Self {
            stem: traversal.contains_key("stem"),
            font_icons: traversal.get("icons").and_then(|value| value.as_str()) == Some("font"),
            nested_stem: false,
        };
        let Ok(()) = traversal.visit_blocks(&mut resources, document.blocks());
        resources
    }

    fn visit_children<'doc>(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        node: StructuralRef<'doc>,
    ) -> Result<(), Infallible> {
        for child in node.children() {
            if let StructuralRef::Block(block) = child {
                traversal.visit_block(self, block)?;
            } else {
                self.visit_children(traversal, child)?;
            }
        }
        Ok(())
    }

    fn visit_table<'doc>(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        table: TableRef<'doc>,
    ) -> Result<(), Infallible> {
        for row in table
            .header()
            .into_iter()
            .chain(table.rows())
            .chain(table.footer())
        {
            let mut formats = table.columns();
            for cell in row.cells() {
                let default_style = formats
                    .next()
                    .map_or(ColumnStyle::Default, ColumnFormatRef::style);
                if cell.style().unwrap_or(default_style) == ColumnStyle::AsciiDoc {
                    traversal.with_table_cell(cell, |context| {
                        context.visit_blocks(self, cell.blocks())
                    })?;
                } else {
                    traversal.visit_blocks(self, cell.blocks())?;
                }
            }
        }
        Ok(())
    }
}

impl<'doc> BlockVisitor<'doc> for HeadResources {
    type Error = Infallible;

    fn visit_block(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        block: BlockRef<'doc>,
    ) -> Result<(), Self::Error> {
        if let BlockKind::DocumentAttribute(_) = block.kind() {
            if traversal.is_nested_document() {
                self.nested_stem |= traversal.contains_key("stem");
            } else {
                self.stem |= traversal.contains_key("stem");
                self.font_icons |=
                    traversal.get("icons").and_then(|value| value.as_str()) == Some("font");
            }
            Ok(())
        } else if let BlockKind::Table(table) = block.kind() {
            self.visit_table(traversal, table)
        } else {
            self.visit_children(traversal, block.into())
        }
    }
}

#[cfg(test)]
mod document_frame_tests;
#[cfg(test)]
mod head_resources_tests;
#[cfg(test)]
mod head_stylesheet_tests;

/// HTML visitor that generates HTML from `AsciiDoc` AST
pub(crate) struct HtmlVisitor<'a, 'd, W: Write> {
    pub(crate) writer: W,
    pub(crate) converter: Rc<Converter<'a>>,

    pub(crate) render_options: RenderOptions,
    /// Per-conversion diagnostics handle (warning source + sink borrow).
    pub(crate) diagnostics: Diagnostics<'d>,
    /// Current effective substitutions for inline rendering.
    /// Set per-block in `visit_delimited_block`, defaults to normal substitutions.
    pub(crate) current_subs: Vec<Substitution>,
    /// Current section style (e.g., "bibliography", "glossary").
    /// Set when entering a section, used by child blocks for style inheritance.
    pub(crate) section_style: Option<String>,
    /// Plain-text title of the section currently being rendered, used as the
    /// label for index back-links. `None` outside any section (e.g. preamble).
    pub(crate) current_section_title: Option<String>,
    /// Surrounding label link, suspended around child anchors and their formatting.
    pub(crate) inline_link: Option<crate::inlines::InlineLink>,
    pub(crate) captured_raw_fragments: Option<Vec<String>>,
    /// Resolved docinfo content for injection at head, header, and footer positions.
    docinfo: DocInfo,
    pub(crate) text_boundaries: TextBoundaries,
    // Header, body and table-cell assignments share one warning per alias.
    warned_highlight_aliases: [bool; DEPRECATED_HIGHLIGHT_ATTRIBUTES.len()],
}

impl<'a, 'd, W: Write> HtmlVisitor<'a, 'd, W> {
    pub(super) fn new(
        writer: W,
        converter: Rc<Converter<'a>>,
        render_options: RenderOptions,

        mut diagnostics: Diagnostics<'d>,
    ) -> Self {
        let docinfo = if render_options.embedded {
            DocInfo::empty()
        } else {
            DocInfo::resolve(
                converter.document_attributes(),
                converter.options.safe_mode(),
                render_options.source_dir.as_deref(),
                render_options.docname.as_deref(),
                &mut diagnostics,
            )
        };
        Self {
            writer,
            converter,

            render_options,
            diagnostics,
            current_subs: NORMAL.to_vec(),
            section_style: None,
            current_section_title: None,
            captured_raw_fragments: None,
            inline_link: None,
            docinfo,
            text_boundaries: TextBoundaries::BOTH,
            warned_highlight_aliases: [false; DEPRECATED_HIGHLIGHT_ATTRIBUTES.len()],
        }
    }

    fn warn_deprecated_highlight_attributes(&mut self, traversal: &TraversalContext<'a>) {
        for ((alias, primary), warned) in DEPRECATED_HIGHLIGHT_ATTRIBUTES
            .into_iter()
            .zip(&mut self.warned_highlight_aliases)
        {
            if !*warned && traversal.is_explicit(alias) {
                self.diagnostics.warn_with_advice(
                    format!("attribute `{alias}` is deprecated"),
                    format!("Use `{primary}` instead."),
                );
                *warned = true;
            }
        }
    }

    pub(crate) fn visit_title(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
    ) -> Result<(), Error> {
        // Titles use normal substitutions, including above a verbatim or subs=none body.
        let previous_subs = std::mem::replace(&mut self.current_subs, NORMAL.to_vec());
        let previous_verbatim = std::mem::replace(&mut self.render_options.inlines_verbatim, false);
        let result = if let Some(title) = title {
            self.visit_content(traversal, title)
        } else {
            self.visit_inline_refs(traversal, std::iter::empty())
        };
        self.render_options.inlines_verbatim = previous_verbatim;
        self.current_subs = previous_subs;
        result
    }

    pub(crate) fn render_captioned_title_with_wrapper(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        attributes: Option<AttributesRef<'_>>,
        fallback: Option<CaptionKind>,
        prefix: &str,
        suffix: &str,
    ) -> Result<(), Error> {
        if title.is_none() {
            return Ok(());
        }
        let converter = Rc::clone(&self.converter);
        let caption = attributes.and_then(|attributes| {
            CaptionRef::resolve(attributes, converter.document_attributes().into(), fallback)
        });
        write!(self.writer, "{prefix}")?;
        match caption {
            Some(CaptionRef::CustomContent(content)) => {
                self.visit_title(traversal, Some(content))?;
            }
            Some(CaptionRef::Custom(text)) => {
                write!(self.writer, "{}", crate::inlines::escape_pcdata(text))?;
            }
            Some(CaptionRef::Numbered {
                label,
                number,
                kind,
            }) => {
                let number = number.map_or_else(
                    || converter.next_caption_number(kind),
                    std::num::NonZeroU32::get,
                );
                write!(
                    self.writer,
                    "{} {number}. ",
                    crate::inlines::escape_pcdata(label)
                )?;
            }
            Some(_) | None => {}
        }
        self.visit_title(traversal, title)?;
        write!(self.writer, "{suffix}")?;
        Ok(())
    }

    pub(crate) fn render_title_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        prefix: &str,
        suffix: &str,
    ) -> Result<(), Error> {
        if let Some(title) = title {
            write!(self.writer, "{prefix}")?;
            self.visit_title(traversal, Some(title))?;
            write!(self.writer, "{suffix}")?;
        }
        Ok(())
    }

    /// Consume the visitor and return the writer
    pub(super) fn into_writer(self) -> W {
        self.writer
    }

    pub(crate) const fn text_boundaries(&self) -> TextBoundaries {
        self.text_boundaries
    }

    /// Check if dark mode is enabled via the `:dark-mode:` document attribute.
    fn is_dark_mode(&self) -> bool {
        DocumentAttributesRef::from(self.converter.document_attributes()).contains("dark-mode")
    }

    /// Emit syntax highlighting CSS in `<head>` when class-based mode is active.
    ///
    /// - Without `linkcss`: embeds CSS in a `<style>` block (default).
    /// - With `linkcss`: emits a `<link>` to `{stylesdir}/acdc-highlight.css`.
    #[cfg(feature = "highlighting")]
    fn maybe_emit_syntax_css(&mut self) -> Result<(), Error> {
        let attributes = DocumentAttributesRef::from(self.converter.document_attributes());
        if attributes.contains("source-highlighter") {
            let (theme_name, mode) = crate::resolve_highlight_settings(attributes);
            if mode == crate::syntax::HighlightMode::Class {
                let linkcss = attributes.contains("linkcss");

                if linkcss {
                    let stylesdir = attributes
                        .get("stylesdir")
                        .and_then(|value| value.text())
                        .unwrap_or(STYLESDIR_DEFAULT);
                    writeln!(
                        self.writer,
                        r#"<link rel="stylesheet" href="{}/{}">"#,
                        stylesdir.trim_end_matches('/'),
                        crate::HIGHLIGHT_STYLESHEET
                    )?;
                } else if let Ok(css) = crate::syntax::highlight_css(&theme_name) {
                    writeln!(self.writer, "<style>\n{css}</style>")?;
                }
            }
        }
        Ok(())
    }

    /// Render webfonts link, stylesheet (embedded or linked), and max-width constraint.
    ///
    /// Skipped entirely when `:!stylesheet:` is set (no-stylesheet mode).
    fn render_stylesheet(&mut self, dark_mode: bool) -> Result<(), Error> {
        let attributes = DocumentAttributesRef::from(self.converter.document_attributes());
        let stylesheet_disabled =
            !attributes.contains("stylesheet") && attributes.is_explicit("stylesheet");

        if stylesheet_disabled {
            return Ok(());
        }

        // Render Google Fonts link (controlled by :webfonts: attribute)
        match attributes.get("webfonts").and_then(|value| value.text()) {
            None if attributes.is_explicit("webfonts") => {
                // :!webfonts: — skip font link entirely
            }
            Some(custom) if !custom.is_empty() => {
                writeln!(
                    self.writer,
                    r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css?family={custom}">"#
                )?;
            }
            Some(_) | None => {
                writeln!(
                    self.writer,
                    r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css?family=Open+Sans:300,300italic,400,400italic,600,600italic%7CNoto+Serif:400,400italic,700,700italic%7CDroid+Sans+Mono:400,700">"#
                )?;
            }
        }

        // Handle stylesheet rendering based on linkcss attribute
        let linkcss = attributes.contains("linkcss");
        let variant = self.converter.variant();
        let default_filename = match (variant, dark_mode) {
            (HtmlVariant::Semantic, true) => crate::STYLESHEET_HTML5S_DARK_MODE,
            (HtmlVariant::Semantic, false) => crate::STYLESHEET_HTML5S_LIGHT_MODE,
            (HtmlVariant::Standard, true) => crate::STYLESHEET_DARK_MODE,
            (HtmlVariant::Standard, false) => crate::STYLESHEET_LIGHT_MODE,
        };

        if linkcss {
            link_css(&mut self.writer, attributes, default_filename)?;
        } else {
            let custom_css = resolve_custom_css(
                attributes,
                self.render_options.source_dir.as_deref(),
                &mut self.diagnostics,
            );
            let css = custom_css
                .as_deref()
                .unwrap_or_else(|| crate::load_css(dark_mode, variant));
            writeln!(
                self.writer,
                "<style>\n{css}\n.stemblock .content {{\n  text-align: center;\n}}\n</style>"
            )?;
        }

        // Add max-width constraint if specified
        if let Some(max_width) = attributes.get("max-width").and_then(|value| value.text())
            && !max_width.is_empty()
        {
            self.diagnostics.warn_with_advice(
                format!("`max-width` usage is not recommended: {max_width}"),
                "Set the maximum content width in a CSS stylesheet instead.",
            );
            writeln!(
                self.writer,
                "<style>
#content {{
  max-width: {max_width};
}}
</style>"
            )?;
        }

        Ok(())
    }

    /// Whether the `:csp:` attribute opts this document into a `<meta>` Content
    /// Security Policy (standalone output only).
    fn is_csp_enabled(&self) -> bool {
        DocumentAttributesRef::from(self.converter.document_attributes()).contains("csp")
    }

    /// The acdc features this document uses, for building its CSP. Mirrors the
    /// gating the head uses to decide which scripts, fonts, and CDNs it emits.
    fn csp_features(
        &self,
        stem: bool,
        font_icons: bool,
        mathjax_numbering_hash: Option<&'static str>,
    ) -> crate::CspFeatures {
        let attrs = DocumentAttributesRef::from(self.converter.document_attributes());
        let stylesheet_disabled = !attrs.contains("stylesheet") && attrs.is_explicit("stylesheet");
        crate::CspFeatures {
            stem,
            mathjax_numbering_hash,
            webfonts: !(stylesheet_disabled
                || !attrs.contains("webfonts") && attrs.is_explicit("webfonts")),
            icons_font: font_icons,
            replay: cfg!(feature = "terminal"),
        }
    }

    fn render_head(
        &mut self,
        document: DocumentRef<'a>,
        stem: bool,
        font_icons: bool,
    ) -> Result<(), Error> {
        let dark_mode = self.is_dark_mode();
        let numbering = mathjax_numbering_script(document.attributes());

        writeln!(
            self.writer,
            r#"<head>
<meta charset="UTF-8">
<meta http-equiv="X-UA-Compatible" content="IE=edge">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="generator" content="{}">"#,
            self.converter.options.generator_metadata()
        )?;

        if dark_mode {
            writeln!(self.writer, r#"<meta name="color-scheme" content="dark">"#)?;
        }

        // Content Security Policy (opt-in via `:csp:`). A `<meta>` CSP governs
        // everything after it, so emit it before the stylesheet, fonts, and
        // scripts below.
        if self.is_csp_enabled() {
            writeln!(
                self.writer,
                r#"<meta http-equiv="Content-Security-Policy" content="{}">"#,
                self.csp_features(stem, font_icons, numbering.map(|(_, hash)| hash))
                    .content_security_policy()
            )?;
        }

        self.render_document_metadata()?;
        if let Some(header) = document.header() {
            self.render_header_metadata(header)?;
        }

        // Render stylesheet and webfonts (skipped when :!stylesheet: is set)
        self.render_stylesheet(dark_mode)?;

        // Add MathJax if stem is enabled
        if stem {
            add_mathjax(&mut self.writer, numbering.map(|(script, _)| script))?;
        }

        // Add Font Awesome if icons are set to font mode
        if font_icons {
            writeln!(
                self.writer,
                r#"<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@fortawesome/fontawesome-free@7.2.0/css/all.min.css">"#
            )?;
        }

        // Emit syntax highlighting CSS (embedded or linked) when using class-based mode
        #[cfg(feature = "highlighting")]
        self.maybe_emit_syntax_css()?;

        if let Some(content) = &self.docinfo.head {
            writeln!(self.writer, "{content}")?;
        }
        writeln!(self.writer, "</head>")?;
        Ok(())
    }

    fn render_body_footer(&mut self) -> Result<(), Error> {
        if self.converter.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "<footer id=\"footer\">")?;
            writeln!(self.writer, "<div id=\"footer-text\">")?;
            self.render_footer_version()?;
            if let Some(last_updated) = self.render_options.last_updated {
                writeln!(
                    self.writer,
                    "Last updated {}",
                    last_updated.format("%F %T %Z")
                )?;
            }
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</footer>")?;
        } else {
            writeln!(self.writer, "<div id=\"footer\">")?;
            writeln!(self.writer, "<div id=\"footer-text\">")?;
            self.render_footer_version()?;
            if let Some(last_updated) = self.render_options.last_updated {
                writeln!(
                    self.writer,
                    "Last updated {}",
                    last_updated.format("%F %T %Z")
                )?;
            }
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</div>")?;
        }
        Ok(())
    }

    /// Emit the footer's `{version-label} {revnumber}<br>` line when the
    /// document carries a revision number, matching asciidoctor. The `v` of a
    /// `vX.Y` revision line is already dropped by the parser; an explicit
    /// `:revnumber:` keeps whatever it was given.
    fn render_footer_version(&mut self) -> Result<(), Error> {
        let attributes = DocumentAttributesRef::from(self.converter.document_attributes());
        if let Some(revnumber) = displayed_attribute_text(attributes, "revnumber") {
            let label = attributes
                .get("version-label")
                .map_or_else(|| "Version".into(), crate::inlines::render_attribute_text);
            writeln!(self.writer, "{label} {revnumber}<br>")?;
        }
        Ok(())
    }

    pub(crate) fn render_footnotes(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        footnotes: impl Iterator<Item = FootnoteDefinitionRef<'a>>,
        in_cell: bool,
    ) -> Result<(), Error> {
        let mut footnotes = footnotes.peekable();
        if traversal.contains_key("nofootnotes") || footnotes.peek().is_none() {
            return Ok(());
        }
        if self.converter.variant() == HtmlVariant::Semantic {
            return self.render_footnotes_semantic(traversal, footnotes);
        }
        if in_cell {
            writeln!(self.writer, "<div class=\"footnotes\">")?;
        } else {
            writeln!(self.writer, "<div id=\"footnotes\">")?;
        }
        writeln!(self.writer, "<hr>")?;
        for footnote in footnotes {
            let number = footnote.number();
            writeln!(
                self.writer,
                "<div class=\"footnote\" id=\"_footnotedef_{number}\">"
            )?;
            write!(
                self.writer,
                "<a href=\"#_footnoteref_{number}\">{number}</a>. "
            )?;
            self.visit_content(traversal, footnote.content())?;
            writeln!(self.writer, "</div>")?;
        }
        writeln!(self.writer, "</div>")?;
        Ok(())
    }

    fn render_footnotes_semantic(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        footnotes: impl Iterator<Item = FootnoteDefinitionRef<'a>>,
    ) -> Result<(), Error> {
        writeln!(
            self.writer,
            "<section class=\"footnotes\" aria-label=\"Footnotes\" role=\"doc-endnotes\">"
        )?;
        writeln!(self.writer, "<hr>")?;
        writeln!(self.writer, "<ol class=\"footnotes\">")?;
        let mut next_number = 1;
        for footnote in footnotes {
            let number = footnote.number();
            write!(
                self.writer,
                "<li class=\"footnote\" id=\"_footnote_{number}\" role=\"doc-endnote\""
            )?;
            if number != next_number {
                write!(self.writer, " value=\"{number}\"")?;
            }
            next_number = number.saturating_add(1);
            writeln!(self.writer, ">")?;
            self.visit_content(traversal, footnote.content())?;
            write!(
                self.writer,
                " <a class=\"footnote-backref\" href=\"#_footnoteref_{number}\" role=\"doc-backlink\" title=\"Jump to the first occurrence in the text\">&#8617;</a>"
            )?;
            writeln!(self.writer, "</li>")?;
        }
        writeln!(self.writer, "</ol>")?;
        writeln!(self.writer, "</section>")?;
        Ok(())
    }
}

impl<'a, W: Write> BlockVisitor<'a> for HtmlVisitor<'a, '_, W> {
    type Error = Error;
    fn before_block(
        &mut self,
        context: &mut TraversalContext<'a>,
        _block: BlockRef<'a>,
    ) -> Result<(), Self::Error> {
        if context.is_nested_document() {
            self.warn_deprecated_highlight_attributes(context);
        }
        Ok(())
    }
    fn visit_block(
        &mut self,
        context: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
    ) -> Result<(), Self::Error> {
        match block.kind() {
            BlockKind::DocumentAttribute(_) => {
                self.warn_deprecated_highlight_attributes(context);
                Ok(())
            }
            BlockKind::Section(section) => self.visit_section_ref(context, section),
            BlockKind::Paragraph(paragraph) => self.visit_paragraph_ref(context, paragraph),
            BlockKind::OrderedList(list) => self.render_ordered_list(context, list),
            BlockKind::UnorderedList(list) => {
                let style = self.section_style.clone();
                self.render_unordered_list(context, list, style.as_deref())
            }
            BlockKind::DescriptionList(list) => self.render_description_list(context, list),
            BlockKind::CalloutList(list) => self.render_callout_list(context, list),
            BlockKind::Admonition(admonition) => self.render_admonition(context, admonition),
            BlockKind::Image(image) => self.render_image(context, image),
            BlockKind::Audio(audio) => self.render_audio(context, audio),
            BlockKind::Video(video) => self.render_video(context, video),
            BlockKind::DiscreteHeading(header) => {
                crate::section::visit_discrete_header(context, header, self)
            }
            BlockKind::TableOfContents(toc) => self.render_toc(context, Some(toc), "macro"),
            BlockKind::ThematicBreak(br) => {
                write!(self.writer, "<hr")?;
                if let Some(id) = br.id() {
                    crate::write_element_id(&mut self.writer, Some(id))?;
                }
                writeln!(self.writer, ">")?;
                Ok(())
            }
            BlockKind::PageBreak(_) => {
                writeln!(
                    self.writer,
                    "<div style=\"page-break-after: always;\"></div>"
                )?;
                Ok(())
            }
            BlockKind::Comment(_) => Ok(()),
            BlockKind::Example(_)
            | BlockKind::Listing(_)
            | BlockKind::Literal(_)
            | BlockKind::Open(_)
            | BlockKind::Sidebar(_)
            | BlockKind::Passthrough(_)
            | BlockKind::Quote(_)
            | BlockKind::Verse(_)
            | BlockKind::Stem(_)
            | BlockKind::Table(_)
            | BlockKind::DelimitedComment(_) => self.visit_delimited_block_ref(context, block),
            _ => {
                self.warn_unhandled_block();
                Ok(())
            }
        }
    }
}

impl<'a, W: Write> acdc_converters_core::visitor::DocumentVisitor<'a> for HtmlVisitor<'a, '_, W> {
    fn visit_document_start(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        doc: DocumentRef<'a>,
    ) -> Result<(), Self::Error> {
        let attributes = doc.attributes();
        self.warn_deprecated_highlight_attributes(traversal);
        // In embedded mode, skip the document frame (DOCTYPE, html, head, body)
        if self.render_options.embedded {
            return Ok(());
        }

        writeln!(self.writer, "<!DOCTYPE html>")?;

        // Add lang attribute if not suppressed by :nolang:
        if let Some(lang_value) = attributes.get("lang").and_then(|value| value.text())
            && !lang_value.is_empty()
        {
            writeln!(self.writer, "<html lang=\"{lang_value}\">")?;
        } else if attributes.contains("lang") {
            writeln!(self.writer, "<html>")?;
        } else if attributes.contains("nolang") {
            // :nolang: attribute suppresses lang
            writeln!(self.writer, "<html>")?;
        } else {
            // No lang attribute and no nolang - use default "en"
            writeln!(self.writer, "<html lang=\"en\">")?;
        }

        let resources = HeadResources::collect(doc);
        self.render_head(doc, resources.stem, resources.font_icons)?;

        let converter_attributes =
            DocumentAttributesRef::from(self.converter.document_attributes());
        if converter_attributes.contains("css-signature") {
            return Err(Error::UnsupportedCssSignature);
        }

        let doctype_str = converter_attributes
            .get("doctype")
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| self.converter.options.doctype().to_string());
        let mut body_classes = vec![doctype_str];

        if self.is_dark_mode() {
            body_classes.push("dark".to_string());
        }

        // Add TOC-related classes to body based on placement and custom toc-class
        let toc_config =
            acdc_converters_core::toc::Config::resolve(None, &TraversalContext::new(attributes));
        let has_custom_toc_class = attributes.contains("toc-class");

        match toc_config.placement() {
            "left" | "right" | "top" | "bottom" => {
                // Sidebar positions: add toc_class and toc-{position}
                body_classes.push(toc_config.toc_class().to_string());
                body_classes.push(format!("toc-{}", toc_config.placement()));
            }
            "auto" if has_custom_toc_class => {
                // Auto placement with custom toc-class: add toc_class and toc-header
                body_classes.push(toc_config.toc_class().to_string());
                body_classes.push("toc-header".to_string());
            }
            _ => {
                // Auto/preamble/macro with default class or no TOC: no additional body classes
            }
        }

        // Add roles from document title metadata to body classes
        if let Some(header) = doc.header() {
            body_classes.extend(header.roles().iter().map(str::to_owned));
        }

        let body_class = crate::inlines::escape_role(&body_classes.join(" "));

        if let Some(id) = doc.id() {
            let id = crate::inlines::escape_attribute(id.as_str());
            writeln!(self.writer, "<body id=\"{id}\" class=\"{body_class}\">")?;
        } else {
            writeln!(self.writer, "<body class=\"{body_class}\">")?;
        }
        if let Some(content) = &self.docinfo.header {
            writeln!(self.writer, "{content}")?;
        }
        Ok(())
    }

    fn visit_preamble_end(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        _doc: DocumentRef<'a>,
    ) -> Result<(), Self::Error> {
        if self.converter.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "</section>")?;
        } else {
            writeln!(self.writer, "</div>")?; // Close sectionbody
            writeln!(self.writer, "</div>")?; // Close preamble
        }

        self.render_toc(traversal, None, "preamble")?;
        Ok(())
    }

    fn visit_document_supplements(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        doc: DocumentRef<'a>,
    ) -> Result<(), Self::Error> {
        // Close #content (only if not in embedded mode)
        if !self.render_options.embedded {
            if self.converter.variant() == HtmlVariant::Semantic {
                writeln!(self.writer, "</main>")?;
            } else {
                writeln!(self.writer, "</div>")?;
            }
        }
        let mut footnotes = doc.footnotes().peekable();
        if !traversal.contains_key("nofootnotes") && footnotes.peek().is_some() {
            self.render_footnotes(traversal, footnotes, false)?;
        }
        // Skip footer in embedded mode
        if !self.render_options.embedded {
            if !traversal.contains_key("nofooter") {
                self.render_body_footer()?;
            }
            if let Some(content) = &self.docinfo.footer {
                writeln!(self.writer, "{content}")?;
            }
        }
        Ok(())
    }

    fn visit_document_end(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _doc: DocumentRef<'a>,
    ) -> Result<(), Self::Error> {
        // In embedded mode, skip the closing document frame tags
        if self.render_options.embedded {
            return Ok(());
        }

        writeln!(self.writer, "</body>")?;
        write!(self.writer, "</html>")?;

        Ok(())
    }

    fn visit_header(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        header: HeaderRef<'a>,
    ) -> Result<(), Self::Error> {
        self.render_header(traversal, header)
    }

    fn visit_body_content_start(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        doc: DocumentRef<'a>,
    ) -> Result<(), Self::Error> {
        if self.render_options.embedded {
            // When there's no header, the TOC hasn't been rendered yet
            if doc.header().is_none() {
                self.render_toc(traversal, None, "auto")?;
            }
            return Ok(());
        }
        // Headerless documents still need a wrapper for an automatic TOC.
        if doc.header().is_none()
            && !traversal.contains_key("noheader")
            && !self.converter.toc_entries.is_empty()
        {
            let toc_config = acdc_converters_core::toc::Config::resolve(
                None,
                &TraversalContext::new(DocumentAttributesRef::from(
                    self.converter.document_attributes(),
                )),
            );
            if matches!(
                toc_config.placement(),
                "auto" | "left" | "right" | "top" | "bottom"
            ) {
                if self.converter.variant() == HtmlVariant::Semantic {
                    writeln!(self.writer, "<header id=\"header\">")?;
                } else {
                    writeln!(self.writer, "<div id=\"header\">")?;
                }
                self.render_toc(traversal, None, "auto")?;
                if self.converter.variant() == HtmlVariant::Semantic {
                    writeln!(self.writer, "</header>")?;
                } else {
                    writeln!(self.writer, "</div>")?;
                }
            }
        }
        if self.converter.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "<main id=\"content\">")?;
        } else {
            writeln!(self.writer, "<div id=\"content\">")?;
        }
        Ok(())
    }

    fn visit_preamble_start(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _doc: DocumentRef<'a>,
    ) -> Result<(), Self::Error> {
        if self.converter.variant() == HtmlVariant::Semantic {
            writeln!(
                self.writer,
                "<section id=\"preamble\" aria-label=\"Preamble\">"
            )?;
        } else {
            writeln!(self.writer, "<div id=\"preamble\">")?;
            writeln!(self.writer, "<div class=\"sectionbody\">")?;
        }
        Ok(())
    }
}

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    fn render_header(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        header: HeaderRef<'_>,
    ) -> Result<(), Error> {
        if self.render_options.embedded {
            // In embedded mode, render the TOC but skip the header chrome
            self.render_toc(traversal, None, "auto")?;
            return Ok(());
        }
        if traversal.contains_key("noheader") {
            return Ok(());
        }
        if self.converter.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "<header>")?;
        } else {
            writeln!(self.writer, "<div id=\"header\">")?;
        }
        if let Some(title) = header.title().filter(|title| !title.is_empty()) {
            write!(self.writer, "<h1>")?;
            self.visit_title(traversal, Some(title))?;
            if let Some(subtitle) = header.subtitle() {
                write!(self.writer, ": ")?;
                self.visit_title(traversal, Some(subtitle))?;
            }
            writeln!(self.writer, "</h1>")?;
            // Output details div if there are authors or revision info
            let attributes = traversal.header();
            let revnumber = displayed_attribute_text(attributes, "revnumber");
            let revdate = displayed_attribute_text(attributes, "revdate");
            let has_revision = revnumber.is_some() || revdate.is_some();
            if header.authors().next().is_some() || has_revision {
                writeln!(self.writer, "<div class=\"details\">")?;
                for (i, author) in header.authors().enumerate() {
                    write!(
                        self.writer,
                        "<span id=\"author{}\" class=\"author\">",
                        if i > 0 {
                            format!("{}", i + 1)
                        } else {
                            String::new()
                        }
                    )?;
                    write!(
                        self.writer,
                        "{} ",
                        crate::inlines::escape_role(author.first_name())
                    )?;
                    if let Some(middle_name) = author.middle_name() {
                        write!(self.writer, "{} ", crate::inlines::escape_role(middle_name))?;
                    }
                    write!(
                        self.writer,
                        "{}",
                        crate::inlines::escape_role(author.last_name())
                    )?;
                    writeln!(self.writer, "</span><br>")?;
                    if let Some(email) = author.email() {
                        let email = crate::inlines::escape_role(email);
                        // Emit on a single line, like asciidoctor: a newline
                        // inside the span renders as a leading space that shifts
                        // the email right of the `–` separator.
                        let suffix = if i > 0 {
                            format!("{}", i + 1)
                        } else {
                            String::new()
                        };
                        writeln!(
                            self.writer,
                            "<span id=\"email{suffix}\" class=\"email\"><a href=\"mailto:{email}\">{email}</a></span><br>"
                        )?;
                    }
                }
                // Render revision info spans. The version word is the
                // `version-label` attribute lowercased; the `v` of a `vX.Y`
                // revision line is already dropped by the parser, while an
                // explicit `:revnumber:` keeps whatever it was given. The
                // trailing comma is emitted only when a revdate follows.
                if let Some(revnumber) = revnumber {
                    let label = attributes
                        .get("version-label")
                        .map_or_else(|| "Version".into(), crate::inlines::render_attribute_text);
                    writeln!(
                        self.writer,
                        "<span id=\"revnumber\">{} {revnumber}{}</span>",
                        label.to_lowercase(),
                        if revdate.is_some() { "," } else { "" }
                    )?;
                }
                if let Some(revdate) = revdate {
                    writeln!(self.writer, "<span id=\"revdate\">{revdate}</span>")?;
                }
                if let Some(revremark) = displayed_attribute_text(attributes, "revremark") {
                    writeln!(self.writer, "<br><span id=\"revremark\">{revremark}</span>")?;
                }
                writeln!(self.writer, "</div>")?;
            }
        }

        // Render TOC after header if toc="auto"
        self.render_toc(traversal, None, "auto")?;
        if self.converter.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "</header>")?;
        } else {
            writeln!(self.writer, "</div>")?; // Close #header div
        }
        Ok(())
    }

    fn visit_section_ref(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        section: SectionRef<'a>,
    ) -> Result<(), Error> {
        let previous_style = self.section_style.clone();
        let data = section;
        self.section_style = data.style().map(str::to_owned);
        // Set before rendering the header so index terms in the section's own
        // title attribute to this section; restore the parent (or None) on exit
        // so nested sections pop back correctly.
        let previous_section_title = self.current_section_title.take();
        self.current_section_title = Some(data.title().map_or_else(String::new, |content| {
            acdc_converters_core::InlineTextTransform::default()
                .decode_char_refs(true)
                .content_to_string(content)
        }));
        let result = self.render_section(traversal, section);
        self.current_section_title = previous_section_title;
        self.section_style = previous_style;
        result
    }

    fn visit_paragraph_ref(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        // Paragraphs with [literal], [listing], or [source] style are verbatim
        let data = para;
        let is_verbatim = data
            .style()
            .is_some_and(|s| matches!(s, "literal" | "listing" | "source"));

        // Compute effective substitutions for this paragraph
        let new_subs = effective_subs(
            match data.attributes().get("subs") {
                Some(AttributeValueRef::Substitutions(spec)) => Some(spec),
                _ => None,
            },
            is_verbatim,
        );
        let original_subs = std::mem::replace(&mut self.current_subs, new_subs);

        let result = self.render_paragraph(traversal, para);

        self.current_subs = original_subs;

        result
    }

    fn visit_delimited_block_ref(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
    ) -> Result<(), Error> {
        let data = block;
        let is_verbatim = matches!(data.kind(), BlockKind::Listing(_) | BlockKind::Literal(_));

        // Compute effective substitutions for this block
        let new_subs = effective_subs(
            match data
                .attributes()
                .and_then(|attributes| attributes.get("subs"))
            {
                Some(AttributeValueRef::Substitutions(spec)) => Some(spec),
                _ => None,
            },
            is_verbatim,
        );
        let original_subs = std::mem::replace(&mut self.current_subs, new_subs);

        // Toggle verbatim mode for verbatim blocks
        let original_verbatim = self.render_options.inlines_verbatim;
        if is_verbatim {
            self.render_options.inlines_verbatim = true;
        }

        let result = self.render_delimited_block(traversal, block);

        // Restore state
        self.current_subs = original_subs;
        self.render_options.inlines_verbatim = original_verbatim;

        result
    }
}

impl<W: Write> HtmlVisitor<'_, '_, W> {
    fn warn_unhandled_block(&mut self) {
        self.diagnostics.warn_with_advice(
            "an unsupported parser block variant was omitted from HTML output",
            "Use another backend for this document and report the unsupported construct.",
        );
    }

    pub(crate) fn writer_mut(&mut self) -> &mut dyn Write {
        &mut self.writer
    }
}

#[cfg(test)]
mod title_media_tests {
    use super::*;
    use acdc_converters_core::{
        Converter as _, Options as ConverterOptions, WarningSource, visitor::DocumentVisitor,
    };
    use acdc_parser::{
        Anchor, AttributeValue, Block, CompatibilityMode, Document, Location,
        Options as ParserOptions, Section, Title, parse, parse_inline,
    };

    type TestError = Box<dyn std::error::Error>;

    struct RejectWrites;

    struct RejectCellText;

    impl Write for RejectCellText {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.windows(7).any(|part| part == b"TRIGGER") {
                Err(std::io::Error::other("rejected cell text"))
            } else {
                Ok(bytes.len())
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Write for RejectWrites {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("rejected"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn source_leaf_errors_restore_substitution_state() -> Result<(), TestError> {
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
            let converter = Rc::new(
                Converter::new(ConverterOptions::default(), ParserOptions::builder())?
                    .with_variant(variant)?,
            );
            for input in [
                "[source,rust,subs=none]\n----\nTRIGGER\n----\n",
                "[literal,subs=none]\n....\nTRIGGER\n....\n",
                "[verse,Author,Book]\n____\nTRIGGER\n____\n",
                "[latexmath,subs=none]\n++++\nTRIGGER\n++++\n",
                "[subs=none]\n++++\nTRIGGER\n++++\n",
            ] {
                let parsed = parse(input, &ParserOptions::default())?;
                let mut parsed_stored = acdc_parser::Document::from(parsed.document());
                let parsed_blocks: Vec<_> = parsed_stored.take_blocks().collect();
                let block = parsed_blocks.first().ok_or("expected block")?;
                let mut visitor = HtmlVisitor::new(
                    RejectCellText,
                    converter.clone(),
                    RenderOptions::default(),
                    Diagnostics::new(&source, &mut warnings),
                );
                visitor.current_subs = vec![Substitution::SpecialChars];
                visitor.render_options.inlines_verbatim = true;
                let mut context = TraversalContext::new(parsed.document().attributes());
                let Err(error) = context.visit_block(&mut visitor, BlockRef::from(block)) else {
                    return Err(format!("body must reach the failing writer: {input}").into());
                };
                assert!(error.to_string().contains("rejected cell text"));
                assert_eq!(visitor.current_subs, [Substitution::SpecialChars]);
                assert!(visitor.render_options.inlines_verbatim);
            }
        }
        Ok(())
    }

    #[test]
    fn footnote_catalog_preserves_stored_order_and_writer_errors() -> Result<(), TestError> {
        let parsed = parse(
            "First footnote:[FIRST *note*]. Second footnote:[SECOND _note_].\n",
            &ParserOptions::default(),
        )?;
        let mut document = Document::default();
        document.set_attributes(acdc_parser::DocumentAttributes::from(
            parsed.document().attributes(),
        ));
        let mut footnotes: Vec<_> = acdc_parser::Document::from(parsed.document())
            .take_footnotes()
            .collect();
        footnotes.reverse();
        let retained = footnotes.clone();
        document.set_footnotes(footnotes);
        for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
            let converter = Rc::new(
                Converter::new(ConverterOptions::default(), ParserOptions::builder())?
                    .with_variant(variant)?,
            );
            let source = WarningSource::new("html");
            let mut warnings = Vec::new();
            let mut output = Vec::new();
            let mut visitor = HtmlVisitor::new(
                &mut output,
                converter.clone(),
                RenderOptions {
                    embedded: true,
                    ..RenderOptions::default()
                },
                Diagnostics::new(&source, &mut warnings),
            );
            let mut context =
                TraversalContext::new(acdc_parser::ast::DocumentRef::from(&document).attributes());
            visitor.visit_document_supplements(&mut context, DocumentRef::from(&document))?;
            let output = String::from_utf8(output)?;
            assert!(
                output.find("SECOND").ok_or("second definition")?
                    < output.find("FIRST").ok_or("first definition")?
            );
            assert_eq!(output.matches("SECOND").count(), 1);
            assert_eq!(output.matches("FIRST").count(), 1);
            assert!(output.contains("<strong>note</strong>"));
            assert!(output.contains("<em>note</em>"));
            let mut visitor = HtmlVisitor::new(
                RejectWrites,
                converter,
                RenderOptions {
                    embedded: true,
                    ..RenderOptions::default()
                },
                Diagnostics::new(&source, &mut warnings),
            );
            assert!(
                visitor
                    .visit_document_supplements(&mut context, DocumentRef::from(&document))
                    .is_err()
            );
        }
        assert_eq!(document.take_footnotes().collect::<Vec<_>>(), retained);
        Ok(())
    }

    #[test]
    fn paragraph_and_compound_errors_restore_substitution_state() -> Result<(), TestError> {
        let converter = Rc::new(Converter::new(
            ConverterOptions::default(),
            ParserOptions::builder(),
        )?);
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        for input in [
            "[subs=none]\nTRIGGER\n",
            "[quote,Author,Book]\n____\nTRIGGER\n____\n",
        ] {
            let parsed = parse(input, &ParserOptions::default())?;
            let mut parsed_stored = acdc_parser::Document::from(parsed.document());
            let parsed_blocks: Vec<_> = parsed_stored.take_blocks().collect();
            let block = parsed_blocks.first().ok_or("expected block")?;
            let mut visitor = HtmlVisitor::new(
                RejectCellText,
                converter.clone(),
                RenderOptions::default(),
                Diagnostics::new(&source, &mut warnings),
            );
            visitor.current_subs = vec![Substitution::SpecialChars];
            visitor.render_options.inlines_verbatim = true;
            let mut context = TraversalContext::new(parsed.document().attributes());
            let Err(error) = context.visit_block(&mut visitor, BlockRef::from(block)) else {
                return Err("body must reach the failing writer".into());
            };
            assert!(error.to_string().contains("rejected cell text"));
            assert_eq!(visitor.current_subs, [Substitution::SpecialChars]);
            assert!(visitor.render_options.inlines_verbatim);
        }
        Ok(())
    }

    #[test]
    fn paragraph_and_compound_attribution_presence_ignores_stale_scalars() -> Result<(), TestError>
    {
        let parsed = parse(
            "[quote]\nParagraph.\n\n[quote]\n____\nCompound.\n____\n",
            &ParserOptions::default(),
        )?;
        for (author, citation) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut document = acdc_parser::Document::default();
            document.set_attributes(acdc_parser::DocumentAttributes::from(
                parsed.document().attributes(),
            ));
            document.set_blocks(parsed.document().blocks().map(acdc_parser::Block::from));
            let mut blocks: Vec<_> = document.take_blocks().collect();
            for block in &mut blocks {
                let mut metadata = if let Block::Paragraph(node) = block {
                    node.clone_metadata()
                } else if let Block::DelimitedBlock(node) = block {
                    node.clone_metadata()
                } else {
                    return Err("expected quote".into());
                };
                metadata.attribution = author.then(|| Vec::new().into());
                metadata.citetitle = citation.then(|| Vec::new().into());
                metadata
                    .attributes
                    .set("attribution".into(), AttributeValue::from("SHADOW AUTHOR"));
                metadata
                    .attributes
                    .set("citetitle".into(), AttributeValue::from("SHADOW BOOK"));
                metadata.roles = vec!["", "one", "one"].into();
                if let Block::Paragraph(node) = block {
                    assert!(node.set_metadata(metadata.clone()).is_err());
                    metadata.attributes.remove("attribution");
                    metadata.attributes.remove("citetitle");
                    node.set_metadata(metadata)?;
                } else if let Block::DelimitedBlock(node) = block {
                    assert!(node.set_metadata(metadata.clone()).is_err());
                    metadata.attributes.remove("attribution");
                    metadata.attributes.remove("citetitle");
                    node.set_metadata(metadata)?;
                }
            }
            document.set_blocks(blocks);
            for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
                let converter =
                    Converter::new(ConverterOptions::default(), ParserOptions::builder())?
                        .with_variant(variant)?;
                let mut output = Vec::new();
                let source = WarningSource::new("html");
                let mut warnings = Vec::new();
                converter.convert_to_writer(
                    (&document).into(),
                    &mut output,
                    &RenderOptions {
                        embedded: true,
                        ..RenderOptions::default()
                    },
                    &mut Diagnostics::new(&source, &mut warnings),
                )?;
                let output = String::from_utf8(output)?;
                assert!(!output.contains("SHADOW"));
                assert_eq!(
                    output.matches("&#8212;").count(),
                    if author { 2 } else { 0 }
                );
                assert_eq!(
                    output.matches("<cite>").count(),
                    if citation { 2 } else { 0 }
                );
                assert_eq!(output.matches("  one one").count(), 2);
                assert_eq!(warnings, [] as [acdc_converters_core::Warning; 0]);
            }
        }
        Ok(())
    }

    #[test]
    fn nested_section_errors_restore_parent_style_and_index_title() -> Result<(), TestError> {
        let parsed = parse(
            "= Manual\n\n[bibliography]\n== Outer\n\n=== Nested\n\nTRIGGER\n",
            &ParserOptions::default(),
        )?;
        let section = parsed
            .document()
            .blocks()
            .next()
            .ok_or("expected section")?;
        let converter = Rc::new(Converter::new(
            ConverterOptions::default(),
            ParserOptions::builder(),
        )?);
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        let mut visitor = HtmlVisitor::new(
            RejectCellText,
            converter.clone(),
            RenderOptions::default(),
            Diagnostics::new(&source, &mut warnings),
        );
        visitor.section_style = Some("parent-style".into());
        visitor.current_section_title = Some("Parent title".into());
        let mut context = TraversalContext::new(parsed.document().attributes());
        let Err(error) = context.visit_block(&mut visitor, section) else {
            return Err("nested section text must reach the failing writer".into());
        };
        assert!(error.to_string().contains("rejected cell text"));
        assert_eq!(visitor.section_style.as_deref(), Some("parent-style"));
        assert_eq!(
            visitor.current_section_title.as_deref(),
            Some("Parent title")
        );
        Ok(())
    }

    #[test]
    fn constructed_section_and_discrete_ids_keep_fallbacks_and_role_order() -> Result<(), TestError>
    {
        let parsed = parse("== *Heading*\n", &ParserOptions::default())?;
        let mut parsed_stored = acdc_parser::Document::from(parsed.document());
        let parsed_blocks: Vec<_> = parsed_stored.take_blocks().collect();
        let Some(Block::Section(section)) = parsed_blocks.first() else {
            return Err("expected section".into());
        };
        let discrete = parse("[discrete]\n== *Heading*\n", &ParserOptions::default())?;
        let mut discrete_stored = acdc_parser::Document::from(discrete.document());
        let discrete_blocks: Vec<_> = discrete_stored.take_blocks().collect();
        for (primary, fallback, expected) in [
            (None, false, "_heading"),
            (None, true, "fallback"),
            (Some("primary"), true, "primary"),
            (Some(""), true, ""),
        ] {
            let mut document = acdc_parser::Document::default();
            document.set_attributes(acdc_parser::DocumentAttributes::from(
                parsed.document().attributes(),
            ));
            document.set_blocks(vec![
                Block::Section(Section::new(
                    section.clone().take_title(),
                    1,
                    Vec::new(),
                    Location::default(),
                )),
                discrete_blocks
                    .first()
                    .ok_or("expected discrete heading")?
                    .clone(),
            ]);
            let mut blocks: Vec<_> = document.take_blocks().collect();
            for block in &mut blocks {
                let mut metadata = if let Block::Section(section) = block {
                    section.clone_metadata()
                } else if let Block::DiscreteHeading(header) = block {
                    header.clone_metadata()
                } else {
                    return Err("expected heading".into());
                };
                metadata.id = primary.map(|id| Anchor::new(id, Location::default()));
                metadata.anchors.clear();
                if fallback {
                    metadata
                        .anchors
                        .push(Anchor::new("fallback", Location::default()));
                }
                metadata.roles = vec!["one", "one"].into();
                if let Block::Section(section) = block {
                    section.set_metadata(metadata)?;
                } else if let Block::DiscreteHeading(header) = block {
                    header.set_metadata(metadata)?;
                }
            }
            document.set_blocks(blocks);
            for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
                let converter =
                    Converter::new(ConverterOptions::default(), ParserOptions::builder())?
                        .with_variant(variant)?;
                let source = WarningSource::new("html");
                let mut warnings = Vec::new();
                let mut output = Vec::new();
                converter.convert_to_writer(
                    (&document).into(),
                    &mut output,
                    &RenderOptions {
                        embedded: true,
                        ..RenderOptions::default()
                    },
                    &mut Diagnostics::new(&source, &mut warnings),
                )?;
                let output = String::from_utf8(output)?;
                assert_eq!(output.matches(&format!("id=\"{expected}\"")).count(), 2);
                assert_eq!(output.matches("one one").count(), 2);
                assert_eq!(warnings, [] as [acdc_converters_core::Warning; 0]);
            }
        }
        Ok(())
    }

    #[test]
    fn title_rendering_restores_body_settings_after_success_and_error() -> Result<(), TestError> {
        let converter = Rc::new(Converter::new(
            ConverterOptions::default(),
            ParserOptions::builder(),
        )?);
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        let mut context = TraversalContext::new(converter.document_attributes());
        let settings = vec![Substitution::SpecialChars];
        let options = RenderOptions {
            embedded: true,
            inlines_verbatim: true,
            ..RenderOptions::default()
        };
        let mut visitor = HtmlVisitor::new(
            Vec::new(),
            converter.clone(),
            options.clone(),
            Diagnostics::new(&source, &mut warnings),
        );
        visitor.current_subs.clone_from(&settings);
        {
            let text = String::from("*Rich* https://example.org[link]");
            let parsed = parse_inline(&text, &ParserOptions::default())?;
            visitor.visit_title(&mut context, Some(parsed.content()))?;
        }
        assert_eq!(visitor.current_subs, settings);
        assert!(visitor.render_options.inlines_verbatim);
        assert!(String::from_utf8(visitor.into_writer())?.contains("<strong>Rich</strong>"));
        let mut visitor = HtmlVisitor::new(
            RejectWrites,
            converter.clone(),
            options,
            Diagnostics::new(&source, &mut warnings),
        );
        visitor.current_subs.clone_from(&settings);
        let parsed = parse_inline("*Rejected*", &ParserOptions::default())?;
        assert!(
            visitor
                .visit_title(&mut context, Some(parsed.content()))
                .is_err()
        );
        assert_eq!(visitor.current_subs, settings);
        assert!(visitor.render_options.inlines_verbatim);
        assert_eq!(warnings, [] as [acdc_converters_core::Warning; 0]);
        Ok(())
    }

    #[test]
    fn captioned_titles_distinguish_absent_and_empty_before_counting() -> Result<(), TestError> {
        let converter = Rc::new(Converter::new(
            ConverterOptions::default(),
            ParserOptions::builder(),
        )?);
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        let mut context = TraversalContext::new(converter.document_attributes());
        let mut visitor = HtmlVisitor::new(
            Vec::new(),
            converter.clone(),
            RenderOptions {
                embedded: true,
                ..RenderOptions::default()
            },
            Diagnostics::new(&source, &mut warnings),
        );
        let mut paragraph = acdc_parser::Paragraph::new(Vec::new(), Location::default());
        let mut metadata = paragraph.take_metadata();
        metadata.caption = Some(acdc_parser::Caption::Numbered {
            kind: CaptionKind::Figure,
            label: "Figure".into(),
            number: None,
        });
        paragraph.set_metadata(metadata)?;
        let attributes = Some(acdc_parser::ast::ParagraphRef::from(&paragraph).attributes());
        visitor.render_captioned_title_with_wrapper(
            &mut context,
            Title::default().content(),
            attributes,
            Some(CaptionKind::Figure),
            "<caption>",
            "</caption>",
        )?;
        assert_eq!(converter.figure_counter.get(), 0);
        assert_eq!(visitor.writer, [] as [u8; 0]);
        visitor.render_captioned_title_with_wrapper(
            &mut context,
            Title::new(Vec::new()).content(),
            attributes,
            Some(CaptionKind::Figure),
            "<caption>",
            "</caption>",
        )?;
        assert_eq!(converter.figure_counter.get(), 1);
        assert_eq!(
            String::from_utf8(visitor.into_writer())?,
            "<caption>Figure 1. </caption>"
        );
        Ok(())
    }

    #[test]
    fn constructed_media_preserve_scalar_controls_roles_and_identity() -> Result<(), TestError> {
        for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
            let options = ParserOptions::builder()
                .with_compatibility_mode(mode)
                .build()?;
            let parsed = parse(
                ".Image\nimage::photo.png[Alt]\n\n.Audio\naudio::song.ogg[]\n\n.Video\nvideo::clip.mp4[]\n",
                &options,
            )?;
            let mut document = acdc_parser::Document::default();
            document.set_attributes(acdc_parser::DocumentAttributes::from(
                parsed.document().attributes(),
            ));
            document.set_blocks(parsed.document().blocks().map(acdc_parser::Block::from));
            let mut blocks: Vec<_> = document.take_blocks().collect();
            for block in &mut blocks {
                let mut metadata = if let Block::Image(image) = block {
                    image.clone_metadata()
                } else if let Block::Audio(audio) = block {
                    audio.clone_metadata()
                } else if let Block::Video(video) = block {
                    video.clone_metadata()
                } else {
                    return Err("expected media".into());
                };
                metadata.id = Some(Anchor::new("primary", Location::default()));
                metadata
                    .anchors
                    .push(Anchor::new("fallback", Location::default()));
                metadata.roles = vec!["one", "one"].into();
                metadata.options = vec!["autoplay", "nocontrols"].into();
                metadata
                    .attributes
                    .set("align".into(), AttributeValue::from("'center'"));
                metadata
                    .attributes
                    .set("width".into(), AttributeValue::from("\"320\""));
                metadata
                    .attributes
                    .set("link".into(), AttributeValue::Bool(false));
                if let Block::Image(image) = block {
                    image.set_metadata(metadata)?;
                } else if let Block::Audio(audio) = block {
                    audio.set_metadata(metadata)?;
                } else if let Block::Video(video) = block {
                    video.set_metadata(metadata)?;
                } else {
                    return Err("expected media".into());
                }
            }
            document.set_blocks(blocks);
            for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
                let converter =
                    Converter::new(ConverterOptions::default(), ParserOptions::builder())?
                        .with_variant(variant)?;
                let source = WarningSource::new("html");
                let mut warnings = Vec::new();
                let mut bytes = Vec::new();
                converter.convert_to_writer(
                    (&document).into(),
                    &mut bytes,
                    &RenderOptions {
                        embedded: true,
                        ..RenderOptions::default()
                    },
                    &mut Diagnostics::new(&source, &mut warnings),
                )?;
                let output = String::from_utf8(bytes)?;
                assert_eq!(output.matches("id=\"primary\"").count(), 3);
                assert!(!output.contains("id=\"fallback\""));
                assert_eq!(output.matches("one one").count(), 3);
                assert!(output.contains("href=\"false\""));
                assert_eq!(output.matches("width=\"&quot;320&quot;\"").count(), 2);
                assert_eq!(output.matches(" autoplay").count(), 2);
                assert!(!output.contains(" controls"));
                assert_eq!(warnings, [] as [acdc_converters_core::Warning; 0]);
            }
        }
        Ok(())
    }

    #[test]
    fn table_cell_scope_restores_parent_after_a_body_write_error() -> Result<(), TestError> {
        let parsed = parse(
            "= Manual\n:scope: outer\n\n[cols=a]\n|===\na|\n:scope: inner\n\nTRIGGER\n|===\n",
            &ParserOptions::default(),
        )?;
        let block = parsed.document().blocks().next().ok_or("expected table")?;
        let converter = Rc::new(Converter::new(
            ConverterOptions::default(),
            ParserOptions::builder(),
        )?);
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        let mut visitor = HtmlVisitor::new(
            RejectCellText,
            converter.clone(),
            RenderOptions::default(),
            Diagnostics::new(&source, &mut warnings),
        );
        let mut context = TraversalContext::new(parsed.document().attributes());
        let Err(error) = context.visit_block(&mut visitor, block) else {
            return Err("cell text must reach the failing writer".into());
        };
        assert!(error.to_string().contains("rejected cell text"));
        assert_eq!(
            context.get("scope").and_then(|value| value.text()),
            Some("outer")
        );
        Ok(())
    }

    #[test]
    fn constructed_compound_blocks_keep_scalar_controls_and_id_policy() -> Result<(), TestError> {
        let parsed = parse(
            ".Note\n[NOTE]\n====\nBody.\n====\n\n.Table\n|===\n|Cell\n|===\n",
            &ParserOptions::default(),
        )?;
        let mut document = acdc_parser::Document::default();
        document.set_attributes(acdc_parser::DocumentAttributes::from(
            parsed.document().attributes(),
        ));
        document.set_blocks(parsed.document().blocks().map(acdc_parser::Block::from));
        let mut blocks: Vec<_> = document.take_blocks().collect();
        for block in &mut blocks {
            let mut metadata = if let Block::Admonition(admonition) = block {
                admonition.clone_metadata()
            } else if let Block::DelimitedBlock(block) = block {
                block.clone_metadata()
            } else {
                return Err("expected compound block".into());
            };
            metadata.id = Some(Anchor::new("primary&", Location::default()));
            metadata
                .anchors
                .push(Anchor::new("fallback", Location::default()));
            metadata.roles = vec!["one", "one"].into();
            metadata
                .attributes
                .set("width".into(), AttributeValue::from("\"75%\""));
            metadata
                .attributes
                .set("float".into(), AttributeValue::from("'right'"));
            metadata
                .attributes
                .set("icon".into(), AttributeValue::Bool(false));
            if let Block::Admonition(admonition) = block {
                admonition.set_metadata(metadata)?;
            } else if let Block::DelimitedBlock(block) = block {
                block.set_metadata(metadata)?;
            } else {
                return Err("expected compound block".into());
            }
        }
        document.set_blocks(blocks);
        for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
            let converter = Converter::new(ConverterOptions::default(), ParserOptions::builder())?
                .with_variant(variant)?;
            let source = WarningSource::new("html");
            let mut warnings = Vec::new();
            let mut output = Vec::new();
            converter.convert_to_writer(
                (&document).into(),
                &mut output,
                &RenderOptions {
                    embedded: true,
                    ..RenderOptions::default()
                },
                &mut Diagnostics::new(&source, &mut warnings),
            )?;
            let output = String::from_utf8(output)?;
            assert_eq!(output.matches("one one").count(), 2);
            assert!(!output.contains("fallback"));
            assert!(output.contains("style=\"width: &quot;75%&quot;;\""));
            assert!(output.contains("stretch 'right'") || output.contains("grid-all 'right'"));
            assert_eq!(output.matches("id=\"primary&amp;\"").count(), 2);
            assert_eq!(output.matches("id=\"primary&\"").count(), 0);
            assert_eq!(warnings, [] as [acdc_converters_core::Warning; 0]);
        }
        Ok(())
    }
    #[test]
    fn caption_values_escape_literal_text_and_preserve_counter_ownership() -> Result<(), TestError>
    {
        use acdc_parser::{Caption, ContentAttribute, Paragraph, ast::ParagraphRef};
        let converter = Rc::new(Converter::new(
            ConverterOptions::default(),
            ParserOptions::builder(),
        )?);
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        let mut context = TraversalContext::new(converter.document_attributes());
        let mut visitor = HtmlVisitor::new(
            Vec::new(),
            Rc::clone(&converter),
            RenderOptions::default(),
            Diagnostics::new(&source, &mut warnings),
        );
        let mut paragraph = Paragraph::new(Vec::new(), Location::default());
        for (caption, expected, counter) in [
            (
                Caption::Custom("<tag>&copy;".into()),
                "&lt;tag&gt;&amp;copy;",
                0,
            ),
            (Caption::Custom("".into()), "", 0),
            (Caption::CustomContent(ContentAttribute::default()), "", 0),
            (Caption::Unnumbered, "", 0),
            (
                Caption::Numbered {
                    kind: CaptionKind::Figure,
                    label: "<Figure>&copy;".into(),
                    number: std::num::NonZeroU32::new(7),
                },
                "&lt;Figure&gt;&amp;copy; 7. ",
                0,
            ),
            (
                Caption::Numbered {
                    kind: CaptionKind::Figure,
                    label: "Figure".into(),
                    number: None,
                },
                "Figure 1. ",
                1,
            ),
        ] {
            let mut metadata = paragraph.take_metadata();
            metadata.caption = Some(caption);
            paragraph.set_metadata(metadata)?;
            visitor.render_captioned_title_with_wrapper(
                &mut context,
                Title::new(Vec::new()).content(),
                Some(ParagraphRef::from(&paragraph).attributes()),
                Some(CaptionKind::Figure),
                "",
                "",
            )?;
            assert_eq!(std::str::from_utf8(&visitor.writer)?, expected);
            assert_eq!(converter.figure_counter.get(), counter);
            visitor.writer.clear();
        }
        let mut metadata = paragraph.take_metadata();
        metadata.caption = None;
        paragraph.set_metadata(metadata)?;
        visitor.render_captioned_title_with_wrapper(
            &mut context,
            Title::new(Vec::new()).content(),
            Some(ParagraphRef::from(&paragraph).attributes()),
            Some(CaptionKind::Figure),
            "",
            "",
        )?;
        assert_eq!(std::str::from_utf8(&visitor.writer)?, "Figure 2. ");
        assert_eq!(converter.figure_counter.get(), 2);
        Ok(())
    }
}

#[cfg(test)]
mod unhandled_block_tests {
    use super::*;
    use acdc_converters_core::{Options as ConverterOptions, WarningSource};
    use acdc_parser::{Options as ParserOptions, parse};

    type TestError = Box<dyn std::error::Error>;

    #[test]
    fn unhandled_parser_block_warning_is_structured() -> Result<(), TestError> {
        let parsed = parse("text", &ParserOptions::default())?;
        let converter = Converter::new_with_variant(
            ConverterOptions::default(),
            ParserOptions::builder().with_attributes(
                acdc_parser::DocumentAttributes::from(parsed.document().attributes()).into_inputs(),
            ),
            HtmlVariant::Standard,
        )?;
        let source = WarningSource::new("html").with_variant("standard");
        let mut warnings = Vec::new();
        {
            let diagnostics = Diagnostics::new(&source, &mut warnings);
            let mut visitor = HtmlVisitor::new(
                Vec::new(),
                std::rc::Rc::new(converter),
                RenderOptions::default(),
                diagnostics,
            );
            visitor.warn_unhandled_block();
        }

        let warning = warnings.first().ok_or("missing unhandled block warning")?;
        assert_eq!(warning.source.converter, "html");
        assert_eq!(warning.source.variant.as_deref(), Some("standard"));
        assert!(warning.message.contains("omitted from HTML output"));
        assert!(warning.advice().is_some());
        Ok(())
    }
}

#[cfg(test)]
mod footnote_repair_tests {
    use std::rc::Rc;

    use acdc_converters_core::{
        Converter as _, Diagnostics, Options as ConverterOptions, TraversalContext, WarningSource,
        visitor::DocumentVisitor,
    };
    use acdc_parser::{
        Block, CompatibilityMode, Document, Footnote, InlineMacro, InlineNode, Location,
        Options as ParserOptions, Paragraph, ast::DocumentRef, parse,
    };

    use super::{Converter, HtmlVariant, HtmlVisitor, RenderOptions};

    #[test]
    fn repaired_references_have_one_definition_and_matching_destinations()
    -> Result<(), Box<dyn std::error::Error>> {
        for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
            for id in [None, Some("kept")] {
                let input = if id.is_some() {
                    "footnote:kept[Retained *body*.]"
                } else {
                    "footnote:[Retained *body*.]"
                };
                let parsed = parse(
                    input,
                    &ParserOptions::builder()
                        .with_compatibility_mode(mode)
                        .build()?,
                )?;
                let mut document = Document::from(parsed.document());
                document.set_blocks([Block::Paragraph(Paragraph::new(
                    (0..2)
                        .map(|_| {
                            InlineNode::Macro(InlineMacro::Footnote(Footnote::reference(
                                id,
                                1,
                                Location::default(),
                            )))
                        })
                        .collect(),
                    Location::default(),
                ))]);
                document.rebuild_footnotes()?;
                for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
                    let converter = Rc::new(
                        Converter::new(ConverterOptions::default(), ParserOptions::builder())?
                            .with_variant(variant)?,
                    );
                    let source = WarningSource::new("html");
                    let mut warnings = Vec::new();
                    let mut output = Vec::new();
                    let mut visitor = HtmlVisitor::new(
                        &mut output,
                        converter,
                        RenderOptions {
                            embedded: true,
                            ..RenderOptions::default()
                        },
                        Diagnostics::new(&source, &mut warnings),
                    );
                    let read = DocumentRef::from(&document);
                    visitor.visit_document(&mut TraversalContext::new(read.attributes()), read)?;
                    let output = String::from_utf8(output)?;
                    let target = if variant == HtmlVariant::Standard {
                        "_footnotedef_1"
                    } else {
                        "_footnote_1"
                    };
                    assert_eq!(output.matches(&format!("id=\"{target}\"")).count(), 1);
                    assert_eq!(output.matches(&format!("href=\"#{target}\"")).count(), 2);
                    assert_eq!(output.matches("id=\"_footnoteref_1\"").count(), 1);
                    assert!(output.contains("<strong>body</strong>"));
                    assert!(warnings.is_empty(), "{warnings:?}");
                }
            }
        }
        Ok(())
    }
}
