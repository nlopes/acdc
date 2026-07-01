//! Visitor implementation for HTML conversion.

use std::{io::Write, rc::Rc, string::ToString};

use acdc_converters_core::{
    Diagnostics,
    visitor::{Visitor, WritableVisitor},
};

#[cfg(not(feature = "pre-spec-subs"))]
use acdc_converters_core::substitutions::baseline_subs;
#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::effective_subs;

use acdc_parser::{
    Admonition, AttributeValue, Audio, CalloutList, DelimitedBlock, DelimitedBlockType,
    DescriptionList, DiscreteHeader, Document, DocumentAttributes, Footnote, Header, Image,
    InlineNode, ListItem, NORMAL, OrderedList, PageBreak, Paragraph, Section, Substitution,
    TableOfContents, ThematicBreak, UnorderedList, Video,
};

use crate::{Error, HtmlVariant, Processor, RenderOptions, docinfo::DocInfo};

fn link_css<W: Write>(
    writer: &mut W,
    attributes: &DocumentAttributes,
    default_filename: &str,
) -> Result<(), Error> {
    // Link to external stylesheet
    let stylesdir = attributes
        .get("stylesdir")
        .map_or_else(|| crate::STYLESDIR_DEFAULT.to_string(), ToString::to_string);

    let stylesheet = attributes
        .get("stylesheet")
        .and_then(|v| {
            let s = v.to_string();
            if s.is_empty() { None } else { Some(s) }
        })
        .unwrap_or_else(|| default_filename.to_string());

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
    source_dir: Option<&std::path::Path>,
    diagnostics: &mut Diagnostics<'_>,
) -> Option<String> {
    let stylesheet = attributes.get("stylesheet").and_then(|v| {
        let s = v.to_string();
        if s.is_empty() { None } else { Some(s) }
    })?;

    let stylesdir = attributes
        .get("stylesdir")
        .map_or_else(|| crate::STYLESDIR_DEFAULT.to_string(), ToString::to_string);

    let path = if std::path::Path::new(&stylesdir).is_absolute() {
        std::path::PathBuf::from(&stylesdir).join(&stylesheet)
    } else {
        let base = source_dir.unwrap_or_else(|| std::path::Path::new("."));
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
/// so a host can allowlist it without `'unsafe-inline'`. The hash is the sha256 of
/// `static/mathjax-config.js` (the code between the `<script>` tags). If you edit
/// that file, recompute it with:
/// `openssl dgst -sha256 -binary static/mathjax-config.js | openssl base64`
pub const MATHJAX_CONFIG_CSP_HASH: &str = "sha256-/viRmZJXKJF/fjIESAG3yWbh5QeUhdiM/Hr7b7qKm+c=";

fn add_mathjax<W: Write>(writer: &mut W) -> Result<(), Error> {
    writeln!(writer, "{MATHJAX_CONFIG_SCRIPT}")?;
    writeln!(
        writer,
        r#"<script defer src="{MATHJAX_LOADER_URL}"></script>"#
    )?;
    Ok(())
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
    /// Resolved docinfo content for injection at head, header, and footer positions.
    docinfo: DocInfo,
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
                &processor.document_attributes,
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
            docinfo,
        }
    }

    /// Consume the visitor and return the writer
    pub fn into_writer(self) -> W {
        self.writer
    }

    /// Check if dark mode is enabled via the `:dark-mode:` document attribute.
    fn is_dark_mode(&self) -> bool {
        self.processor
            .document_attributes
            .get("dark-mode")
            .is_some_and(|v| !matches!(v, AttributeValue::Bool(false) | AttributeValue::None))
    }

    /// Emit syntax highlighting CSS in `<head>` when class-based mode is active.
    ///
    /// - Without `linkcss`: embeds CSS in a `<style>` block (default).
    /// - With `linkcss`: emits a `<link>` to `{stylesdir}/acdc-syntect.css`.
    #[cfg(feature = "highlighting")]
    fn maybe_emit_syntax_css(&mut self) -> Result<(), Error> {
        if self
            .processor
            .document_attributes
            .get("source-highlighter")
            .is_some_and(|v| !matches!(v, AttributeValue::Bool(false)))
        {
            let (theme_name, mode) =
                crate::resolve_highlight_settings(&self.processor.document_attributes);
            if mode == crate::syntax::HighlightMode::Class {
                let linkcss = self.processor.document_attributes.get("linkcss").is_some();

                if linkcss {
                    let stylesdir = self
                        .processor
                        .document_attributes
                        .get("stylesdir")
                        .map_or_else(|| crate::STYLESDIR_DEFAULT.to_string(), ToString::to_string);
                    writeln!(
                        self.writer,
                        r#"<link rel="stylesheet" href="{}/{}">"#,
                        stylesdir.trim_end_matches('/'),
                        crate::SYNTECT_STYLESHEET
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
        let stylesheet_disabled = self
            .processor
            .document_attributes
            .get("stylesheet")
            .is_some_and(|v| matches!(v, AttributeValue::Bool(false)));

        if stylesheet_disabled {
            return Ok(());
        }

        // Render Google Fonts link (controlled by :webfonts: attribute)
        match self.processor.document_attributes.get("webfonts") {
            Some(AttributeValue::Bool(false)) => {
                // :!webfonts: — skip font link entirely
            }
            Some(AttributeValue::String(custom)) if !custom.is_empty() => {
                writeln!(
                    self.writer,
                    r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css?family={custom}">"#
                )?;
            }
            _ => {
                writeln!(
                    self.writer,
                    r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css?family=Open+Sans:300,300italic,400,400italic,600,600italic%7CNoto+Serif:400,400italic,700,700italic%7CDroid+Sans+Mono:400,700">"#
                )?;
            }
        }

        // Handle stylesheet rendering based on linkcss attribute
        let linkcss = self.processor.document_attributes.get("linkcss").is_some();
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
                &self.processor.document_attributes,
                default_filename,
            )?;
        } else {
            let custom_css = resolve_custom_css(
                &self.processor.document_attributes,
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
        if let Some(AttributeValue::String(max_width)) =
            self.processor.document_attributes.get("max-width")
            && !max_width.is_empty()
        {
            self.diagnostics.warn(format!(
                "`max-width` usage is not recommended. Use CSS stylesheet instead: {max_width}"
            ));
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
        self.processor
            .document_attributes
            .get("csp")
            .is_some_and(|v| !matches!(v, AttributeValue::Bool(false) | AttributeValue::None))
    }

    /// The acdc features this document uses, for building its CSP. Mirrors the
    /// gating the head uses to decide which scripts, fonts, and CDNs it emits.
    fn csp_features(&self) -> crate::CspFeatures {
        let attrs = &self.processor.document_attributes;
        let stylesheet_disabled = attrs
            .get("stylesheet")
            .is_some_and(|v| matches!(v, AttributeValue::Bool(false)));
        crate::CspFeatures {
            stem: attrs.get("stem").is_some(),
            webfonts: !stylesheet_disabled
                && !matches!(attrs.get("webfonts"), Some(AttributeValue::Bool(false))),
            icons_font: attrs.get("icons").is_some_and(|v| v.to_string() == "font"),
            replay: cfg!(feature = "terminal"),
        }
    }

    fn render_head(&mut self, document: &Document) -> Result<(), Error> {
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
                crate::content_security_policy(&self.csp_features())
            )?;
        }

        if let Some(header) = &document.header {
            self.render_header_metadata(header)?;
        }

        // Render stylesheet and webfonts (skipped when :!stylesheet: is set)
        self.render_stylesheet(dark_mode)?;

        // Add MathJax if stem is enabled
        if self.processor.document_attributes.get("stem").is_some() {
            add_mathjax(&mut self.writer)?;
        }

        // Add Font Awesome if icons are set to font mode
        if self
            .processor
            .document_attributes
            .get("icons")
            .is_some_and(|v| v.to_string() == "font")
        {
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
        if let Some(AttributeValue::String(revnumber)) =
            self.processor.document_attributes.get("revnumber")
        {
            let label = self
                .processor
                .document_attributes
                .get_string("version-label");
            let label = label.as_deref().unwrap_or("Version");
            writeln!(self.writer, "{label} {revnumber}<br>")?;
        }
        Ok(())
    }

    fn render_footnotes(&mut self, footnotes: &[Footnote]) -> Result<(), Error> {
        if self.processor.variant() == HtmlVariant::Semantic {
            return self.render_footnotes_semantic(footnotes);
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
            self.visit_inline_nodes(&footnote.content)?;
            writeln!(self.writer, "</div>")?;
        }
        writeln!(self.writer, "</div>")?;
        Ok(())
    }

    fn render_footnotes_semantic(&mut self, footnotes: &[Footnote]) -> Result<(), Error> {
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
            self.visit_inline_nodes(&footnote.content)?;
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

impl<W: Write> Visitor for HtmlVisitor<'_, '_, W> {
    type Error = Error;

    fn visit_document_start(&mut self, doc: &Document) -> Result<(), Self::Error> {
        // In embedded mode, skip the document frame (DOCTYPE, html, head, body)
        if self.render_options.embedded {
            return Ok(());
        }

        writeln!(self.writer, "<!DOCTYPE html>")?;

        // Add lang attribute if not suppressed by :nolang:
        if let Some(lang) = doc.attributes.get("lang") {
            match lang {
                AttributeValue::String(lang_value) if !lang_value.is_empty() => {
                    writeln!(self.writer, "<html lang=\"{lang_value}\">")?;
                }
                AttributeValue::String(_) | AttributeValue::Bool(_) | AttributeValue::None | _ => {
                    writeln!(self.writer, "<html>")?;
                }
            }
        } else if doc.attributes.contains_key("nolang") {
            // :nolang: attribute suppresses lang
            writeln!(self.writer, "<html>")?;
        } else {
            // No lang attribute and no nolang - use default "en"
            writeln!(self.writer, "<html lang=\"en\">")?;
        }

        self.render_head(doc)?;

        // Check for unsupported css-signature attribute
        if self
            .processor
            .document_attributes
            .contains_key("css-signature")
        {
            return Err(Error::UnsupportedCssSignature);
        }

        // Build body class with doctype and optional TOC placement classes
        // Prefer document attribute :doctype: over CLI option (inline attribute wins)
        let doctype_str = self
            .processor
            .document_attributes
            .get("doctype")
            .map_or_else(
                || self.processor.options.doctype().to_string(),
                ToString::to_string,
            );
        let mut body_classes = vec![doctype_str];

        if self.is_dark_mode() {
            body_classes.push("dark".to_string());
        }

        // Add TOC-related classes to body based on placement and custom toc-class
        let toc_config = acdc_converters_core::toc::Config::from_attributes(None, &doc.attributes);
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

    fn visit_preamble_end(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        if self.processor.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "</section>")?;
        } else {
            writeln!(self.writer, "</div>")?; // Close sectionbody
            writeln!(self.writer, "</div>")?; // Close preamble
        }

        self.render_toc(None, "preamble")?;
        Ok(())
    }

    fn visit_document_supplements(&mut self, doc: &Document) -> Result<(), Self::Error> {
        // Close #content (only if not in embedded mode)
        if !self.render_options.embedded {
            if self.processor.variant() == HtmlVariant::Semantic {
                writeln!(self.writer, "</main>")?;
            } else {
                writeln!(self.writer, "</div>")?;
            }
        }
        if !doc.footnotes.is_empty() {
            self.render_footnotes(&doc.footnotes)?;
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

    fn visit_document_end(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        // In embedded mode, skip the closing document frame tags
        if self.render_options.embedded {
            return Ok(());
        }

        writeln!(self.writer, "</body>")?;
        write!(self.writer, "</html>")?;

        Ok(())
    }

    fn visit_header(&mut self, header: &Header) -> Result<(), Self::Error> {
        if self.render_options.embedded {
            // In embedded mode, render the TOC but skip the header chrome
            self.render_toc(None, "auto")?;
            return Ok(());
        }
        if self.processor.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "<header>")?;
        } else {
            writeln!(self.writer, "<div id=\"header\">")?;
        }
        if !header.title.is_empty() {
            write!(self.writer, "<h1>")?;
            self.visit_inline_nodes(&header.title)?;
            if let Some(subtitle) = &header.subtitle {
                write!(self.writer, ": ")?;
                self.visit_inline_nodes(subtitle)?;
            }
            writeln!(self.writer, "</h1>")?;
            // Output details div if there are authors or revision info
            let has_revision = matches!(
                self.processor.document_attributes.get("revnumber"),
                Some(AttributeValue::String(_))
            ) || matches!(
                self.processor.document_attributes.get("revdate"),
                Some(AttributeValue::String(_))
            );
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
                let has_revdate = matches!(
                    self.processor.document_attributes.get("revdate"),
                    Some(AttributeValue::String(_))
                );
                if let Some(AttributeValue::String(revnumber)) =
                    self.processor.document_attributes.get("revnumber")
                {
                    let label = self
                        .processor
                        .document_attributes
                        .get_string("version-label");
                    let label = label.as_deref().unwrap_or("Version");
                    writeln!(
                        self.writer,
                        "<span id=\"revnumber\">{} {revnumber}{}</span>",
                        label.to_lowercase(),
                        if has_revdate { "," } else { "" }
                    )?;
                }
                if let Some(AttributeValue::String(revdate)) =
                    self.processor.document_attributes.get("revdate")
                {
                    writeln!(self.writer, "<span id=\"revdate\">{revdate}</span>")?;
                }
                if let Some(AttributeValue::String(revremark)) =
                    self.processor.document_attributes.get("revremark")
                {
                    writeln!(self.writer, "<br><span id=\"revremark\">{revremark}</span>")?;
                }
                writeln!(self.writer, "</div>")?;
            }
        }

        // Render TOC after header if toc="auto"
        self.render_toc(None, "auto")?;
        if self.processor.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "</header>")?;
        } else {
            writeln!(self.writer, "</div>")?; // Close #header div
        }
        Ok(())
    }

    fn visit_body_content_start(&mut self, doc: &Document) -> Result<(), Self::Error> {
        if self.render_options.embedded {
            // When there's no header, the TOC hasn't been rendered yet
            if doc.header.is_none() {
                self.render_toc(None, "auto")?;
            }
            return Ok(());
        }
        // When there's no header, emit a header wrapper for the TOC
        // (matching asciidoctor which always emits <div id="header"> when TOC is enabled)
        if doc.header.is_none() && !self.processor.toc_entries.is_empty() {
            let toc_config = acdc_converters_core::toc::Config::from_attributes(
                None,
                &self.processor.document_attributes,
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
                self.render_toc(None, "auto")?;
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

    fn visit_preamble_start(&mut self, _doc: &Document) -> Result<(), Self::Error> {
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

    fn visit_section(&mut self, section: &Section) -> Result<(), Self::Error> {
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
        self.current_section_title = Some(acdc_parser::inlines_to_string(&section.title));
        let result = self.render_section(section);
        self.current_section_title = previous_section_title;
        self.section_style = previous_style;
        result
    }

    fn visit_paragraph(&mut self, para: &Paragraph) -> Result<(), Self::Error> {
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

        // Set hardbreaks if the paragraph option or document attribute is present
        let original_hardbreaks = self.render_options.hardbreaks;
        if para.metadata.options.contains(&"hardbreaks")
            || self
                .processor
                .document_attributes()
                .contains_key("hardbreaks")
        {
            self.render_options.hardbreaks = true;
        }

        let result = self.render_paragraph(para);

        // Restore state
        self.current_subs = original_subs;
        self.render_options.hardbreaks = original_hardbreaks;

        result
    }

    fn visit_delimited_block(&mut self, block: &DelimitedBlock) -> Result<(), Self::Error> {
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

        let result = self.render_delimited_block(block);

        // Restore state
        self.current_subs = original_subs;
        self.render_options.inlines_verbatim = original_verbatim;

        result
    }

    fn visit_ordered_list(&mut self, list: &OrderedList) -> Result<(), Self::Error> {
        self.render_ordered_list(list)
    }

    fn visit_unordered_list(&mut self, list: &UnorderedList) -> Result<(), Self::Error> {
        let section_style = self.section_style.clone();
        self.render_unordered_list(list, section_style.as_deref())
    }

    fn visit_description_list(&mut self, list: &DescriptionList) -> Result<(), Self::Error> {
        self.render_description_list(list)
    }

    fn visit_callout_list(&mut self, list: &CalloutList) -> Result<(), Self::Error> {
        self.render_callout_list(list)
    }

    fn visit_list_item(&mut self, _item: &ListItem) -> Result<(), Self::Error> {
        // List items are handled by their parent list visitors
        Ok(())
    }

    fn visit_admonition(&mut self, admon: &Admonition) -> Result<(), Self::Error> {
        self.render_admonition(admon)
    }

    fn visit_image(&mut self, img: &Image) -> Result<(), Self::Error> {
        self.render_image(img)
    }

    fn visit_video(&mut self, video: &Video) -> Result<(), Self::Error> {
        self.render_video(video)
    }

    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error> {
        self.render_audio(audio)
    }

    fn visit_thematic_break(&mut self, br: &ThematicBreak) -> Result<(), Self::Error> {
        if !br.title.is_empty() {
            write!(self.writer, "<div class=\"title\">")?;
            self.visit_inline_nodes(&br.title)?;
            writeln!(self.writer, "</div>")?;
        }
        writeln!(self.writer, "<hr>")?;
        Ok(())
    }

    fn visit_page_break(&mut self, _br: &PageBreak) -> Result<(), Self::Error> {
        writeln!(
            self.writer,
            "<div style=\"page-break-after: always;\"></div>"
        )?;
        Ok(())
    }

    fn visit_table_of_contents(&mut self, toc: &TableOfContents) -> Result<(), Self::Error> {
        self.render_toc(Some(toc), "macro")
    }

    fn visit_discrete_header(&mut self, header: &DiscreteHeader) -> Result<(), Self::Error> {
        crate::section::visit_discrete_header(header, self)
    }

    fn visit_inline_node(&mut self, node: &InlineNode) -> Result<(), Self::Error> {
        let saved = self.render_options.in_inline_span;
        if acdc_converters_core::visitor::is_formatting_span(node) {
            self.render_options.in_inline_span = true;
        }

        let options = self.render_options.clone();
        let subs = self.current_subs.clone();
        let result = self.render_inline_node(node, &options, &subs);

        self.render_options.in_inline_span = saved;
        result
    }

    fn visit_text(&mut self, text: &str) -> Result<(), Self::Error> {
        write!(self.writer, "{text}")?;
        Ok(())
    }
}

impl<W: Write> WritableVisitor for HtmlVisitor<'_, '_, W> {
    fn writer_mut(&mut self) -> &mut dyn Write {
        &mut self.writer
    }
}
