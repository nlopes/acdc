use std::io::Write;

use acdc_converters_core::code::detect_language;
#[cfg(not(feature = "pre-spec-subs"))]
use acdc_converters_core::substitutions::baseline_subs;
#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::effective_subs;
use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{AttributeValue, Paragraph};

use crate::{Error, HtmlVariant, HtmlVisitor, build_class, write_attribution, write_id};

impl<W: Write> HtmlVisitor<'_, '_, W> {
    /// Render a paragraph to HTML.
    ///
    /// This is called from the `HtmlVisitor` trait implementation.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn render_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        // Check if this paragraph should be rendered as a literal block
        if let Some(style) = para.metadata.style
            && style == "literal"
        {
            let class = build_class("literalblock", &para.metadata.roles);
            writeln!(self.writer, "<div class=\"{class}\">")?;
            self.render_title_with_wrapper(&para.title, "<div class=\"title\">", "</div>\n")?;
            writeln!(self.writer, "<div class=\"content\">")?;
            write!(self.writer, "<pre>")?;
            self.visit_inline_nodes(&para.content)?;
            writeln!(self.writer, "</pre>")?;
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</div>")?;
            return Ok(());
        }

        // Check if this paragraph should be rendered as a collapsible example block
        if para.metadata.style == Some("example") && para.metadata.options.contains(&"collapsible")
        {
            let is_open = para.metadata.options.contains(&"open");
            write!(self.writer, "<details")?;
            write_id(&mut self.writer, &para.metadata)?;
            if is_open {
                writeln!(self.writer, " open>")?;
            } else {
                writeln!(self.writer, ">")?;
            }
            if para.title.is_empty() {
                writeln!(self.writer, "<summary class=\"title\">Details</summary>")?;
            } else {
                self.render_title_with_wrapper(
                    &para.title,
                    "<summary class=\"title\">",
                    "</summary>\n",
                )?;
            }
            writeln!(self.writer, "<div class=\"content\">")?;
            self.visit_inline_nodes(&para.content)?;
            writeln!(self.writer)?;
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</details>")?;
            return Ok(());
        }

        if let Some(style) = para.metadata.style {
            // Check if this paragraph should be rendered as a quote block
            if style == "quote" {
                let class = build_class("quoteblock", &para.metadata.roles);
                writeln!(self.writer, "<div class=\"{class}\">")?;
                self.render_title_with_wrapper(&para.title, "<div class=\"title\">", "</div>\n")?;
                writeln!(self.writer, "<blockquote>")?;
                self.visit_inline_nodes(&para.content)?;
                writeln!(self.writer)?;
                writeln!(self.writer, "</blockquote>")?;
                write_attribution(self, &para.metadata)?;
                writeln!(self.writer, "</div>")?;
                return Ok(());
            }

            // Check if this paragraph should be rendered as a verse block
            if style == "verse" {
                let class = build_class("verseblock", &para.metadata.roles);
                writeln!(self.writer, "<div class=\"{class}\">")?;
                self.render_title_with_wrapper(&para.title, "<div class=\"title\">", "</div>\n")?;
                write!(self.writer, "<pre class=\"content\">")?;
                self.visit_inline_nodes(&para.content)?;
                write_attribution(self, &para.metadata)?;
                writeln!(self.writer, "</div>")?;
                return Ok(());
            }

            // Check if this paragraph should be rendered as a listing/source block
            if matches!(style, "listing" | "source") {
                return self.render_listing_paragraph(para);
            }
        }

