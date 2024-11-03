use std::io::Write;

use crate::Render;

impl Render for acdc_parser::DelimitedBlock {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        writeln!(w)?;
        match &self.inner {
            acdc_parser::DelimitedBlockType::DelimitedTable(t) => t.render(w),
            _ => Ok(()),
        }
    }
}
