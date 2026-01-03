//! Admonition rendering for manpages.
//!
//! Admonitions (NOTE, TIP, WARNING, etc.) are rendered with a bold label
//! followed by indented content.

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::Admonition;

use crate::{Error, ManpageVisitor};

/// Visit an admonition block.
pub fn visit_admonition<W: Write>(
    admon: &Admonition,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    // Spacing before admonition
    writeln!(w, ".sp")?;

    // Label (bold, uppercase)
    let label = format!("{:?}", admon.variant).to_uppercase();
    write!(w, "\\fB{label}:\\fP")?;

    // Optional title
    if !admon.title.is_empty() {
        write!(w, " ")?;
        visitor.visit_inline_nodes(&admon.title)?;
    }

    let w = visitor.writer_mut();
    writeln!(w)?;

    // Indented content
    writeln!(w, ".RS 4")?;

    for block in &admon.blocks {
        visitor.visit_block(block)?;
    }

    let w = visitor.writer_mut();
    writeln!(w, ".RE")?;

    Ok(())
}
