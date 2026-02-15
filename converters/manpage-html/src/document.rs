use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{Author, Document};

use crate::{Error, ManpageHtmlVisitor, css, escape::extract_plain_text};

fn format_author_name(author: &Author) -> String {
    match &author.middle_name {
        Some(middle) => format!("{} {middle} {}", author.first_name, author.last_name),
        None => format!("{} {}", author.first_name, author.last_name),
    }
}

pub(crate) fn visit_document_start<W: Write>(
    doc: &Document,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    if visitor.processor.options.embedded() {
        write!(
            visitor.writer_mut(),
            "<div class=\"manpage manpage-terminal\">"
        )?;
        return Ok(());
    }

    let title = doc
        .header
        .as_ref()
        .map(|h| crate::escape::escape_html(&extract_plain_text(&h.title)))
        .unwrap_or_default();

    let w = visitor.writer_mut();
    write!(
        w,
        "<!DOCTYPE html>\
         <html lang=\"en\">\
         <head>\
         <meta charset=\"UTF-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\
         <title>{title}</title>\
         <style>{}</style>\
         <style>{}</style>\
         </head>\
         <body>\
         <div class=\"manpage manpage-terminal\">",
        css::TERMINAL_CSS,
        css::MODERN_CSS
    )?;

    Ok(())
}

pub(crate) fn visit_document_supplements<W: Write>(
    doc: &Document,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    // Footnotes
    if !doc.footnotes.is_empty() {
        let w = visitor.writer_mut();
        write!(w, "<section class=\"Sh\"><h1>NOTES</h1>")?;
        for footnote in &doc.footnotes {
            let w = visitor.writer_mut();
            write!(w, "<p class=\"Pp footnote\">[{}] ", footnote.number)?;
            visitor.visit_inline_nodes(&footnote.content)?;
            write!(visitor.writer_mut(), "</p>")?;
        }
        write!(visitor.writer_mut(), "</section>")?;
    }

    // Author(s) section
    if let Some(header) = &doc.header
        && !header.authors.is_empty()
    {
        let w = visitor.writer_mut();
        if header.authors.len() == 1 {
            write!(w, "<section class=\"Sh\"><h1>AUTHOR</h1>")?;
        } else {
            write!(w, "<section class=\"Sh\"><h1>AUTHORS</h1>")?;
        }
        for author in &header.authors {
            let w = visitor.writer_mut();
            let name = crate::escape::escape_html(&format_author_name(author));
            write!(w, "<p class=\"Pp\"><b>{name}</b>")?;
            if let Some(email) = &author.email {
                let escaped = crate::escape::escape_html(email);
                write!(w, " &lt;<a href=\"mailto:{escaped}\">{escaped}</a>&gt;")?;
            }
            write!(w, "</p>")?;
        }
        write!(visitor.writer_mut(), "</section>")?;
    }

    Ok(())
}

pub(crate) fn visit_document_end<W: Write>(
    _doc: &Document,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    if let Some(ref first) = visitor.first_section_title
        && !first.eq_ignore_ascii_case("NAME")
    {
        tracing::warn!(
            first_section = %first,
            "manpage convention: NAME should be the first section"
        );
    }
    if let Some(ref second) = visitor.second_section_title
        && !second.eq_ignore_ascii_case("SYNOPSIS")
    {
        tracing::warn!(
            second_section = %second,
            "manpage convention: SYNOPSIS should be the second section"
        );
    }

    write!(visitor.writer_mut(), "</div>")?;

    if !visitor.processor.options.embedded() {
        write!(visitor.writer_mut(), "</body></html>")?;
    }

    Ok(())
}
