use std::io::Write;

use acdc_parser::Block;

use crate::{Processor, Render};

impl Render for Block {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        match self {
            Block::Paragraph(p) => p.render(w, processor),
            Block::DelimitedBlock(d) => d.render(w, processor),
            Block::Section(s) => s.render(w, processor),
            Block::UnorderedList(u) => u.render(w, processor),
            Block::Image(i) => i.render(w, processor),
            Block::Audio(a) => a.render(w, processor),
            Block::Video(v) => v.render(w, processor),
            Block::DiscreteHeader(d) => d.render(w, processor),
            Block::ThematicBreak(t) => t.render(w, processor),
            Block::PageBreak(p) => p.render(w, processor),
            _ => {
                tracing::warn!("Unexpected block: {:?}", self);
                Ok(())
            }
        }?;
        writeln!(w)?;
        Ok(())
    }
}
