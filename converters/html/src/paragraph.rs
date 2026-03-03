use acdc_converters_core::visitor::{WritableVisitor, WritableVisitorExt};
use acdc_parser::Paragraph;

use crate::{Error, HtmlVariant, Processor, build_class, write_attribution};

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
        && para.metadata.options.contains(&"collapsible".to_string())
    {
        let is_open = para.metadata.options.contains(&"open".to_string());
        let w = visitor.writer_mut();
        write!(w, "<details")?;
        if let Some(id) = &para.metadata.id {
            write!(w, " id=\"{}\"", id.id)?;
        } else if let Some(anchor) = para.metadata.anchors.first() {
            write!(w, " id=\"{}\"", anchor.id)?;
        }
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
            if let Some(id) = &para.metadata.id {
                write!(w, " id=\"{}\"", id.id)?;
            } else if let Some(anchor) = para.metadata.anchors.first() {
                write!(w, " id=\"{}\"", anchor.id)?;
            }
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
            if let Some(id) = &para.metadata.id {
                write!(w, " id=\"{}\"", id.id)?;
            } else if let Some(anchor) = para.metadata.anchors.first() {
                write!(w, " id=\"{}\"", anchor.id)?;
            }
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
