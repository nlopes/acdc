use acdc_converters_core::code::detect_language;
use acdc_converters_core::visitor::{WritableVisitor, WritableVisitorExt};
use acdc_parser::{AttributeValue, Paragraph};

use crate::{Error, HtmlVariant, Processor, build_class, write_attribution, write_id};

/// Visit a paragraph using the visitor pattern
///
/// This is called from the `HtmlVisitor` trait implementation.
#[allow(clippy::too_many_lines)]
pub(crate) fn visit_paragraph<V: WritableVisitor<Error = Error>>(
    para: &Paragraph,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    // Check if this paragraph should be rendered as a literal block
    if let Some(style) = &para.metadata.style
        && style == "literal"
    {
        let mut w = visitor.writer_mut();
        let class = build_class("literalblock", &para.metadata.roles);
        writeln!(w, "<div class=\"{class}\">")?;
        let _ = w;
        visitor.render_title_with_wrapper(&para.title, "<div class=\"title\">", "</div>\n")?;
        w = visitor.writer_mut();
        writeln!(w, "<div class=\"content\">")?;
        write!(w, "<pre>")?;
        let _ = w;
        visitor.visit_inline_nodes(&para.content)?;
        w = visitor.writer_mut();
        writeln!(w, "</pre>")?;
        writeln!(w, "</div>")?;
        writeln!(w, "</div>")?;
        return Ok(());
    }

    // Check if this paragraph should be rendered as a collapsible example block
    if para.metadata.style.as_deref() == Some("example")
        && para.metadata.options.iter().any(|s| s == "collapsible")
    {
        let is_open = para.metadata.options.iter().any(|s| s == "open");
        let w = visitor.writer_mut();
        write!(w, "<details")?;
        write_id(w, &para.metadata)?;
        if is_open {
            writeln!(w, " open>")?;
        } else {
            writeln!(w, ">")?;
        }
        let _ = w;
        if para.title.is_empty() {
            let w = visitor.writer_mut();
            writeln!(w, "<summary class=\"title\">Details</summary>")?;
        } else {
            visitor.render_title_with_wrapper(
                &para.title,
                "<summary class=\"title\">",
                "</summary>\n",
            )?;
        }
        let mut w = visitor.writer_mut();
        writeln!(w, "<div class=\"content\">")?;
        let _ = w;
        visitor.visit_inline_nodes(&para.content)?;
        w = visitor.writer_mut();
        writeln!(w)?;
        writeln!(w, "</div>")?;
        writeln!(w, "</details>")?;
        return Ok(());
    }

    if let Some(style) = &para.metadata.style {
        // Check if this paragraph should be rendered as a quote block
        if style == "quote" {
            let mut w = visitor.writer_mut();
            let class = build_class("quoteblock", &para.metadata.roles);
            writeln!(w, "<div class=\"{class}\">")?;
            let _ = w;
            visitor.render_title_with_wrapper(&para.title, "<div class=\"title\">", "</div>\n")?;
            w = visitor.writer_mut();
            writeln!(w, "<blockquote>")?;
            let _ = w;
            visitor.visit_inline_nodes(&para.content)?;
            w = visitor.writer_mut();
            writeln!(w)?;
            writeln!(w, "</blockquote>")?;
            let _ = w;
            write_attribution(visitor, &para.metadata)?;
            let w = visitor.writer_mut();
            writeln!(w, "</div>")?;
            return Ok(());
        }

        // Check if this paragraph should be rendered as a verse block
        if style == "verse" {
            let mut w = visitor.writer_mut();
            let class = build_class("verseblock", &para.metadata.roles);
            writeln!(w, "<div class=\"{class}\">")?;
            let _ = w;
            visitor.render_title_with_wrapper(&para.title, "<div class=\"title\">", "</div>\n")?;
            w = visitor.writer_mut();
            write!(w, "<pre class=\"content\">")?;
            let _ = w;
            visitor.visit_inline_nodes(&para.content)?;
            let _ = visitor.writer_mut();
            write_attribution(visitor, &para.metadata)?;
            let w = visitor.writer_mut();
            writeln!(w, "</div>")?;
            return Ok(());
        }

        // Check if this paragraph should be rendered as a listing/source block
        if matches!(style.as_str(), "listing" | "source") {
            return render_listing_paragraph(para, visitor, processor);
        }
    }

    // Regular paragraph rendering
    if processor.variant() == HtmlVariant::Semantic {
        let has_title = !para.title.is_empty();
        let has_id = para.metadata.id.is_some() || !para.metadata.anchors.is_empty();
        let has_roles = !para.metadata.roles.is_empty();

        if has_title {
            // Titled paragraphs get a section wrapper
            let mut w = visitor.writer_mut();
            let class = build_class("paragraph", &para.metadata.roles);
            write!(w, "<section")?;
            write_id(w, &para.metadata)?;
            writeln!(w, " class=\"{class}\">")?;
            let _ = w;
            visitor.render_title_with_wrapper(
                &para.title,
                "<h6 class=\"block-title\">",
                "</h6>\n",
            )?;
            w = visitor.writer_mut();
            write!(w, "<p>")?;
            let _ = w;
            visitor.visit_inline_nodes(&para.content)?;
            w = visitor.writer_mut();
            writeln!(w, "</p>")?;
            writeln!(w, "</section>")?;
        } else if has_id || has_roles {
            // Id/roles without title: put attributes directly on <p>
            let mut w = visitor.writer_mut();
            write!(w, "<p")?;
            if has_roles {
                write!(w, " class=\"{}\"", para.metadata.roles.join(" "))?;
            }
            write_id(w, &para.metadata)?;
            write!(w, ">")?;
            let _ = w;
            visitor.visit_inline_nodes(&para.content)?;
            w = visitor.writer_mut();
            writeln!(w, "</p>")?;
        } else {
            // Bare paragraph — no wrapper
            let mut w = visitor.writer_mut();
            write!(w, "<p>")?;
            let _ = w;
            visitor.visit_inline_nodes(&para.content)?;
            w = visitor.writer_mut();
            writeln!(w, "</p>")?;
        }
    } else {
        let mut w = visitor.writer_mut();
        let class = build_class("paragraph", &para.metadata.roles);
        writeln!(w, "<div class=\"{class}\">")?;
        let _ = w;
        visitor.render_title_with_wrapper(&para.title, "<div class=\"title\">", "</div>\n")?;
        w = visitor.writer_mut();
        write!(w, "<p>")?;
        let _ = w;
        visitor.visit_inline_nodes(&para.content)?;
        w = visitor.writer_mut();
        writeln!(w, "</p>")?;
        writeln!(w, "</div>")?;
    }
    Ok(())
}

