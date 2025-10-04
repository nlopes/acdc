use std::io::Write;

use comfy_table::{Cell, Color, ContentArrangement, Table};

use crate::{Processor, Render};

impl Render for acdc_parser::Table {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        let mut table = Table::new();
        table
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_width(80)
            .load_preset(comfy_table::presets::UTF8_FULL)
            .apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS);

        if let Some(header) = &self.header {
            let header_cells = header
                .columns
                .iter()
                .map(|col| {
                    let mut inner = std::io::BufWriter::new(Vec::new());
                    col.content
                        .iter()
                        .try_for_each(|block| block.render(&mut inner, processor))?;
                    inner.flush()?;
                    Ok(
                        Cell::new(String::from_utf8(inner.get_ref().clone()).unwrap_or_default())
                            .fg(Color::Green)
                            .add_attribute(comfy_table::Attribute::Bold),
                    )
                })
                .collect::<Result<Vec<_>, Self::Error>>()?;
            table.set_header(header_cells);
        }

        for row in &self.rows {
            let cells = row
                .columns
                .iter()
                .map(|col| {
                    let mut inner = std::io::BufWriter::new(Vec::new());
                    col.content
                        .iter()
                        .try_for_each(|block| block.render(&mut inner, processor))?;
                    inner.flush()?;
                    Ok(Cell::new(
                        String::from_utf8(inner.get_ref().clone()).unwrap_or_default(),
                    ))
                })
                .collect::<Result<Vec<_>, Self::Error>>()?;
            table.add_row(cells);
        }

        writeln!(w, "{table}")?;
        Ok(())
    }
}
