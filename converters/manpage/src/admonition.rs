//! Admonition rendering for manpages.
//!
//! Admonitions (NOTE, TIP, WARNING, etc.) are rendered with a bold label
//! followed by indented content.

use std::io::Write;

use acdc_converters_core::{
    TraversalContext,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::Admonition;

use crate::{Error, ManpageVisitor};

impl<'a, W: Write> ManpageVisitor<'a, '_, W> {
    /// Visit an admonition block.
    pub(crate) fn render_admonition(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        admon: &'a Admonition<'a>,
    ) -> Result<(), Error> {
        let w = self.writer_mut();

        // Spacing before admonition
        writeln!(w, ".sp")?;

        // Label (bold, uppercase)
        let label = format!("{:?}", admon.variant).to_uppercase();
        write!(w, "\\fB{label}:\\fP")?;

        // Optional title
        if !admon.title.is_empty() {
            write!(w, " ")?;
            self.visit_inline_nodes(traversal, &admon.title)?;
        }

        let w = self.writer_mut();
        writeln!(w)?;

        // Indented content
        writeln!(w, ".RS 4")?;

        for block in &admon.blocks {
            traversal.visit_block(self, block)?;
        }

        let w = self.writer_mut();
        writeln!(w, ".RE")?;

        Ok(())
    }
}
