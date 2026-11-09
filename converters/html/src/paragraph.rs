use std::io::Write;

use acdc_converters_core::{
    TraversalContext, code::source_language, substitutions::effective_subs,
};
use acdc_parser::{
    CaptionKind,
    ast::{AttributeValueRef, ParagraphRef},
};

use crate::{
    Error, HtmlVariant, HtmlVisitor, build_class, render_pre_code, write_attribution,
    write_element_id,
};

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    /// Render a paragraph to HTML.
    ///
    /// Called by the HTML visitor's paragraph callback.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn render_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        let data = para;
        if data.style() == Some("pass") {
            self.visit_content(traversal, data.content())?;
            writeln!(self.writer)?;
            return Ok(());
        }
        if data.style() == Some("sidebar") {
            return self.render_sidebar_content(
                traversal,
                data.title(),
                data.attributes(),
                |visitor, traversal| {
                    visitor.visit_content(traversal, data.content())?;
                    writeln!(visitor.writer).map_err(Error::from)
                },
            );
        }
        // Check if this paragraph should be rendered as a literal block
        if let Some(style) = data.style()
            && style == "literal"
        {
            let class = build_class("literalblock", data.roles().iter());
            write!(self.writer, "<div")?;
            write_element_id(&mut self.writer, data.id())?;
            writeln!(self.writer, " class=\"{class}\">")?;
            self.render_title_content(
                traversal,
                data.title(),
                "<div class=\"title\">",
                "</div>\n",
            )?;
            writeln!(self.writer, "<div class=\"content\">")?;
            write!(self.writer, "<pre>")?;
            self.visit_indented_inlines(traversal, data.content(), Some(data.attributes()))?;
            writeln!(self.writer, "</pre>")?;
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</div>")?;
            return Ok(());
        }

        if data.style() == Some("abstract") {
            return self.render_abstract_paragraph(traversal, data);
        }

        // Check if this paragraph should be rendered as a collapsible example block
        if data.style() == Some("example") && data.options().contains("collapsible") {
            let is_open = data.options().contains("open");
            write!(self.writer, "<details")?;
            write_element_id(&mut self.writer, data.id())?;
            if !data.roles().is_empty() {
                write!(
                    self.writer,
                    " class=\"{}\"",
                    build_class("", data.roles().iter())
                )?;
            }
            if is_open {
                writeln!(self.writer, " open>")?;
            } else {
                writeln!(self.writer, ">")?;
            }
            if data.title().is_some() {
                self.render_title_content(
                    traversal,
                    data.title(),
                    "<summary class=\"title\">",
                    "</summary>\n",
                )?;
            } else {
                writeln!(self.writer, "<summary class=\"title\">Details</summary>")?;
            }
            writeln!(self.writer, "<div class=\"content\">")?;
            self.visit_content(traversal, data.content())?;
            writeln!(self.writer)?;
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</details>")?;
            return Ok(());
        }

        if data.style() == Some("example") {
            return self.render_example_paragraph(traversal, data);
        }

        if let Some(style) = data.style() {
            // Check if this paragraph should be rendered as a quote block
            if style == "quote" {
                let class = build_class("quoteblock", data.roles().iter());
                write!(self.writer, "<div")?;
                write_element_id(&mut self.writer, data.id())?;
                writeln!(self.writer, " class=\"{class}\">")?;
                self.render_title_content(
                    traversal,
                    data.title(),
                    "<div class=\"title\">",
                    "</div>\n",
                )?;
                writeln!(self.writer, "<blockquote>")?;
                self.visit_content(traversal, data.content())?;
                writeln!(self.writer)?;
                writeln!(self.writer, "</blockquote>")?;
                write_attribution(traversal, self, Some(data.attributes()))?;
                writeln!(self.writer, "</div>")?;
                return Ok(());
            }

            // Check if this paragraph should be rendered as a verse block
            if style == "verse" {
                let class = build_class("verseblock", data.roles().iter());
                write!(self.writer, "<div")?;
                write_element_id(&mut self.writer, data.id())?;
                writeln!(self.writer, " class=\"{class}\">")?;
                self.render_title_content(
                    traversal,
                    data.title(),
                    "<div class=\"title\">",
                    "</div>\n",
                )?;
                write!(self.writer, "<pre class=\"content\">")?;
                self.visit_content(traversal, data.content())?;
                writeln!(self.writer, "</pre>")?;
                write_attribution(traversal, self, Some(data.attributes()))?;
                writeln!(self.writer, "</div>")?;
                return Ok(());
            }

            // Check if this paragraph should be rendered as a listing/source block
            if matches!(style, "listing" | "source") {
                return self.render_listing_paragraph(traversal, para);
            }
        }

        // Regular paragraph rendering
        if self.converter.variant() == HtmlVariant::Semantic {
            let has_title = data.title().is_some();
            let has_id = data.id().is_some();
            let has_roles = !data.roles().is_empty();

            if has_title {
                // Titled paragraphs get a section wrapper
                let class = build_class("paragraph", data.roles().iter());
                write!(self.writer, "<section")?;
                write_element_id(&mut self.writer, data.id())?;
                writeln!(self.writer, " class=\"{class}\">")?;
                self.render_captioned_title_with_wrapper(
                    traversal,
                    data.title(),
                    Some(data.attributes()),
                    CaptionKind::for_style(data.style()),
                    "<h6 class=\"block-title\">",
                    "</h6>\n",
                )?;
                write!(self.writer, "<p>")?;
                self.visit_content(traversal, data.content())?;
                writeln!(self.writer, "</p>")?;
                writeln!(self.writer, "</section>")?;
            } else if has_id || has_roles {
                // Id/roles without title: put attributes directly on <p>
                write!(self.writer, "<p")?;
                if has_roles {
                    write!(
                        self.writer,
                        " class=\"{}\"",
                        build_class("", data.roles().iter())
                    )?;
                }
                write_element_id(&mut self.writer, data.id())?;
                write!(self.writer, ">")?;
                self.visit_content(traversal, data.content())?;
                writeln!(self.writer, "</p>")?;
            } else {
                // Bare paragraph — no wrapper
                write!(self.writer, "<p>")?;
                self.visit_content(traversal, data.content())?;
                writeln!(self.writer, "</p>")?;
            }
        } else {
            let class = build_class("paragraph", data.roles().iter());
            write!(self.writer, "<div")?;
            write_element_id(&mut self.writer, data.id())?;
            writeln!(self.writer, " class=\"{class}\">")?;
            self.render_captioned_title_with_wrapper(
                traversal,
                data.title(),
                Some(data.attributes()),
                CaptionKind::for_style(data.style()),
                "<div class=\"title\">",
                "</div>\n",
            )?;
            write!(self.writer, "<p>")?;
            self.visit_content(traversal, data.content())?;
            writeln!(self.writer, "</p>")?;
            writeln!(self.writer, "</div>")?;
        }
        Ok(())
    }

    fn render_abstract_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        data: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        let semantic = self.converter.variant() == HtmlVariant::Semantic;
        let has_title = data.title().is_some();
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
        let class = build_class(base_class, data.roles().iter());

        write!(self.writer, "<{tag}")?;
        write_element_id(&mut self.writer, data.id())?;
        writeln!(self.writer, " class=\"{class}\">")?;
        if has_title {
            let (open, close) = if semantic {
                ("<h6 class=\"block-title\">", "</h6>\n")
            } else {
                ("<div class=\"title\">", "</div>\n")
            };
            self.render_title_content(traversal, data.title(), open, close)?;
        }
        writeln!(self.writer, "<blockquote>")?;
        self.visit_content(traversal, data.content())?;
        writeln!(self.writer)?;
        writeln!(self.writer, "</blockquote>")?;
        writeln!(self.writer, "</{tag}>")?;
        Ok(())
    }

    fn render_example_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        data: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        let semantic = self.converter.variant() == HtmlVariant::Semantic;
        let has_title = data.title().is_some();
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
        let class = build_class(base_class, data.roles().iter());

        write!(self.writer, "<{tag}")?;
        write_element_id(&mut self.writer, data.id())?;
        writeln!(self.writer, " class=\"{class}\">")?;
        if has_title {
            let (open, close) = if semantic {
                ("<figcaption>", "</figcaption>\n")
            } else {
                ("<div class=\"title\">", "</div>\n")
            };
            self.render_captioned_title_with_wrapper(
                traversal,
                data.title(),
                Some(data.attributes()),
                Some(CaptionKind::Example),
                open,
                close,
            )?;
        }
        let content_class = if semantic { "example" } else { "content" };
        writeln!(self.writer, "<div class=\"{content_class}\">")?;
        self.visit_content(traversal, data.content())?;
        writeln!(self.writer)?;
        writeln!(self.writer, "</div>")?;
        writeln!(self.writer, "</{tag}>")?;
        Ok(())
    }

    /// Render a listing/source-styled paragraph as a listing block.
    fn render_listing_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        let data = para;
        let attributes = data.attributes();
        let language = source_language(attributes).map(str::to_owned).or_else(|| {
            matches!(
                attributes.get("style"),
                Some(AttributeValueRef::Text("source"))
            )
            .then(|| {
                traversal
                    .get("source-language")
                    .and_then(|value| value.text())
            })
            .flatten()
            .map(str::to_owned)
        });
        let source_indent =
            crate::source_indent::resolve(Some(attributes), traversal, &mut self.diagnostics);
        let subs = effective_subs(
            match attributes.get("subs") {
                Some(AttributeValueRef::Substitutions(spec)) => Some(spec),
                _ => None,
            },
            true,
        );

        if self.converter.variant() == HtmlVariant::Semantic {
            if data.title().is_some() {
                write!(self.writer, "<figure")?;
                write_element_id(&mut self.writer, data.id())?;
                let class = build_class("listing-block", data.roles().iter());
                writeln!(self.writer, " class=\"{class}\">")?;
                self.render_captioned_title_with_wrapper(
                    traversal,
                    data.title(),
                    Some(data.attributes()),
                    Some(CaptionKind::Listing),
                    "<figcaption>",
                    "</figcaption>\n",
                )?;
                render_pre_code(
                    traversal,
                    data.content(),
                    Some(attributes),
                    language.as_deref(),
                    self,
                    &subs,
                    source_indent,
                )?;
                writeln!(self.writer, "</figure>")?;
            } else {
                write!(self.writer, "<div")?;
                write_element_id(&mut self.writer, data.id())?;
                let class = build_class("listing-block", data.roles().iter());
                writeln!(self.writer, " class=\"{class}\">")?;
                render_pre_code(
                    traversal,
                    data.content(),
                    Some(attributes),
                    language.as_deref(),
                    self,
                    &subs,
                    source_indent,
                )?;
                writeln!(self.writer, "</div>")?;
            }
        } else {
            write!(self.writer, "<div")?;
            write_element_id(&mut self.writer, data.id())?;
            let class = build_class("listingblock", data.roles().iter());
            writeln!(self.writer, " class=\"{class}\">")?;

            self.render_captioned_title_with_wrapper(
                traversal,
                data.title(),
                Some(data.attributes()),
                Some(CaptionKind::Listing),
                "<div class=\"title\">",
                "</div>\n",
            )?;

            writeln!(self.writer, "<div class=\"content\">")?;
            render_pre_code(
                traversal,
                data.content(),
                Some(attributes),
                language.as_deref(),
                self,
                &subs,
                source_indent,
            )?;
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</div>")?;
        }

        Ok(())
    }
}
