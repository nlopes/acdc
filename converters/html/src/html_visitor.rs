//! Visitor implementation for HTML conversion.

use std::{io::Write, string::ToString};

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{
    Admonition, AttributeValue, Audio, CalloutList, DelimitedBlock, DelimitedBlockType,
    DescriptionList, DiscreteHeader, Document, DocumentAttributes, Footnote, Header, Image,
    InlineNode, ListItem, NORMAL, OrderedList, PageBreak, Paragraph, Section, Substitution,
    SubstitutionSpec, TableOfContents, ThematicBreak, UnorderedList, VERBATIM, Video,
};

use crate::{Error, HtmlVariant, Processor, RenderOptions};

/// Compute effective substitutions for a block.
///
/// This function resolves `SubstitutionSpec` to a concrete list of substitutions
/// using the appropriate baseline for the block type:
/// - Verbatim blocks (listing, literal): Use `VERBATIM` baseline
/// - Non-verbatim blocks (paragraph, etc.): Use `NORMAL` baseline
#[cfg(feature = "pre-spec-subs")]
#[must_use]
fn effective_subs(spec: Option<&SubstitutionSpec>, is_verbatim: bool) -> Vec<Substitution> {
    let baseline = if is_verbatim { VERBATIM } else { NORMAL };

    let result = match spec {
        Some(s) => s.resolve(baseline),
        None => baseline.to_vec(),
    };
    tracing::debug!(
        "effective_subs(spec={:?}, is_verbatim={}) => {:?}",
        spec,
        is_verbatim,
        result
    );
    result
}

/// Compute effective substitutions for a block (no pre-spec-subs feature).
#[cfg(not(feature = "pre-spec-subs"))]
#[must_use]
fn effective_subs(_spec: Option<&SubstitutionSpec>, is_verbatim: bool) -> Vec<Substitution> {
    if is_verbatim {
        VERBATIM.to_vec()
    } else {
        NORMAL.to_vec()
    }
}

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
        stylesdir.as_str().trim_end_matches('/'),
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

fn add_mathjax<W: Write>(writer: &mut W) -> Result<(), Error> {
    writeln!(
        writer,
        r#"<script>
MathJax = {{
      loader: {{load: ['input/asciimath']}},
      tex: {{
        processEscapes: false
      }},
      asciimath: {{
        delimiters: {{'[+]': [['\\$','\\$']]}},
        displaystyle: false
      }},
      options: {{
        ignoreHtmlClass: 'tex2jax_ignore|nostem|nolatexmath|noasciimath',
        processHtmlClass: 'tex2jax_process'
      }},
      startup: {{
        ready() {{
          MathJax.startup.defaultReady();
          MathJax.startup.promise.then(() => {{
            const asciimath = MathJax._.input.asciimath.AsciiMath;
            if (asciimath) {{
              const originalCompile = asciimath.compile;
              asciimath.compile = function(math, display) {{
                const node = math.math;
                if (node && node.parentElement && node.parentElement.parentElement &&
                  node.parentElement.parentElement.classList.contains('stemblock')) {{
                  display = true;
                }}
                return originalCompile.call(this, math, display);
              }};
            }}
          }});
        }}
      }}
}};
</script>
<script defer src="https://cdn.jsdelivr.net/npm/mathjax@4/tex-mml-chtml.js"></script>"#
    )?;
    Ok(())
}

/// HTML visitor that generates HTML from `AsciiDoc` AST
pub struct HtmlVisitor<W: Write> {
    writer: W,
    pub(crate) processor: Processor,
    pub(crate) render_options: RenderOptions,
    /// Current effective substitutions for inline rendering.
    /// Set per-block in `visit_delimited_block`, defaults to normal substitutions.
    pub(crate) current_subs: Vec<Substitution>,
    /// Current section style (e.g., "bibliography", "glossary").
    /// Set when entering a section, used by child blocks for style inheritance.
    pub(crate) section_style: Option<String>,
}

