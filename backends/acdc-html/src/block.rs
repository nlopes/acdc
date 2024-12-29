use std::io::Write;

use acdc_parser::Block;

use crate::{Processor, Render, RenderOptions};

impl Render for Block {
    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> std::io::Result<()> {
        match self {
            Block::Paragraph(p) => p.render(w, processor, options),
            Block::DelimitedBlock(d) => d.render(w, processor, options),
            Block::Section(s) => s.render(w, processor, options),
            Block::UnorderedList(u) => u.render(w, processor, options),
            unknown => todo!("rendering for block type: {:?}", unknown),
        }
    }
}
