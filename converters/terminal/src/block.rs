use std::io::Write;

use crate::{Processor, Render};

impl Render for acdc_parser::Block {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        match self {
            acdc_parser::Block::Paragraph(p) => p.render(w, processor),
            acdc_parser::Block::DelimitedBlock(d) => d.render(w, processor),
            acdc_parser::Block::Section(s) => s.render(w, processor),
            acdc_parser::Block::UnorderedList(u) => u.render(w, processor),
            acdc_parser::Block::Image(i) => i.render(w, processor),
            acdc_parser::Block::ThematicBreak(t) => t.render(w, processor),
            acdc_parser::Block::PageBreak(p) => p.render(w, processor),
            _ => {
                tracing::warn!("Unexpected block: {:?}", self);
                Ok(())
            }
        }?;
        writeln!(w)?;
        Ok(())
    }
}