impl<W: Write> HtmlVisitor<W> {
    pub fn new(writer: W, processor: Processor, render_options: RenderOptions) -> Self {
        Self {
            writer,
            processor,
            render_options,
            current_subs: NORMAL.to_vec(),
            section_style: None,
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

        if let Some(header) = &document.header {
            // Create a temporary visitor with inlines_basic mode
            let mut temp_visitor = HtmlVisitor {
                writer: &mut self.writer,
                processor: self.processor.clone(),
                render_options: RenderOptions {
                    inlines_basic: true,
                    ..self.render_options.clone()
                },
                current_subs: self.current_subs.clone(),
                section_style: None,
            };
            let processor = temp_visitor.processor.clone();
            let options = temp_visitor.render_options.clone();
            crate::document::render_header_metadata(
                header,
                &mut temp_visitor,
                &processor,
                &options,
            )?;
        }

        // Render Google Fonts link
        writeln!(
            self.writer,
            r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css?family=Open+Sans:300,300italic,400,400italic,600,600italic%7CNoto+Serif:400,400italic,700,700italic%7CDroid+Sans+Mono:400,700">"#
        )?;

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
            // Embed stylesheet directly (default behavior)
            let css = crate::load_css(dark_mode, variant);
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
            tracing::warn!(%max_width, "`max-width` usage is not recommended. Use CSS stylesheet instead.");
            writeln!(
                self.writer,
                "<style>
#content {{
  max-width: {max_width};
}}
</style>"
            )?;
        }

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
                r#"<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@fortawesome/fontawesome-free@7.1.0/css/all.min.css">
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@fortawesome/fontawesome-free@7.1.0/css/v4-shims.min.css">"#
            )?;
        }

        writeln!(self.writer, "</head>")?;
        Ok(())
    }

    fn render_body_footer(&mut self) -> Result<(), Error> {
        if self.processor.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "<footer id=\"footer\">")?;
            writeln!(self.writer, "<div id=\"footer-text\">")?;
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

