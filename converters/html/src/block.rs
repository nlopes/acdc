use std::io::Write;

use acdc_parser::Block;

use crate::{Processor, Render, RenderOptions};

impl Render for Block {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        match self {
            Block::Admonition(a) => a.render(w, processor, options),
            Block::Paragraph(p) => p.render(w, processor, options),
            Block::DelimitedBlock(d) => d.render(w, processor, options),
            Block::Section(s) => s.render(w, processor, options),
            Block::UnorderedList(u) => u.render(w, processor, options),
            Block::OrderedList(o) => o.render(w, processor, options),
            Block::CalloutList(c) => c.render(w, processor, options),
            Block::DescriptionList(d) => d.render(w, processor, options),
            Block::DocumentAttribute(_) => Ok(()),
            Block::TableOfContents(t) => t.render(w, processor, options),
            Block::Image(i) => i.render(w, processor, options),
            Block::Audio(a) => a.render(w, processor, options),
            Block::Video(v) => v.render(w, processor, options),
            Block::DiscreteHeader(d) => d.render(w, processor, options),
            Block::ThematicBreak(t) => {
                if !t.title.is_empty() {
                    write!(w, "<div class=\"title\">")?;
                    crate::inlines::render_inlines(&t.title, w, processor, options)?;
                    writeln!(w, "</div>")?;
                }
                writeln!(w, "<hr>")?;
                Ok(())
            }
            Block::PageBreak(_) => {
                writeln!(w, "<div style=\"page-break-after: always;\"></div>")?;
                Ok(())
            }
            unknown => todo!("rendering for block type: {:?}", unknown),
        }
    }
}
