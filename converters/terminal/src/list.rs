use std::io::Write;

use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use acdc_parser::{
    DescriptionList, DescriptionListItem, ListItem, ListItemCheckedStatus, OrderedList,
    UnorderedList,
};

use crate::{Processor, Render};

/// Render inline nodes with spaces between them.
///
/// This helper function renders a collection of inline nodes, inserting a space
/// between each node. It uses a peekable iterator to avoid adding a trailing space.
#[tracing::instrument(skip(w, processor))]
fn render_nodes_with_spaces<W: Write, N>(
    nodes: &[N],
    w: &mut W,
    processor: &Processor,
) -> Result<(), crate::Error>
where
    N: Render<Error = crate::Error> + std::fmt::Debug,
{
    let mut iter = nodes.iter().peekable();
    while let Some(node) = iter.next() {
        node.render(w, processor)?;
        if iter.peek().is_some() {
            write!(w, " ")?;
        }
    }
    Ok(())
}

/// Render a title with italic styling.
///
/// This helper function renders inline nodes to a buffer, converts to a string,
/// trims whitespace, and applies italic styling for terminal output.
#[tracing::instrument(skip(w, processor))]
fn render_styled_title<W: Write, N>(
    title: &[N],
    w: &mut W,
    processor: &Processor,
) -> Result<(), crate::Error>
where
    N: Render<Error = crate::Error> + std::fmt::Debug,
{
    if !title.is_empty() {
        let mut inner = std::io::BufWriter::new(Vec::new());
        title
            .iter()
            .try_for_each(|node| node.render(&mut inner, processor))?;
        inner.flush()?;
        let bytes = inner
            .into_inner()
            .map_err(|e| std::io::Error::other(format!("Buffer error: {e}")))?;
        w.queue(PrintStyledContent(
            String::from_utf8_lossy(&bytes).trim().to_string().italic(),
        ))?;
    }
    Ok(())
}

/// Render nested list items with proper indentation based on their level
#[tracing::instrument(skip(w, processor))]
fn render_nested_list_items<W: Write>(
    items: &[ListItem],
    w: &mut W,
    processor: &Processor,
    expected_level: u8,
    indent: usize,
    is_ordered: bool,
) -> Result<(), crate::Error> {
    let mut i = 0;
    let mut item_number = 1;

    while i < items.len() {
        let item = &items[i];

        if item.level < expected_level {
            // Item at lower level, return to parent
            break;
        }

        if item.level == expected_level {
            // Render item at current level with appropriate indentation
            render_list_item_with_indent(item, w, processor, indent, is_ordered, item_number)?;
            item_number += 1;

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
                    is_ordered,
                )?;

                i -= 1; // Adjust because we'll increment at the end of the loop
            }

            i += 1;
        } else {
            // Item at higher level than expected, treat as same level
            render_list_item_with_indent(item, w, processor, indent, is_ordered, item_number)?;
            item_number += 1;
            i += 1;
        }
    }
    Ok(())
}

/// Render a single list item with the specified indentation
#[tracing::instrument(skip(w, processor))]
fn render_list_item_with_indent<W: Write>(
    item: &ListItem,
    w: &mut W,
    processor: &Processor,
    indent: usize,
    is_ordered: bool,
    item_number: usize,
) -> Result<(), crate::Error> {
    // Write indentation
    write!(w, "{:indent$}", " ", indent = indent)?;

    // Write marker based on list type
    if is_ordered {
        write!(w, "{item_number}.")?;
    } else {
        write!(w, "*")?;
    }

    render_checked_status(item.checked.as_ref(), w)?;
    write!(w, " ")?;

    // Render principal text inline
    render_nodes_with_spaces(&item.principal, w, processor)?;
    writeln!(w)?;

    // Render attached blocks with proper indentation
    for block in &item.blocks {
        // Add indentation for nested content
        write!(w, "{:indent$}", " ", indent = indent + 2)?;
        block.render(w, processor)?;
    }
    Ok(())
}

#[tracing::instrument(skip(w))]
fn render_checked_status<W: Write>(
    checked: Option<&ListItemCheckedStatus>,
    w: &mut W,
) -> Result<(), crate::Error> {
    if let Some(checked) = checked {
        write!(w, " ")?;
        if checked == &ListItemCheckedStatus::Checked {
            w.queue(PrintStyledContent("[✔]".bold()))?;
        } else {
            w.queue(PrintStyledContent("[ ]".bold()))?;
        }
    }
    Ok(())
}

impl Render for UnorderedList {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        render_styled_title(&self.title, w, processor)?;
        writeln!(w)?;
        render_nested_list_items(&self.items, w, processor, 1, 0, false)?;
        Ok(())
    }
}

/// Renders an ordered list in terminal format.
///
/// Items are numbered starting from 1 with format "N. " where N is the item number.
/// Nested lists restart numbering from 1 at each level.
///
/// # Format
/// ```text
/// 1. First item
/// 2. Second item
///    1. Nested item
/// 3. Third item
/// ```
impl Render for OrderedList {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        render_styled_title(&self.title, w, processor)?;
        writeln!(w)?;
        render_nested_list_items(&self.items, w, processor, 1, 0, true)?;
        Ok(())
    }
}

impl Render for ListItem {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        write!(w, "{}", self.marker)?;
        render_checked_status(self.checked.as_ref(), w)?;
        write!(w, " ")?;
        // Render principal text inline
        render_nodes_with_spaces(&self.principal, w, processor)?;
        writeln!(w)?;
        // Render attached blocks
        for block in &self.blocks {
            block.render(w, processor)?;
        }
        Ok(())
    }
}

/// Renders a description list in terminal format.
///
/// Description lists consist of terms and their definitions, formatted with
/// terms in bold and definitions indented.
///
/// # Format
/// ```text
/// Term 1
///   Definition for term 1
/// Term 2
///   Definition for term 2
/// ```
impl Render for DescriptionList {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        render_styled_title(&self.title, w, processor)?;
        writeln!(w)?;
        for item in &self.items {
            item.render(w, processor)?;
        }
        Ok(())
    }
}

/// Renders a single description list item (term and definition).
///
/// The term is rendered in bold, followed by the principal text (if present)
/// indented with 2 spaces, and any additional description blocks.
impl Render for DescriptionListItem {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        // Render term in bold
        let mut term_buffer = std::io::BufWriter::new(Vec::new());
        render_nodes_with_spaces(&self.term, &mut term_buffer, processor)?;
        term_buffer.flush()?;
        let bytes = term_buffer
            .into_inner()
            .map_err(|e| std::io::Error::other(format!("Buffer error: {e}")))?;
        w.queue(PrintStyledContent(
            String::from_utf8_lossy(&bytes).to_string().bold(),
        ))?;
        writeln!(w)?;

        // Render principal text with indentation if present
        if !self.principal_text.is_empty() {
            write!(w, "  ")?;
            render_nodes_with_spaces(&self.principal_text, w, processor)?;
            writeln!(w)?;
        }

        // Render description blocks (without indentation as block.render handles formatting)
        for block in &self.description {
            block.render(w, processor)?;
        }

        Ok(())
    }
}