/// Render a listing/source-styled paragraph as a listing block.
fn render_listing_paragraph<V: WritableVisitor<Error = Error>>(
    para: &Paragraph,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    let language = detect_language(&para.metadata);
    let subs = crate::html_visitor::effective_subs(para.metadata.substitutions.as_ref(), true);

    if processor.variant() == HtmlVariant::Semantic {
        let w = visitor.writer_mut();
        if para.title.is_empty() {
            write!(w, "<div")?;
            write_id(w, &para.metadata)?;
            let class = build_class("listing-block", &para.metadata.roles);
            writeln!(w, " class=\"{class}\">")?;
            let _ = w;
            crate::render_pre_code(&para.content, language, visitor, processor, &subs)?;
            let w = visitor.writer_mut();
            writeln!(w, "</div>")?;
        } else {
            write!(w, "<figure")?;
            write_id(w, &para.metadata)?;
            let class = build_class("listing-block", &para.metadata.roles);
            writeln!(w, " class=\"{class}\">")?;
            let _ = w;
            visitor.render_title_with_wrapper(&para.title, "<figcaption>", "</figcaption>\n")?;
            crate::render_pre_code(&para.content, language, visitor, processor, &subs)?;
            let w = visitor.writer_mut();
            writeln!(w, "</figure>")?;
        }
    } else {
        let w = visitor.writer_mut();
        write!(w, "<div")?;
        write_id(w, &para.metadata)?;
        let class = build_class("listingblock", &para.metadata.roles);
        writeln!(w, " class=\"{class}\">")?;
        let _ = w;

        // Title with optional listing-caption numbering
        if !para.title.is_empty() {
            if let Some(AttributeValue::String(caption)) =
                processor.document_attributes.get("listing-caption")
            {
                let count = processor.listing_counter.get() + 1;
                processor.listing_counter.set(count);
                visitor.render_title_with_wrapper(
                    &para.title,
                    &format!("<div class=\"title\">{caption} {count}. "),
                    "</div>\n",
                )?;
            } else {
                visitor.render_title_with_wrapper(
                    &para.title,
                    "<div class=\"title\">",
                    "</div>\n",
                )?;
            }
        }

        let mut w = visitor.writer_mut();
        writeln!(w, "<div class=\"content\">")?;
        let _ = w;
        crate::render_pre_code(&para.content, language, visitor, processor, &subs)?;
        w = visitor.writer_mut();
        writeln!(w, "</div>")?;
        writeln!(w, "</div>")?;
    }

    Ok(())
}
