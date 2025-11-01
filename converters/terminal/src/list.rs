use std::io::{self, BufWriter, Write};

use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use acdc_converters_common::visitor::{Visitor, WritableVisitor};
use acdc_parser::{
    CalloutList, DescriptionList, DescriptionListItem, InlineNode, ListItem, ListItemCheckedStatus,
    OrderedList, UnorderedList,
};

use crate::{Error, TerminalVisitor};

/// Render inline nodes with spaces between them.
///
/// This helper function renders a collection of inline nodes, inserting a space
/// between each node. It uses a peekable iterator to avoid adding a trailing space.
#[tracing::instrument(skip(visitor))]
fn render_nodes_with_spaces<V: WritableVisitor<Error = Error>>(
    nodes: &[InlineNode],
    visitor: &mut V,
) -> Result<(), Error> {
    let last_index = if nodes.is_empty() { 0 } else { nodes.len() - 1 };
    for (i, node) in nodes.iter().enumerate() {
        visitor.visit_inline_node(node)?;
        if i != last_index {
            let w = visitor.writer_mut();
            write!(w, " ")?;
        }
    }
    Ok(())
}

/// Render a title with italic styling.
///
/// This helper function renders inline nodes to a buffer, converts to a string,
/// trims whitespace, and applies italic styling for terminal output.
#[tracing::instrument(skip(visitor))]
fn render_styled_title<V: WritableVisitor<Error = Error>>(
    title: &[InlineNode],
    visitor: &mut V,
    processor: &crate::Processor,
) -> Result<(), Error> {
    if !title.is_empty() {
        let processor = processor.clone();
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor);
        for node in title {
            temp_visitor.visit_inline_node(node)?;
        }
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;
        let w = visitor.writer_mut();
        w.queue(PrintStyledContent(
            String::from_utf8_lossy(&buffer).trim().to_string().italic(),
        ))?;
    }
    Ok(())
}

/// Render nested list items with proper indentation based on their level
#[tracing::instrument(skip(visitor))]
fn render_nested_list_items<V: WritableVisitor<Error = Error>>(
    items: &[ListItem],
    visitor: &mut V,
    expected_level: u8,
    indent: usize,
    is_ordered: bool,
) -> Result<(), Error> {
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
            render_list_item_with_indent(item, visitor, indent, is_ordered, item_number)?;
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
                    visitor,
                    next_level,
                    nested_indent,
                    is_ordered,
                )?;

                i -= 1; // Adjust because we'll increment at the end of the loop
            }

            i += 1;
        } else {
            // Item at higher level than expected, treat as same level
            render_list_item_with_indent(item, visitor, indent, is_ordered, item_number)?;
            item_number += 1;
            i += 1;
        }
    }
    Ok(())
}

/// Render a single list item with the specified indentation
#[tracing::instrument(skip(visitor))]
fn render_list_item_with_indent<V: WritableVisitor<Error = Error>>(
    item: &ListItem,
    visitor: &mut V,
    indent: usize,
    is_ordered: bool,
    item_number: usize,
) -> Result<(), Error> {
    let mut w = visitor.writer_mut();
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
    let _ = w;

    // Render principal text inline
    render_nodes_with_spaces(&item.principal, visitor)?;

    w = visitor.writer_mut();
    writeln!(w)?;

    // Render attached blocks with proper indentation
    for block in &item.blocks {
        let w = visitor.writer_mut();
        // Add indentation for nested content
        write!(w, "{:indent$}", " ", indent = indent + 2)?;
        let _ = w;
        visitor.visit_block(block)?;
    }
    Ok(())
}

