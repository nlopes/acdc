use std::io::Write;

use crate::{Processor, Render};

impl Render for acdc_parser::DelimitedBlock {
    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> std::io::Result<()> {
        writeln!(w)?;
        match &self.inner {
            acdc_parser::DelimitedBlockType::DelimitedTable(t) => t.render(w, processor),
            _ => Ok(()),
        }
    }
}