impl<W: Write> Visitor for HtmlVisitor<W> {
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
                body_classes.push(role.clone());
            }
        }

        let body_class = body_classes.join(" ");

        // Get body ID from document title metadata (anchors or explicit id)
        let body_id = doc.header.as_ref().and_then(|header| {
            // Check explicit ID from attribute list first (e.g., [id=my-id])
            if let Some(anchor) = &header.metadata.id {
                return Some(anchor.id.clone());
            }
            // Check anchors from [[id]] or [#id] syntax - use last one like asciidoctor
            header
                .metadata
                .anchors
                .last()
                .map(|anchor| anchor.id.clone())
        });

        // Render body tag with optional id from title metadata
        if let Some(id) = body_id {
            writeln!(self.writer, "<body id=\"{id}\" class=\"{body_class}\">")?;
        } else {
            writeln!(self.writer, "<body class=\"{body_class}\">")?;
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

        let processor = self.processor.clone();
        crate::toc::render(None, self, "preamble", &processor)?;
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
            let processor = self.processor.clone();
            crate::toc::render(None, self, "auto", &processor)?;
            return Ok(());
        }
        if self.processor.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "<header id=\"header\">")?;
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
                        writeln!(
                            self.writer,
                            "<span id=\"email{}\" class=\"email\">",
                            if i > 0 {
                                format!("{}", i + 1)
                            } else {
                                String::new()
                            }
                        )?;
                        writeln!(self.writer, "<a href=\"mailto:{email}\">{email}</a>")?;
                        writeln!(self.writer, "</span>")?;
                        writeln!(self.writer, "<br>")?;
                    }
                }
                // Render revision info spans
                if let Some(AttributeValue::String(revnumber)) =
                    self.processor.document_attributes.get("revnumber")
                {
                    // Strip leading "v" if present (asciidoctor behavior)
                    let version = revnumber.strip_prefix('v').unwrap_or(revnumber);
                    writeln!(
                        self.writer,
                        "<span id=\"revnumber\">version {version},</span>"
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
        let processor = self.processor.clone();
        crate::toc::render(None, self, "auto", &processor)?;
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
                let processor = self.processor.clone();
                crate::toc::render(None, self, "auto", &processor)?;
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
                let processor = self.processor.clone();
                crate::toc::render(None, self, "auto", &processor)?;
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
        let processor = self.processor.clone();
        let previous_style = self.section_style.clone();
        self.section_style.clone_from(&section.metadata.style);
        let result = crate::section::visit_section(section, self, &processor);
        self.section_style = previous_style;
        result
    }

    fn visit_paragraph(&mut self, para: &Paragraph) -> Result<(), Self::Error> {
        // Paragraphs with [literal] style are verbatim
        let is_verbatim = para.metadata.style.as_ref().is_some_and(|s| s == "literal");

        // Compute effective substitutions for this paragraph
        let original_subs = std::mem::replace(
            &mut self.current_subs,
            effective_subs(para.metadata.substitutions.as_ref(), is_verbatim),
        );

        let processor = self.processor.clone();
        let result = crate::paragraph::visit_paragraph(para, self, &processor);

        // Restore state
        self.current_subs = original_subs;

        result
    }

    fn visit_delimited_block(&mut self, block: &DelimitedBlock) -> Result<(), Self::Error> {
        let is_verbatim = matches!(
            &block.inner,
            DelimitedBlockType::DelimitedListing(_) | DelimitedBlockType::DelimitedLiteral(_)
        );

        // Compute effective substitutions for this block
        let original_subs = std::mem::replace(
            &mut self.current_subs,
            effective_subs(block.metadata.substitutions.as_ref(), is_verbatim),
        );

        // Toggle verbatim mode for verbatim blocks
        let original_verbatim = self.render_options.inlines_verbatim;
        if is_verbatim {
            self.render_options.inlines_verbatim = true;
        }

        let processor = self.processor.clone();
        let options = self.render_options.clone();
        let result = crate::delimited::visit_delimited_block(self, block, &processor, &options);

        // Restore state
        self.current_subs = original_subs;
        self.render_options.inlines_verbatim = original_verbatim;

        result
    }

    fn visit_ordered_list(&mut self, list: &OrderedList) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::list::visit_ordered_list(list, self, &processor)
    }

    fn visit_unordered_list(&mut self, list: &UnorderedList) -> Result<(), Self::Error> {
        let section_style = self.section_style.clone();
        let processor = self.processor.clone();
        crate::list::visit_unordered_list(list, self, section_style.as_deref(), &processor)
    }

    fn visit_description_list(&mut self, list: &DescriptionList) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::list::visit_description_list(list, self, &processor)
    }

    fn visit_callout_list(&mut self, list: &CalloutList) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::list::visit_callout_list(list, self, &processor)
    }

    fn visit_list_item(&mut self, _item: &ListItem) -> Result<(), Self::Error> {
        // List items are handled by their parent list visitors
        Ok(())
    }

    fn visit_admonition(&mut self, admon: &Admonition) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::admonition::visit_admonition(self, admon, &processor)
    }

    fn visit_image(&mut self, img: &Image) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::image::visit_image(img, self, &processor)
    }

    fn visit_video(&mut self, video: &Video) -> Result<(), Self::Error> {
        crate::video::visit_video(video, self)
    }

    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error> {
        crate::audio::visit_audio(audio, self)
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
        let processor = self.processor.clone();
        crate::toc::render(Some(toc), self, "macro", &processor)
    }

    fn visit_discrete_header(&mut self, header: &DiscreteHeader) -> Result<(), Self::Error> {
        crate::section::visit_discrete_header(header, self)
    }

    fn visit_inline_nodes(&mut self, nodes: &[InlineNode]) -> Result<(), Self::Error> {
        for inline in nodes {
            self.visit_inline_node(inline)?;
        }
        Ok(())
    }

    fn visit_inline_node(&mut self, node: &InlineNode) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        let options = self.render_options.clone();
        let subs = self.current_subs.clone();
        crate::inlines::visit_inline_node(node, self, &processor, &options, &subs)
    }

    fn visit_text(&mut self, text: &str) -> Result<(), Self::Error> {
        write!(self.writer, "{text}")?;
        Ok(())
    }
}

impl<W: Write> WritableVisitor for HtmlVisitor<W> {
    fn writer_mut(&mut self) -> &mut dyn Write {
        &mut self.writer
    }
}
