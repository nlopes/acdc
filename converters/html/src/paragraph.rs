use std::io::Write;

#[cfg(not(feature = "pre-spec-subs"))]
use acdc_converters_core::substitutions::baseline_subs;
#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::effective_subs;
use acdc_converters_core::{
    TraversalContext,
    code::detect_language,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{CaptionKind, Paragraph};

use crate::{
    Error, HtmlVariant, HtmlVisitor, build_class, render_pre_code, write_attribution, write_id,
};

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    /// Render a paragraph to HTML.
    ///
    /// Called by the HTML visitor's paragraph callback.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn render_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: &Paragraph,
    ) -> Result<(), Error> {
        // Check if this paragraph should be rendered as a literal block
        if let Some(style) = para.metadata.style
            && style == "literal"
        {
            let class = build_class("literalblock", &para.metadata.roles);
            write!(self.writer, "<div")?;
            write_id(&mut self.writer, &para.metadata)?;
            writeln!(self.writer, " class=\"{class}\">")?;
            self.render_title_with_wrapper(
                traversal,
                &para.title,
                "<div class=\"title\">",
                "</div>\n",
            )?;
            writeln!(self.writer, "<div class=\"content\">")?;
            write!(self.writer, "<pre>")?;
            self.visit_inline_nodes(traversal, &para.content)?;
            writeln!(self.writer, "</pre>")?;
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</div>")?;
            return Ok(());
        }

        if para.metadata.style == Some("abstract") {
            return self.render_abstract_paragraph(traversal, para);
        }

        // Check if this paragraph should be rendered as a collapsible example block
        if para.metadata.style == Some("example") && para.metadata.options.contains(&"collapsible")
        {
            let is_open = para.metadata.options.contains(&"open");
            write!(self.writer, "<details")?;
            write_id(&mut self.writer, &para.metadata)?;
            if !para.metadata.roles.is_empty() {
                write!(self.writer, " class=\"{}\"", para.metadata.roles.join(" "))?;
            }
            if is_open {
                writeln!(self.writer, " open>")?;
            } else {
                writeln!(self.writer, ">")?;
            }
            if para.title.is_empty() {
                writeln!(self.writer, "<summary class=\"title\">Details</summary>")?;
            } else {
                self.render_title_with_wrapper(
                    traversal,
                    &para.title,
                    "<summary class=\"title\">",
                    "</summary>\n",
                )?;
            }
            writeln!(self.writer, "<div class=\"content\">")?;
            self.visit_inline_nodes(traversal, &para.content)?;
            writeln!(self.writer)?;
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</details>")?;
            return Ok(());
        }

        if para.metadata.style == Some("example") {
            return self.render_example_paragraph(traversal, para);
        }

        if let Some(style) = para.metadata.style {
            // Check if this paragraph should be rendered as a quote block
            if style == "quote" {
                let class = build_class("quoteblock", &para.metadata.roles);
                write!(self.writer, "<div")?;
                write_id(&mut self.writer, &para.metadata)?;
                writeln!(self.writer, " class=\"{class}\">")?;
                self.render_title_with_wrapper(
                    traversal,
                    &para.title,
                    "<div class=\"title\">",
                    "</div>\n",
                )?;
                writeln!(self.writer, "<blockquote>")?;
                self.visit_inline_nodes(traversal, &para.content)?;
                writeln!(self.writer)?;
                writeln!(self.writer, "</blockquote>")?;
                write_attribution(traversal, self, &para.metadata)?;
                writeln!(self.writer, "</div>")?;
                return Ok(());
            }

            // Check if this paragraph should be rendered as a verse block
            if style == "verse" {
                let class = build_class("verseblock", &para.metadata.roles);
                write!(self.writer, "<div")?;
                write_id(&mut self.writer, &para.metadata)?;
                writeln!(self.writer, " class=\"{class}\">")?;
                self.render_title_with_wrapper(
                    traversal,
                    &para.title,
                    "<div class=\"title\">",
                    "</div>\n",
                )?;
                write!(self.writer, "<pre class=\"content\">")?;
                self.visit_inline_nodes(traversal, &para.content)?;
                writeln!(self.writer, "</pre>")?;
                write_attribution(traversal, self, &para.metadata)?;
                writeln!(self.writer, "</div>")?;
                return Ok(());
            }

            // Check if this paragraph should be rendered as a listing/source block
            if matches!(style, "listing" | "source") {
                return self.render_listing_paragraph(traversal, para);
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
                self.render_captioned_title_with_wrapper(
                    traversal,
                    &para.title,
                    &para.metadata,
                    CaptionKind::for_style(para.metadata.style),
                    "<h6 class=\"block-title\">",
                    "</h6>\n",
                )?;
                write!(self.writer, "<p>")?;
                self.visit_inline_nodes(traversal, &para.content)?;
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
                self.visit_inline_nodes(traversal, &para.content)?;
                writeln!(self.writer, "</p>")?;
            } else {
                // Bare paragraph — no wrapper
                write!(self.writer, "<p>")?;
                self.visit_inline_nodes(traversal, &para.content)?;
                writeln!(self.writer, "</p>")?;
            }
        } else {
            let class = build_class("paragraph", &para.metadata.roles);
            write!(self.writer, "<div")?;
            write_id(&mut self.writer, &para.metadata)?;
            writeln!(self.writer, " class=\"{class}\">")?;
            self.render_captioned_title_with_wrapper(
                traversal,
                &para.title,
                &para.metadata,
                CaptionKind::for_style(para.metadata.style),
                "<div class=\"title\">",
                "</div>\n",
            )?;
            write!(self.writer, "<p>")?;
            self.visit_inline_nodes(traversal, &para.content)?;
            writeln!(self.writer, "</p>")?;
            writeln!(self.writer, "</div>")?;
        }
        Ok(())
    }

    fn render_abstract_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: &Paragraph,
    ) -> Result<(), Error> {
        let semantic = self.processor.variant() == HtmlVariant::Semantic;
        let has_title = !para.title.is_empty();
        let tag = if semantic && has_title {
            "section"
        } else {
            "div"
        };
        let base_class = if semantic {
            "quote-block abstract"
        } else {
            "quoteblock abstract"
        };
        let class = build_class(base_class, &para.metadata.roles);

        write!(self.writer, "<{tag}")?;
        write_id(&mut self.writer, &para.metadata)?;
        writeln!(self.writer, " class=\"{class}\">")?;
        if has_title {
            let (open, close) = if semantic {
                ("<h6 class=\"block-title\">", "</h6>\n")
            } else {
                ("<div class=\"title\">", "</div>\n")
            };
            self.render_title_with_wrapper(traversal, &para.title, open, close)?;
        }
        writeln!(self.writer, "<blockquote>")?;
        self.visit_inline_nodes(traversal, &para.content)?;
        writeln!(self.writer)?;
        writeln!(self.writer, "</blockquote>")?;
        writeln!(self.writer, "</{tag}>")?;
        Ok(())
    }

    fn render_example_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: &Paragraph,
    ) -> Result<(), Error> {
        let semantic = self.processor.variant() == HtmlVariant::Semantic;
        let has_title = !para.title.is_empty();
        let tag = if semantic && has_title {
            "figure"
        } else {
            "div"
        };
        let base_class = if semantic {
            "example-block"
        } else {
            "exampleblock"
        };
        let class = build_class(base_class, &para.metadata.roles);

        write!(self.writer, "<{tag}")?;
        write_id(&mut self.writer, &para.metadata)?;
        writeln!(self.writer, " class=\"{class}\">")?;
        if has_title {
            let (open, close) = if semantic {
                ("<figcaption>", "</figcaption>\n")
            } else {
                ("<div class=\"title\">", "</div>\n")
            };
            self.render_captioned_title_with_wrapper(
                traversal,
                &para.title,
                &para.metadata,
                Some(CaptionKind::Example),
                open,
                close,
            )?;
        }
        let content_class = if semantic { "example" } else { "content" };
        writeln!(self.writer, "<div class=\"{content_class}\">")?;
        self.visit_inline_nodes(traversal, &para.content)?;
        writeln!(self.writer)?;
        writeln!(self.writer, "</div>")?;
        writeln!(self.writer, "</{tag}>")?;
        Ok(())
    }

    /// Render a listing/source-styled paragraph as a listing block.
    fn render_listing_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: &Paragraph,
    ) -> Result<(), Error> {
        let language = detect_language(&para.metadata)
            .map(str::to_owned)
            .or_else(|| {
                (para.metadata.style == Some("source"))
                    .then(|| {
                        traversal
                            .get("source-language")
                            .and_then(|value| value.text())
                    })
                    .flatten()
                    .map(str::to_owned)
            });
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
                render_pre_code(
                    traversal,
                    &para.content,
                    &para.metadata,
                    language.as_deref(),
                    self,
                    &subs,
                )?;
                writeln!(self.writer, "</div>")?;
            } else {
                write!(self.writer, "<figure")?;
                write_id(&mut self.writer, &para.metadata)?;
                let class = build_class("listing-block", &para.metadata.roles);
                writeln!(self.writer, " class=\"{class}\">")?;
                self.render_captioned_title_with_wrapper(
                    traversal,
                    &para.title,
                    &para.metadata,
                    Some(CaptionKind::Listing),
                    "<figcaption>",
                    "</figcaption>\n",
                )?;
                render_pre_code(
                    traversal,
                    &para.content,
                    &para.metadata,
                    language.as_deref(),
                    self,
                    &subs,
                )?;
                writeln!(self.writer, "</figure>")?;
            }
        } else {
            write!(self.writer, "<div")?;
            write_id(&mut self.writer, &para.metadata)?;
            let class = build_class("listingblock", &para.metadata.roles);
            writeln!(self.writer, " class=\"{class}\">")?;

            self.render_captioned_title_with_wrapper(
                traversal,
                &para.title,
                &para.metadata,
                Some(CaptionKind::Listing),
                "<div class=\"title\">",
                "</div>\n",
            )?;

            writeln!(self.writer, "<div class=\"content\">")?;
            render_pre_code(
                traversal,
                &para.content,
                &para.metadata,
                language.as_deref(),
                self,
                &subs,
            )?;
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</div>")?;
        }

        Ok(())
    }
}
