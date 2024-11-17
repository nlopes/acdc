use std::io::Write;

use crate::Render;

impl Render for acdc_parser::Section {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        write!(w, "> ")?;
        for node in &self.title {
            node.render(w)?;
        }
        writeln!(w, " <")?;
        for (i, block) in self.content.iter().enumerate() {
            block.render(w)?;
            if i != self.content.len() - 1 {
                writeln!(w)?;
            }
        }
        Ok(())
    }
}
