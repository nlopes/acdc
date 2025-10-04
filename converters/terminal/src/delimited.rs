use std::io::Write;

use crate::{Processor, Render};

impl Render for acdc_parser::DelimitedBlock {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        writeln!(w)?;
        match &self.inner {
            acdc_parser::DelimitedBlockType::DelimitedTable(t) => t.render(w, processor),
            _ => Ok(()),
        }
    }
}
