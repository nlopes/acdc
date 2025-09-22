use std::io::Write;

use crate::{Processor, Render};

impl Render for acdc_parser::Section {
    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> std::io::Result<()> {
        write!(w, "> ")?;
        for node in &self.title {
            node.render(w, processor)?;
        }
        writeln!(w, " <")?;
        let last_index = self.content.len() - 1;
        for (i, block) in self.content.iter().enumerate() {
            block.render(w, processor)?;
            if i != last_index {
                writeln!(w)?;
            }
        }
        Ok(())
    }
}
