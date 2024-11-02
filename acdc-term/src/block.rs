use std::io::Write;

use crate::Render;

impl Render for acdc_parser::Block {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        writeln!(w)?;
        match self {
            acdc_parser::Block::Paragraph(p) => p.render(w),
            acdc_parser::Block::Section(s) => s.render(w),
            _ => Ok(()),
        }
    }
}
