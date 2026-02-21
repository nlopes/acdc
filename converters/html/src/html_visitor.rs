//! Visitor implementation for HTML conversion.

use std::{io::Write, path::Path, rc::Rc};

use acdc_converters_core::{
    Diagnostics, TraversalContext, document_attribute_text, inlines_to_string,
    substitutions::TextBoundaries,
    visitor::{Visitor, WritableVisitor},
};

#[cfg(not(feature = "pre-spec-subs"))]
use acdc_converters_core::substitutions::baseline_subs;
#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::effective_subs;

use acdc_parser::{
    Admonition, Audio, Block, BlockMetadata, CalloutList, CaptionKind, DelimitedBlock,
    DelimitedBlockType, DescriptionList, DiscreteHeader, Document, DocumentAttributes, Footnote,
    Header, Image, InlineNode, ListItem, NORMAL, OrderedList, PageBreak, Paragraph, Section,
    Substitution, TableOfContents, ThematicBreak, UnorderedList, Video,
};

use crate::{Error, HtmlVariant, Processor, RenderOptions, STYLESDIR_DEFAULT, docinfo::DocInfo};

fn link_css<W: Write>(
    writer: &mut W,
    attributes: &DocumentAttributes,
    default_filename: &str,
) -> Result<(), Error> {
    // Link to external stylesheet
    let stylesdir = attributes
        .get("stylesdir")
        .and_then(|value| value.text())
        .map_or_else(|| STYLESDIR_DEFAULT.to_string(), str::to_string);

    let stylesheet = attributes
        .get("stylesheet")
        .and_then(|value| value.text())
        .filter(|value| !value.is_empty())
        .map_or_else(|| default_filename.to_string(), str::to_string);

    writeln!(
        writer,
        r#"<link rel="stylesheet" href="{}/{}">"#,
        stylesdir.trim_end_matches('/'),
        stylesheet
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
    attributes: &DocumentAttributes,
    source_dir: Option<&Path>,
    diagnostics: &mut Diagnostics<'_>,
) -> Option<String> {
    let stylesheet = attributes
        .get("stylesheet")
        .and_then(|value| value.text())
        .filter(|value| !value.is_empty())?
        .to_string();

    let stylesdir = attributes
        .get("stylesdir")
        .and_then(|value| value.text())
        .map_or_else(|| STYLESDIR_DEFAULT.to_string(), str::to_string);

    let path = if Path::new(&stylesdir).is_absolute() {
        std::path::PathBuf::from(&stylesdir).join(&stylesheet)
    } else {
        let base = source_dir.unwrap_or_else(|| Path::new("."));
        base.join(&stylesdir).join(&stylesheet)
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

/// The inline `MathJax` configuration `<script>` acdc emits when `:stem:` is set,
/// wrapping the JavaScript in `static/mathjax-config.js` (embedded at compile
/// time). Exposed for embedded-mode consumers to reproduce the same
/// configuration; its CSP `script-src` hash is [`MATHJAX_CONFIG_CSP_HASH`].
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

fn add_mathjax<W: Write>(writer: &mut W) -> Result<(), Error> {
    writeln!(writer, "{MATHJAX_CONFIG_SCRIPT}")?;
    writeln!(
        writer,
        r#"<script defer src="{MATHJAX_LOADER_URL}"></script>"#
    )?;
    Ok(())
}

/// Return whether the document enables STEM in its header, body, or a nested
/// `AsciiDoc` cell. Embedded consumers can use this to enable math rendering.
#[must_use]
pub fn uses_stem(document: &Document<'_>) -> bool {
    let resources = HeadResources::collect(document);
    resources.stem || resources.nested_stem
}

struct HeadResources {
    stem: bool,
    font_icons: bool,
    nested_stem: bool,
}

impl HeadResources {
    fn collect(document: &Document<'_>) -> Self {
        let mut traversal = TraversalContext::new(&document.attributes);
        let mut resources = Self {
            stem: traversal.contains_key("stem"),
            font_icons: traversal.get("icons").and_then(|value| value.as_str()) == Some("font"),
            nested_stem: false,
        };
        let Ok(()) = traversal.visit_blocks(&mut resources, &document.blocks);
        resources
    }
}

impl<'doc> Visitor<'doc> for HeadResources {
    type Error = std::convert::Infallible;

    fn visit_document_attribute(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        _attribute: &'doc acdc_parser::DocumentAttribute<'doc>,
    ) -> Result<(), Self::Error> {
        if traversal.is_nested_document() {
            self.nested_stem |= traversal.contains_key("stem");
        } else {
            self.stem |= traversal.contains_key("stem");
            self.font_icons |=
                traversal.get("icons").and_then(|value| value.as_str()) == Some("font");
        }
        Ok(())
    }
}

/// HTML visitor that generates HTML from `AsciiDoc` AST
pub struct HtmlVisitor<'a, 'd, W: Write> {
    pub(crate) writer: W,
    pub(crate) processor: Rc<Processor<'a>>,

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
    pub(crate) captured_raw_fragments: Option<Vec<String>>,
    /// Resolved docinfo content for injection at head, header, and footer positions.
    docinfo: DocInfo,
    text_boundaries: TextBoundaries,
}

impl<'a, 'd, W: Write> HtmlVisitor<'a, 'd, W> {
    pub fn new(
        writer: W,
        processor: Rc<Processor<'a>>,
        render_options: RenderOptions,

        mut diagnostics: Diagnostics<'d>,
    ) -> Self {
        let docinfo = if render_options.embedded {
            DocInfo::empty()
        } else {
            DocInfo::resolve(
                processor.document_attributes(),
                processor.options.safe_mode(),
                render_options.source_dir.as_deref(),
                render_options.docname.as_deref(),
                &mut diagnostics,
            )
        };
        Self {
            writer,
            processor,

            render_options,
            diagnostics,
            current_subs: NORMAL.to_vec(),
            section_style: None,
            current_section_title: None,
            captured_raw_fragments: None,
            docinfo,
            text_boundaries: TextBoundaries::BOTH,
        }
    }

    pub(crate) fn render_captioned_title_with_wrapper(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: &[InlineNode],
        metadata: &BlockMetadata<'_>,
        fallback: Option<CaptionKind>,
        prefix: &str,
        suffix: &str,
    ) -> Result<(), Error> {
        if title.is_empty() {
            return Ok(());
        }
        let caption = self
            .processor
            .caption_prefix(metadata, fallback)
            .unwrap_or_default();
        self.render_title_with_wrapper(traversal, title, &format!("{prefix}{caption}"), suffix)
    }

    /// Consume the visitor and return the writer
    pub fn into_writer(self) -> W {
        self.writer
    }

    pub(crate) const fn text_boundaries(&self) -> TextBoundaries {
        self.text_boundaries
    }

    /// Check if dark mode is enabled via the `:dark-mode:` document attribute.
    fn is_dark_mode(&self) -> bool {
        self.processor
            .document_attributes()
            .get("dark-mode")
            .is_some()
    }

    /// Emit syntax highlighting CSS in `<head>` when class-based mode is active.
    ///
    /// - Without `linkcss`: embeds CSS in a `<style>` block (default).
    /// - With `linkcss`: emits a `<link>` to `{stylesdir}/acdc-highlight.css`.
    #[cfg(feature = "highlighting")]
    fn maybe_emit_syntax_css(&mut self) -> Result<(), Error> {
        if self
            .processor
            .document_attributes()
            .get("source-highlighter")
            .is_some()
        {
            let (theme_name, mode) =
                crate::resolve_highlight_settings(self.processor.document_attributes());
            if mode == crate::syntax::HighlightMode::Class {
                let linkcss = self
                    .processor
                    .document_attributes()
                    .get("linkcss")
                    .is_some();

                if linkcss {
                    let stylesdir = self
                        .processor
                        .document_attributes()
                        .get("stylesdir")
                        .and_then(|value| value.text())
                        .map_or_else(|| STYLESDIR_DEFAULT.to_string(), str::to_string);
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
        let stylesheet_disabled = !self
            .processor
            .document_attributes()
            .contains_key("stylesheet")
            && self
                .processor
                .document_attributes()
                .is_explicit("stylesheet");

        if stylesheet_disabled {
            return Ok(());
        }

        // Render Google Fonts link (controlled by :webfonts: attribute)
        match self
            .processor
            .document_attributes()
            .get("webfonts")
            .and_then(|value| value.text())
        {
            None if self.processor.document_attributes().is_explicit("webfonts") => {
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
        let linkcss = self
            .processor
            .document_attributes()
            .get("linkcss")
            .is_some();
        let variant = self.processor.variant();
        let default_filename = match (variant, dark_mode) {
            (HtmlVariant::Semantic, true) => crate::STYLESHEET_HTML5S_DARK_MODE,
            (HtmlVariant::Semantic, false) => crate::STYLESHEET_HTML5S_LIGHT_MODE,
            (HtmlVariant::Standard, true) => crate::STYLESHEET_DARK_MODE,
            (HtmlVariant::Standard, false) => crate::STYLESHEET_LIGHT_MODE,
        };

        if linkcss {
            link_css(
                &mut self.writer,
                self.processor.document_attributes(),
                default_filename,
            )?;
        } else {
            let custom_css = resolve_custom_css(
                self.processor.document_attributes(),
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
        if let Some(max_width) = self
            .processor
            .document_attributes()
            .get("max-width")
            .and_then(|value| value.text())
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
        self.processor.document_attributes().get("csp").is_some()
    }

    /// The acdc features this document uses, for building its CSP. Mirrors the
    /// gating the head uses to decide which scripts, fonts, and CDNs it emits.
    fn csp_features(&self, stem: bool, font_icons: bool) -> crate::CspFeatures {
        let attrs = &self.processor.document_attributes();
        let stylesheet_disabled =
            !attrs.contains_key("stylesheet") && attrs.is_explicit("stylesheet");
        crate::CspFeatures {
            stem,
            webfonts: !(stylesheet_disabled
                || !attrs.contains_key("webfonts") && attrs.is_explicit("webfonts")),
            icons_font: font_icons,
            replay: cfg!(feature = "terminal"),
        }
    }

    fn render_head(
        &mut self,
        document: &'a Document<'a>,
        stem: bool,
        font_icons: bool,
    ) -> Result<(), Error> {
        let dark_mode = self.is_dark_mode();

        writeln!(
            self.writer,
            r#"<head>
<meta charset="UTF-8">
<meta http-equiv="X-UA-Compatible" content="IE=edge">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="generator" content="{}">"#,
            self.processor.options.generator_metadata()
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
                self.csp_features(stem, font_icons)
                    .content_security_policy()
            )?;
        }

        self.render_document_metadata()?;
        if let Some(header) = &document.header {
            self.render_header_metadata(header)?;
        }

        // Render stylesheet and webfonts (skipped when :!stylesheet: is set)
        self.render_stylesheet(dark_mode)?;

        // Add MathJax if stem is enabled
        if stem {
            add_mathjax(&mut self.writer)?;
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
        if self.processor.variant() == HtmlVariant::Semantic {
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
        if let Some(revnumber) = self
            .processor
            .document_attributes()
            .get("revnumber")
            .and_then(|value| value.text())
        {
            let label = document_attribute_text(
                (self.processor.document_attributes()).get("version-label"),
            )
            .unwrap_or("Version");
            writeln!(self.writer, "{label} {revnumber}<br>")?;
        }
        Ok(())
    }

    fn render_footnotes(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        footnotes: &[Footnote],
    ) -> Result<(), Error> {
        if self.processor.variant() == HtmlVariant::Semantic {
            return self.render_footnotes_semantic(traversal, footnotes);
        }
        writeln!(self.writer, "<div id=\"footnotes\">")?;
        writeln!(self.writer, "<hr>")?;
        for footnote in footnotes {
            let number = footnote.number;
            writeln!(
                self.writer,
                "<div class=\"footnote\" id=\"_footnotedef_{number}\">"
            )?;
            write!(
                self.writer,
                "<a href=\"#_footnoteref_{number}\">{number}</a>. "
            )?;
            self.visit_inline_nodes(traversal, &footnote.content)?;
            writeln!(self.writer, "</div>")?;
        }
        writeln!(self.writer, "</div>")?;
        Ok(())
    }

    fn render_footnotes_semantic(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        footnotes: &[Footnote],
    ) -> Result<(), Error> {
        writeln!(
            self.writer,
            "<section class=\"footnotes\" aria-label=\"Footnotes\" role=\"doc-endnotes\">"
        )?;
        writeln!(self.writer, "<hr>")?;
        writeln!(self.writer, "<ol class=\"footnotes\">")?;
        for footnote in footnotes {
            let number = footnote.number;
            writeln!(
                self.writer,
                "<li class=\"footnote\" id=\"_footnote_{number}\" role=\"doc-endnote\">"
            )?;
            self.visit_inline_nodes(traversal, &footnote.content)?;
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

impl<'a, W: Write> Visitor<'a> for HtmlVisitor<'a, '_, W> {
    type Error = Error;

    fn visit_unhandled_block(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _block: &'a Block<'a>,
    ) -> Result<(), Self::Error> {
        self.diagnostics.warn_with_advice(
            "an unsupported parser block variant was omitted from HTML output",
            "Use another backend for this document and report the unsupported construct.",
        );
        Ok(())
    }

    fn visit_document_start(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        doc: &'a Document<'a>,
    ) -> Result<(), Self::Error> {
        // In embedded mode, skip the document frame (DOCTYPE, html, head, body)
        if self.render_options.embedded {
            return Ok(());
        }

        writeln!(self.writer, "<!DOCTYPE html>")?;

        // Add lang attribute if not suppressed by :nolang:
        if let Some(lang_value) = doc.attributes.get("lang").and_then(|value| value.text())
            && !lang_value.is_empty()
        {
            writeln!(self.writer, "<html lang=\"{lang_value}\">")?;
        } else if doc.attributes.contains_key("lang") {
            writeln!(self.writer, "<html>")?;
        } else if doc.attributes.get("nolang").is_some() {
            // :nolang: attribute suppresses lang
            writeln!(self.writer, "<html>")?;
        } else {
            // No lang attribute and no nolang - use default "en"
            writeln!(self.writer, "<html lang=\"en\">")?;
        }

        let resources = HeadResources::collect(doc);
        self.render_head(doc, resources.stem, resources.font_icons)?;

        // Check for unsupported css-signature attribute
        if self
            .processor
            .document_attributes()
            .contains_key("css-signature")
        {
            return Err(Error::UnsupportedCssSignature);
        }

        // Build body class with doctype and optional TOC placement classes
        // Prefer document attribute :doctype: over CLI option (inline attribute wins)
        let doctype_str = self
            .processor
            .document_attributes()
            .get("doctype")
            .and_then(|value| value.as_str().map(std::string::ToString::to_string))
            .unwrap_or_else(|| self.processor.options.doctype().to_string());
        let mut body_classes = vec![doctype_str];

        if self.is_dark_mode() {
            body_classes.push("dark".to_string());
        }

        // Add TOC-related classes to body based on placement and custom toc-class
        let toc_config = acdc_converters_core::toc::Config::from_attributes(
            None,
            &TraversalContext::new(&doc.attributes),
        );
        let has_custom_toc_class = doc.attributes.get("toc-class").is_some();

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
        if let Some(header) = &doc.header {
            for role in &header.metadata.roles {
                body_classes.push(role.to_string());
            }
        }

        let body_class = body_classes.join(" ");

        // Get body ID from document title metadata (anchors or explicit id)
        let body_id = doc.header.as_ref().and_then(|header| {
            // Check explicit ID from attribute list first (e.g., [id=my-id])
            if let Some(anchor) = &header.metadata.id {
                return Some(anchor.id);
            }
            // Check anchors from [[id]] or [#id] syntax - use last one like asciidoctor
            header.metadata.anchors.last().map(|anchor| anchor.id)
        });

        // Render body tag with optional id from title metadata
        if let Some(id) = body_id {
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
        _doc: &'a Document<'a>,
    ) -> Result<(), Self::Error> {
        if self.processor.variant() == HtmlVariant::Semantic {
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
        doc: &'a Document<'a>,
    ) -> Result<(), Self::Error> {
        // Close #content (only if not in embedded mode)
        if !self.render_options.embedded {
            if self.processor.variant() == HtmlVariant::Semantic {
                writeln!(self.writer, "</main>")?;
            } else {
                writeln!(self.writer, "</div>")?;
            }
        }
        if !doc.footnotes.is_empty() {
            self.render_footnotes(traversal, &doc.footnotes)?;
        }
        // Skip footer in embedded mode
        if !self.render_options.embedded {
            self.render_body_footer()?;
            if let Some(content) = &self.docinfo.footer {
                writeln!(self.writer, "{content}")?;
            }
        }
        Ok(())
    }

    fn visit_document_end(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _doc: &'a Document<'a>,
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
        header: &Header,
    ) -> Result<(), Self::Error> {
        if self.render_options.embedded {
            // In embedded mode, render the TOC but skip the header chrome
            self.render_toc(traversal, None, "auto")?;
            return Ok(());
        }
        if self.processor.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "<header>")?;
        } else {
            writeln!(self.writer, "<div id=\"header\">")?;
        }
        if !header.title.is_empty() {
            write!(self.writer, "<h1>")?;
            self.visit_inline_nodes(traversal, &header.title)?;
            if let Some(subtitle) = &header.subtitle {
                write!(self.writer, ": ")?;
                self.visit_inline_nodes(traversal, subtitle)?;
            }
            writeln!(self.writer, "</h1>")?;
            // Output details div if there are authors or revision info
            let attributes = traversal.header();
            let revnumber = attributes.get("revnumber").and_then(|value| value.text());
            let revdate = attributes.get("revdate").and_then(|value| value.text());
            let has_revision = revnumber.is_some() || revdate.is_some();
            if !header.authors.is_empty() || has_revision {
                writeln!(self.writer, "<div class=\"details\">")?;
                for (i, author) in header.authors.iter().enumerate() {
                    write!(
                        self.writer,
                        "<span id=\"author{}\" class=\"author\">",
                        if i > 0 {
                            format!("{}", i + 1)
                        } else {
                            String::new()
                        }
                    )?;
                    write!(self.writer, "{} ", author.first_name)?;
                    if let Some(middle_name) = &author.middle_name {
                        write!(self.writer, "{middle_name} ")?;
                    }
                    write!(self.writer, "{}", author.last_name)?;
                    writeln!(self.writer, "</span><br>")?;
                    if let Some(email) = &author.email {
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
                    let label = document_attribute_text(attributes.get("version-label"))
                        .unwrap_or("Version");
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
                if let Some(revremark) = attributes.get("revremark").and_then(|value| value.text())
                {
                    writeln!(self.writer, "<br><span id=\"revremark\">{revremark}</span>")?;
                }
                writeln!(self.writer, "</div>")?;
            }
        }

        // Render TOC after header if toc="auto"
        self.render_toc(traversal, None, "auto")?;
        if self.processor.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "</header>")?;
        } else {
            writeln!(self.writer, "</div>")?; // Close #header div
        }
        Ok(())
    }

    fn visit_body_content_start(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        doc: &'a Document<'a>,
    ) -> Result<(), Self::Error> {
        if self.render_options.embedded {
            // When there's no header, the TOC hasn't been rendered yet
            if doc.header.is_none() {
                self.render_toc(traversal, None, "auto")?;
            }
            return Ok(());
        }
        // When there's no header, emit a header wrapper for the TOC
        // (matching asciidoctor which always emits <div id="header"> when TOC is enabled)
        if doc.header.is_none() && !self.processor.toc_entries.is_empty() {
            let toc_config = acdc_converters_core::toc::Config::from_attributes(
                None,
                &TraversalContext::new(self.processor.document_attributes()),
            );
            if matches!(
                toc_config.placement(),
                "auto" | "left" | "right" | "top" | "bottom"
            ) {
                if self.processor.variant() == HtmlVariant::Semantic {
                    writeln!(self.writer, "<header id=\"header\">")?;
                } else {
                    writeln!(self.writer, "<div id=\"header\">")?;
                }
                self.render_toc(traversal, None, "auto")?;
                if self.processor.variant() == HtmlVariant::Semantic {
                    writeln!(self.writer, "</header>")?;
                } else {
                    writeln!(self.writer, "</div>")?;
                }
            }
        }
        if self.processor.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "<main id=\"content\">")?;
        } else {
            writeln!(self.writer, "<div id=\"content\">")?;
        }
        Ok(())
    }

    fn visit_preamble_start(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _doc: &'a Document<'a>,
    ) -> Result<(), Self::Error> {
        if self.processor.variant() == HtmlVariant::Semantic {
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

    fn visit_section(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        section: &'a Section<'a>,
    ) -> Result<(), Self::Error> {
        let previous_style = self.section_style.clone();
        self.section_style = section
            .metadata
            .style
            .as_ref()
            .map(std::string::ToString::to_string);
        // Set before rendering the header so index terms in the section's own
        // title attribute to this section; restore the parent (or None) on exit
        // so nested sections pop back correctly.
        let previous_section_title = self.current_section_title.take();
        self.current_section_title = Some(inlines_to_string(&section.title));
        let result = self.render_section(traversal, section);
        self.current_section_title = previous_section_title;
        self.section_style = previous_style;
        result
    }

    fn visit_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: &Paragraph,
    ) -> Result<(), Self::Error> {
        // Paragraphs with [literal], [listing], or [source] style are verbatim
        let is_verbatim = para
            .metadata
            .style
            .is_some_and(|s| matches!(s, "literal" | "listing" | "source"));

        // Compute effective substitutions for this paragraph
        #[cfg(feature = "pre-spec-subs")]
        let new_subs = effective_subs(para.metadata.substitutions.as_ref(), is_verbatim);
        #[cfg(not(feature = "pre-spec-subs"))]
        let new_subs = baseline_subs(is_verbatim);
        let original_subs = std::mem::replace(&mut self.current_subs, new_subs);

        let result = self.render_paragraph(traversal, para);

        self.current_subs = original_subs;

        result
    }

    fn visit_delimited_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: &'a DelimitedBlock<'a>,
    ) -> Result<(), Self::Error> {
        let is_verbatim = matches!(
            &block.inner,
            DelimitedBlockType::DelimitedListing(_) | DelimitedBlockType::DelimitedLiteral(_)
        );

        // Compute effective substitutions for this block
        #[cfg(feature = "pre-spec-subs")]
        let new_subs = effective_subs(block.metadata.substitutions.as_ref(), is_verbatim);
        #[cfg(not(feature = "pre-spec-subs"))]
        let new_subs = baseline_subs(is_verbatim);
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

    fn visit_ordered_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: &'a OrderedList<'a>,
    ) -> Result<(), Self::Error> {
        self.render_ordered_list(traversal, list)
    }

    fn visit_unordered_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: &'a UnorderedList<'a>,
    ) -> Result<(), Self::Error> {
        let section_style = self.section_style.clone();
        self.render_unordered_list(traversal, list, section_style.as_deref())
    }

    fn visit_description_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: &'a DescriptionList<'a>,
    ) -> Result<(), Self::Error> {
        self.render_description_list(traversal, list)
    }

    fn visit_callout_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: &'a CalloutList<'a>,
    ) -> Result<(), Self::Error> {
        self.render_callout_list(traversal, list)
    }

    fn visit_list_item(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _item: &'a ListItem<'a>,
    ) -> Result<(), Self::Error> {
        // List items are handled by their parent list visitors
        Ok(())
    }

    fn visit_admonition(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        admon: &'a Admonition<'a>,
    ) -> Result<(), Self::Error> {
        self.render_admonition(traversal, admon)
    }

    fn visit_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        img: &Image,
    ) -> Result<(), Self::Error> {
        self.render_image(traversal, img)
    }

    fn visit_video(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        video: &Video,
    ) -> Result<(), Self::Error> {
        self.render_video(traversal, video)
    }

    fn visit_audio(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        audio: &Audio,
    ) -> Result<(), Self::Error> {
        self.render_audio(traversal, audio)
    }

    fn visit_thematic_break(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        br: &ThematicBreak,
    ) -> Result<(), Self::Error> {
        write!(self.writer, "<hr")?;
        if let Some(anchor) = br.anchors.first() {
            write!(self.writer, " id=\"{}\"", anchor.id)?;
        }
        writeln!(self.writer, ">")?;
        Ok(())
    }

    fn visit_page_break(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _br: &PageBreak,
    ) -> Result<(), Self::Error> {
        writeln!(
            self.writer,
            "<div style=\"page-break-after: always;\"></div>"
        )?;
        Ok(())
    }

    fn visit_table_of_contents(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        toc: &TableOfContents,
    ) -> Result<(), Self::Error> {
        self.render_toc(traversal, Some(toc), "macro")
    }

    fn visit_discrete_header(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        header: &DiscreteHeader,
    ) -> Result<(), Self::Error> {
        crate::section::visit_discrete_header(traversal, header, self)
    }

    fn visit_inline_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: &InlineNode,
    ) -> Result<(), Self::Error> {
        let saved = self.render_options.in_inline_span;
        if acdc_converters_core::visitor::is_formatting_span(node) {
            self.render_options.in_inline_span = true;
        }

        let options = self.render_options.clone();
        let subs = self.current_subs.clone();
        let result = self.render_inline_node(traversal, node, &options, &subs);

        self.render_options.in_inline_span = saved;
        result
    }

    fn visit_inline_nodes(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: &[InlineNode],
    ) -> Result<(), Self::Error> {
        let previous_boundaries = self.text_boundaries;
        let last = nodes.len().saturating_sub(1);
        let result = (|| {
            for (index, node) in nodes.iter().enumerate() {
                let follows_break =
                    index > 0 && matches!(nodes.get(index - 1), Some(InlineNode::LineBreak(_)));
                let precedes_break = matches!(nodes.get(index + 1), Some(InlineNode::LineBreak(_)));
                self.text_boundaries = TextBoundaries::new(
                    follows_break
                        || (!self.render_options.in_inline_span
                            && previous_boundaries.at_paragraph_start()
                            && index == 0),
                    precedes_break
                        || (!self.render_options.in_inline_span
                            && previous_boundaries.at_paragraph_end()
                            && index == last),
                );
                self.visit_inline_node(traversal, node)?;
            }
            Ok(())
        })();
        self.text_boundaries = previous_boundaries;
        result
    }

    fn visit_text(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        text: &str,
    ) -> Result<(), Self::Error> {
        write!(self.writer, "{text}")?;
        Ok(())
    }
}

impl<'a, W: Write> WritableVisitor<'a> for HtmlVisitor<'a, '_, W> {
    fn writer_mut(&mut self) -> &mut dyn Write {
        &mut self.writer
    }
}
