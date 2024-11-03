use std::io::Write;

use crossterm::style::Stylize;

use crate::Render;

impl Render for acdc_parser::Paragraph {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        if let Some(title) = &self.title {
            write!(w, "{}", title.clone().bold().white())?;
        }
        for node in &self.content {
            node.render(w)?;
        }
        Ok(())
    }
}
