use std::io::Write;

use acdc_parser::{DelimitedBlock, DelimitedBlockType};

use crate::{Processor, Render, RenderOptions};

impl Render for DelimitedBlock {
    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> std::io::Result<()> {
        writeln!(w, "<div>")?;
        let _ = match &self.inner {
            DelimitedBlockType::DelimitedTable(t) => t.render(w, processor, options),
            unknown => todo!("Unknown delimited block type: {:?}", unknown),
        };
        writeln!(w, "</div>")?;
        Ok(())
    }
}
