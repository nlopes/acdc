use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::Paragraph;

use crate::{Error, ManpageHtmlVisitor, escape::extract_plain_text};

pub(crate) fn visit_paragraph<W: Write>(
    para: &Paragraph,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    if let Some(style) = &para.metadata.style {
        match style.as_str() {
            "quote" => return render_quote_paragraph(para, visitor),
            "verse" => return render_verse_paragraph(para, visitor),
            "literal" => return render_literal_paragraph(para, visitor),
            _ => {}
        }
    }

    if !para.title.is_empty() {
        write!(visitor.writer_mut(), "<p class=\"Pp title\"><b>")?;
        visitor.visit_inline_nodes(&para.title)?;
        write!(visitor.writer_mut(), "</b></p>")?;
    }

    write!(visitor.writer_mut(), "<p class=\"Pp\">")?;
    visitor.visit_inline_nodes(&para.content)?;
    write!(visitor.writer_mut(), "</p>")?;

    Ok(())
}

fn render_quote_paragraph<W: Write>(
    para: &Paragraph,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    write!(visitor.writer_mut(), "<blockquote class=\"Bd-indent\">")?;
    write!(visitor.writer_mut(), "<p class=\"Pp\">")?;
    visitor.visit_inline_nodes(&para.content)?;
    write!(visitor.writer_mut(), "</p>")?;

    let attribution = para.metadata.attributes.get_string("attribution");
    let citation = para.metadata.attributes.get_string("citation");
    if attribution.is_some() || citation.is_some() {
        write!(visitor.writer_mut(), "<footer>")?;
        if let Some(cite) = citation {
            write!(
                visitor.writer_mut(),
                "{} ",
                crate::escape::escape_html(&cite)
            )?;
        }
        if let Some(author) = attribution {
            write!(
                visitor.writer_mut(),
                "&mdash; {}",
                crate::escape::escape_html(&author)
            )?;
        }
        write!(visitor.writer_mut(), "</footer>")?;
    }

    write!(visitor.writer_mut(), "</blockquote>")?;
    Ok(())
}

fn render_verse_paragraph<W: Write>(
    para: &Paragraph,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    let content = extract_plain_text(&para.content);
    let escaped = crate::escape::escape_html(&content);
    write!(visitor.writer_mut(), "<pre class=\"verse\">{escaped}</pre>")?;

    let attribution = para.metadata.attributes.get_string("attribution");
    let citation = para.metadata.attributes.get_string("citation");
    if attribution.is_some() || citation.is_some() {
        write!(visitor.writer_mut(), "<footer class=\"verse-footer\">")?;
        if let Some(cite) = citation {
            write!(
                visitor.writer_mut(),
                "{} ",
                crate::escape::escape_html(&cite)
            )?;
        }
        if let Some(author) = attribution {
            write!(
                visitor.writer_mut(),
                "&mdash; {}",
                crate::escape::escape_html(&author)
            )?;
        }
        write!(visitor.writer_mut(), "</footer>")?;
    }

    Ok(())
}

fn render_literal_paragraph<W: Write>(
    para: &Paragraph,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    let content = extract_plain_text(&para.content);
    let escaped = crate::escape::escape_html(&content);
    write!(visitor.writer_mut(), "<pre class=\"Li\">{escaped}</pre>")?;
    Ok(())
}
