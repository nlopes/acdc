use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::Section;

use crate::{Error, ManpageHtmlVisitor, escape::extract_plain_text};

pub(crate) fn visit_section<W: Write>(
    section: &Section,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    let title_text = extract_plain_text(&section.title);

    if section.level == 1 {
        visitor.record_section_title(&title_text);
    }

    let is_name_section = title_text.eq_ignore_ascii_case("name");

    // In embedded mode, skip the NAME section (matches manpage converter behavior)
    if visitor.processor.options.embedded() && is_name_section {
        return Ok(());
    }

    let w = visitor.writer_mut();

    if section.level == 1 {
        let upper = title_text.to_uppercase();
        let escaped = crate::escape::escape_html(&upper);
        write!(w, "<section class=\"Sh\"><h1>{escaped}</h1>")?;
    } else if section.level == 2 {
        let escaped = crate::escape::escape_html(&title_text);
        write!(w, "<section class=\"Ss\"><h2>{escaped}</h2>")?;
    } else {
        write!(w, "<section class=\"Ss\"><h3>")?;
        visitor.visit_inline_nodes(&section.title)?;
        write!(visitor.writer_mut(), "</h3>")?;
    }

    if is_name_section {
        visitor.in_name_section = true;
    }

    for block in &section.content {
        visitor.visit_block(block)?;
    }

    if is_name_section {
        visitor.in_name_section = false;
    }

    write!(visitor.writer_mut(), "</section>")?;

    Ok(())
}