        // Regular paragraph rendering
        if self.processor.variant() == HtmlVariant::Semantic {
            let has_title = !para.title.is_empty();
            let has_id = para.metadata.id.is_some() || !para.metadata.anchors.is_empty();
            let has_roles = !para.metadata.roles.is_empty();

            if has_title {
                // Titled paragraphs get a section wrapper
                let class = build_class("paragraph", &para.metadata.roles);
                write!(self.writer, "<section")?;
                write_id(&mut self.writer, &para.metadata)?;
                writeln!(self.writer, " class=\"{class}\">")?;
                self.render_title_with_wrapper(
                    &para.title,
                    "<h6 class=\"block-title\">",
                    "</h6>\n",
                )?;
                write!(self.writer, "<p>")?;
                self.visit_inline_nodes(&para.content)?;
                writeln!(self.writer, "</p>")?;
                writeln!(self.writer, "</section>")?;
            } else if has_id || has_roles {
                // Id/roles without title: put attributes directly on <p>
                write!(self.writer, "<p")?;
                if has_roles {
                    write!(self.writer, " class=\"{}\"", para.metadata.roles.join(" "))?;
                }
                write_id(&mut self.writer, &para.metadata)?;
                write!(self.writer, ">")?;
                self.visit_inline_nodes(&para.content)?;
                writeln!(self.writer, "</p>")?;
            } else {
                // Bare paragraph — no wrapper
                write!(self.writer, "<p>")?;
                self.visit_inline_nodes(&para.content)?;
                writeln!(self.writer, "</p>")?;
            }
        } else {
            let class = build_class("paragraph", &para.metadata.roles);
            writeln!(self.writer, "<div class=\"{class}\">")?;
            self.render_title_with_wrapper(&para.title, "<div class=\"title\">", "</div>\n")?;
            write!(self.writer, "<p>")?;
            self.visit_inline_nodes(&para.content)?;
            writeln!(self.writer, "</p>")?;
            writeln!(self.writer, "</div>")?;
        }
        Ok(())
    }

    /// Render a listing/source-styled paragraph as a listing block.
    fn render_listing_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        let language = detect_language(&para.metadata);
        #[cfg(feature = "pre-spec-subs")]
        let subs = effective_subs(para.metadata.substitutions.as_ref(), true);
        #[cfg(not(feature = "pre-spec-subs"))]
        let subs = baseline_subs(true);

        if self.processor.variant() == HtmlVariant::Semantic {
            if para.title.is_empty() {
                write!(self.writer, "<div")?;
                write_id(&mut self.writer, &para.metadata)?;
                let class = build_class("listing-block", &para.metadata.roles);
                writeln!(self.writer, " class=\"{class}\">")?;
                crate::render_pre_code(&para.content, language, self, &subs)?;
                writeln!(self.writer, "</div>")?;
            } else {
                write!(self.writer, "<figure")?;
                write_id(&mut self.writer, &para.metadata)?;
                let class = build_class("listing-block", &para.metadata.roles);
                writeln!(self.writer, " class=\"{class}\">")?;
                self.render_title_with_wrapper(&para.title, "<figcaption>", "</figcaption>\n")?;
                crate::render_pre_code(&para.content, language, self, &subs)?;
                writeln!(self.writer, "</figure>")?;
            }
        } else {
            write!(self.writer, "<div")?;
            write_id(&mut self.writer, &para.metadata)?;
            let class = build_class("listingblock", &para.metadata.roles);
            writeln!(self.writer, " class=\"{class}\">")?;

            // Title with optional listing-caption numbering
            if !para.title.is_empty() {
                if let Some(AttributeValue::String(caption)) =
                    self.processor.document_attributes.get("listing-caption")
                {
                    let count = self.processor.listing_counter.get() + 1;
                    self.processor.listing_counter.set(count);
                    self.render_title_with_wrapper(
                        &para.title,
                        &format!("<div class=\"title\">{caption} {count}. "),
                        "</div>\n",
                    )?;
                } else {
                    self.render_title_with_wrapper(
                        &para.title,
                        "<div class=\"title\">",
                        "</div>\n",
                    )?;
                }
            }

            writeln!(self.writer, "<div class=\"content\">")?;
            crate::render_pre_code(&para.content, language, self, &subs)?;
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</div>")?;
        }

        Ok(())
    }
}
