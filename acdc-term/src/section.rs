use std::io::Write;

use crossterm::style::Stylize;

use crate::Render;

impl Render for acdc_parser::Section {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        writeln!(w, "> {} <", self.title.clone().bold().white())?;
        for (i, block) in self.content.iter().enumerate() {
            block.render(w)?;
            if i != self.content.len() - 1 {
                writeln!(w)?;
            }
        }
        Ok(())
    }
}
