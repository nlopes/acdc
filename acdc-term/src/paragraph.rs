use std::io::Write;

use crate::Render;

impl Render for acdc_parser::Paragraph {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        for node in &self.title {
            node.render(w)?;
        }

        for node in &self.content {
            node.render(w)?;
        }
        Ok(())
    }
}
