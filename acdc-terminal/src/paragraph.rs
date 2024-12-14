use std::io::Write;

use crate::Render;

impl Render for acdc_parser::Paragraph {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        for node in &self.title {
            node.render(w)?;
        }

        let last_index = self.content.len() - 1;
        for (i, node) in self.content.iter().enumerate() {
            node.render(w)?;
            if i != last_index {
                write!(w, " ")?;
            }
        }
        Ok(())
    }
}
