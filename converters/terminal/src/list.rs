use std::io::Write;

use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::Render;

/*
   pub title: Vec<InlineNode>,
   pub metadata: BlockMetadata,
   pub items: Vec<ListItem>,
   pub marker: String,
   pub location: Location,
*/

impl Render for acdc_parser::UnorderedList {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        if !self.title.is_empty() {
            let mut inner = std::io::BufWriter::new(Vec::new());
            self.title
                .iter()
                .try_for_each(|node| node.render(&mut inner))?;
            inner.flush()?;
            w.queue(PrintStyledContent(
                String::from_utf8(inner.get_ref().clone())
                    .unwrap_or_default()
                    .trim()
                    .italic(),
            ))?;
        }
        writeln!(w)?;
        self.items.iter().try_for_each(|item| item.render(w))?;
        Ok(())
    }
}

impl Render for acdc_parser::ListItem {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        write!(w, "{}", self.marker)?;
        if let Some(checked) = self.checked {
            write!(w, " ")?;
            if checked {
                w.queue(PrintStyledContent("✔".bold()))?;
            } else {
                w.queue(PrintStyledContent("✘".bold()))?;
            }
        }
        write!(w, " ")?;
        // render each node with a space between them
        let last_index = self.content.len() - 1;
        for (i, node) in self.content.iter().enumerate() {
            node.render(w)?;
            if i != last_index {
                write!(w, " ")?;
            }
        }
        writeln!(w)
    }
}
