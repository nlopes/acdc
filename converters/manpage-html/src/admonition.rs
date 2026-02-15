use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::Admonition;

use crate::{Error, ManpageHtmlVisitor};

pub(crate) fn visit_admonition<W: Write>(
    admon: &Admonition,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    let label = format!("{:?}", admon.variant).to_uppercase();

    write!(
        visitor.writer_mut(),
        "<div class=\"admonition {}\">",
        label.to_lowercase()
    )?;
    write!(visitor.writer_mut(), "<p class=\"Pp\"><b>{label}:</b>")?;

    if !admon.title.is_empty() {
        write!(visitor.writer_mut(), " ")?;
        visitor.visit_inline_nodes(&admon.title)?;
    }

    write!(visitor.writer_mut(), "</p>")?;

    for block in &admon.blocks {
        visitor.visit_block(block)?;
    }

    write!(visitor.writer_mut(), "</div>")?;

    Ok(())
}