#[tracing::instrument(skip(w))]
fn render_checked_status<W: Write + ?Sized>(
    checked: Option<&ListItemCheckedStatus>,
    w: &mut W,
) -> Result<(), Error> {
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

pub(crate) fn visit_unordered_list<V: WritableVisitor<Error = Error>>(
    list: &UnorderedList,
    visitor: &mut V,
    processor: &crate::Processor,
) -> Result<(), Error> {
    render_styled_title(&list.title, visitor, processor)?;
    let w = visitor.writer_mut();
    writeln!(w)?;
    let _ = w;
    render_nested_list_items(&list.items, visitor, 1, 0, false)?;
    Ok(())
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
pub(crate) fn visit_ordered_list<V: WritableVisitor<Error = Error>>(
    list: &OrderedList,
    visitor: &mut V,
    processor: &crate::Processor,
) -> Result<(), Error> {
    render_styled_title(&list.title, visitor, processor)?;
    let w = visitor.writer_mut();
    writeln!(w)?;
    let _ = w;
    render_nested_list_items(&list.items, visitor, 1, 0, true)?;
    Ok(())
}

/// Renders a callout list in terminal format.
///
/// Callout lists are used to annotate code blocks with numbered references.
/// Items are formatted with angle bracket notation `<N>` where N is the item number.
///
/// # Format
/// ```text
/// <1> First explanation
/// <2> Second explanation
/// <3> Third explanation
/// ```
pub(crate) fn visit_callout_list<V: WritableVisitor<Error = Error>>(
    list: &CalloutList,
    visitor: &mut V,
    processor: &crate::Processor,
) -> Result<(), Error> {
    render_styled_title(&list.title, visitor, processor)?;
    if !list.title.is_empty() {
        let w = visitor.writer_mut();
        writeln!(w)?;
    }

    for (idx, item) in list.items.iter().enumerate() {
        let item_number = idx + 1;
        let mut w = visitor.writer_mut();
        write!(w, "<{item_number}>")?;
        write!(w, " ")?;
        let _ = w;

        // Render principal text inline
        render_nodes_with_spaces(&item.principal, visitor)?;

        w = visitor.writer_mut();
        writeln!(w)?;

        // Render attached blocks with indentation
        for block in &item.blocks {
            let w = visitor.writer_mut();
            write!(w, "  ")?;
            let _ = w;
            visitor.visit_block(block)?;
        }
    }
    Ok(())
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
pub(crate) fn visit_description_list<V: WritableVisitor<Error = Error>>(
    list: &DescriptionList,
    visitor: &mut V,
    processor: &crate::Processor,
) -> Result<(), Error> {
    render_styled_title(&list.title, visitor, processor)?;
    let w = visitor.writer_mut();
    writeln!(w)?;
    let _ = w;
    for item in &list.items {
        visit_description_list_item(item, visitor, processor)?;
    }
    Ok(())
}

/// Renders a single description list item (term and definition).
///
/// The term is rendered in bold, followed by the principal text (if present)
/// indented with 2 spaces, and any additional description blocks.
fn visit_description_list_item<V: WritableVisitor<Error = Error>>(
    item: &DescriptionListItem,
    visitor: &mut V,
    processor: &crate::Processor,
) -> Result<(), Error> {
    // Render term in bold
    let processor = processor.clone();
    let buffer = Vec::new();
    let inner = BufWriter::new(buffer);
    let mut temp_visitor = TerminalVisitor::new(inner, processor);
    render_nodes_with_spaces(&item.term, &mut temp_visitor)?;
    let buffer = temp_visitor
        .into_writer()
        .into_inner()
        .map_err(io::IntoInnerError::into_error)?;

    let mut w = visitor.writer_mut();
    w.queue(PrintStyledContent(
        String::from_utf8_lossy(&buffer).to_string().bold(),
    ))?;
    writeln!(w)?;

    // Render principal text with indentation if present
    if !item.principal_text.is_empty() {
        write!(w, "  ")?;
        let _ = w;
        render_nodes_with_spaces(&item.principal_text, visitor)?;
        w = visitor.writer_mut();
        writeln!(w)?;
    }
    let _ = w;

    // Render description blocks (without indentation as block.render handles formatting)
    for block in &item.description {
        visitor.visit_block(block)?;
    }

    Ok(())
}
