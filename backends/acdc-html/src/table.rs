use std::io::Write;

use acdc_parser::Table;

use crate::{Processor, Render, RenderOptions};

impl Render for Table {
    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> std::io::Result<()> {
        writeln!(w, "<table>")?;
        if let Some(header) = &self.header {
            writeln!(w, "<thead>")?;
            writeln!(w, "<tr>")?;
            for cell in &header.columns {
                write!(w, "<th>")?;
                for block in &cell.content {
                    block.render(w, processor, options)?;
                }
                writeln!(w, "</th>")?;
            }
            writeln!(w, "</tr>")?;
            writeln!(w, "</thead>")?;
        }
        writeln!(w, "<tbody>")?;
        for row in &self.rows {
            writeln!(w, "<tr>")?;
            for cell in &row.columns {
                write!(w, "<td>")?;
                for block in &cell.content {
                    block.render(w, processor, options)?;
                }
                writeln!(w, "</td>")?;
            }
            writeln!(w, "</tr>")?;
        }
        writeln!(w, "</tbody>")?;
        writeln!(w, "</table>")?;
        Ok(())
    }
}
