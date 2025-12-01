//! Paragraph rendering for manpages.
//!
//! Handles `.PP` paragraph macro and paragraph titles.

use std::io::Write;

use acdc_converters_common::visitor::{Visitor, WritableVisitor};
use acdc_parser::Paragraph;

use crate::{Error, ManpageVisitor};

/// Visit a paragraph.
pub fn visit_paragraph<W: Write>(
    para: &Paragraph,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    // Paragraph break
    writeln!(w, ".PP")?;

    // Optional title (rendered as bold)
    if !para.title.is_empty() {
        write!(w, "\\fB")?;
        visitor.visit_inline_nodes(&para.title)?;
        let w = visitor.writer_mut();
        writeln!(w, "\\fP")?;
        writeln!(w, ".br")?;
    }

    // Paragraph content
    visitor.visit_inline_nodes(&para.content)?;

    let w = visitor.writer_mut();
    writeln!(w)?;

    Ok(())
}
