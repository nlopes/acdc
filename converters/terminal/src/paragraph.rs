use std::io::Write;

use crate::{Processor, Render};

impl Render for acdc_parser::Paragraph {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        for node in &self.title {
            node.render(w, processor)?;
        }

        let last_index = self.content.len() - 1;
        for (i, node) in self.content.iter().enumerate() {
            node.render(w, processor)?;
            if i != last_index {
                write!(w, " ")?;
            }
        }
        Ok(())
    }
}
