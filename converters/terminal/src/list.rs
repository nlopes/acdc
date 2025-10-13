use std::io::Write;

use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use acdc_parser::{ListItem, ListItemCheckedStatus, UnorderedList};

use crate::{Processor, Render};

/// Render nested list items with proper indentation based on their level
fn render_nested_list_items<W: Write>(
    items: &[ListItem],
    w: &mut W,
    processor: &Processor,
    expected_level: u8,
    indent: usize,
) -> Result<(), crate::Error> {
    let mut i = 0;
    while i < items.len() {
        let item = &items[i];

        if item.level < expected_level {
            // Item at lower level, return to parent
            break;
        }

        if item.level == expected_level {
            // Render item at current level with appropriate indentation
            render_list_item_with_indent(item, w, processor, indent)?;

            // Check if next items are nested (higher level)
            if i + 1 < items.len() && items[i + 1].level > expected_level {
                let next_level = items[i + 1].level;
                let nested_indent = indent + 2; // Indent by 2 spaces per level

                // Find range of nested items
                i += 1;
                let nested_start = i;
                while i < items.len() && items[i].level >= next_level {
                    i += 1;
                }

                // Recursively render nested items
                render_nested_list_items(
                    &items[nested_start..i],
                    w,
                    processor,
                    next_level,
                    nested_indent,
                )?;

                i -= 1; // Adjust because we'll increment at the end of the loop
            }

            i += 1;
        } else {
            // Item at higher level than expected, treat as same level
            render_list_item_with_indent(item, w, processor, indent)?;
            i += 1;
        }
    }
    Ok(())
}

/// Render a single list item with the specified indentation
fn render_list_item_with_indent<W: Write>(
    item: &ListItem,
    w: &mut W,
    processor: &Processor,
    indent: usize,
) -> Result<(), crate::Error> {
    // Write indentation
    write!(w, "{:indent$}", "", indent = indent)?;

    // Write a normalized marker (always * regardless of level)
    write!(w, "*")?;

    if let Some(checked) = &item.checked {
        write!(w, " ")?;
        if checked == &ListItemCheckedStatus::Checked {
            w.queue(PrintStyledContent("✔".bold()))?;
        } else {
            w.queue(PrintStyledContent("✘".bold()))?;
        }
    }

    write!(w, " ")?;

    // Render each node with a space between them
    let last_index = item.content.len() - 1;
    for (i, node) in item.content.iter().enumerate() {
        node.render(w, processor)?;
        if i != last_index {
            write!(w, " ")?;
        }
    }
    writeln!(w)?;
    Ok(())
}

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
        render_nested_list_items(&self.items, w, processor, 1, 0)?;
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
