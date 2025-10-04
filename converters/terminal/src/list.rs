use std::io::Write;

use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use acdc_parser::{ListItem, ListItemCheckedStatus, UnorderedList};

use crate::{Processor, Render};

impl Render for UnorderedList {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        if !self.title.is_empty() {
            let mut inner = std::io::BufWriter::new(Vec::new());
            self.title
                .iter()
                .try_for_each(|node| node.render(&mut inner, processor))?;
            inner.flush()?;
            w.queue(PrintStyledContent(
                String::from_utf8(inner.get_ref().clone())
                    .unwrap_or_default()
                    .trim()
                    .italic(),
            ))?;
        }
        writeln!(w)?;
        self.items
            .iter()
            .try_for_each(|item| item.render(w, processor))?;
        Ok(())
    }
}

impl Render for ListItem {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        write!(w, "{}", self.marker)?;
        if let Some(checked) = &self.checked {
            write!(w, " ")?;
            if checked == &ListItemCheckedStatus::Checked {
                w.queue(PrintStyledContent("✔".bold()))?;
            } else {
                w.queue(PrintStyledContent("✘".bold()))?;
            }
        }
        write!(w, " ")?;
        // render each node with a space between them
        let last_index = self.content.len() - 1;
        for (i, node) in self.content.iter().enumerate() {
            node.render(w, processor)?;
            if i != last_index {
                write!(w, " ")?;
            }
        }
        writeln!(w)?;
        Ok(())
    }
}
