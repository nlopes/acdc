use std::io::Write;

use crate::Render;

impl Render for acdc_parser::Block {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        match self {
            acdc_parser::Block::Paragraph(p) => p.render(w),
            acdc_parser::Block::DelimitedBlock(d) => d.render(w),
            acdc_parser::Block::Section(s) => s.render(w),
            acdc_parser::Block::UnorderedList(u) => u.render(w),
            acdc_parser::Block::Image(i) => i.render(w),
            _ => {
                tracing::warn!("Unexpected block: {:?}", self);
                Ok(())
            }
        }?;
        writeln!(w)?;
        Ok(())
    }
}
