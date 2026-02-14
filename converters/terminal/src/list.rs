use std::io::{self, BufWriter, Write};

use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{
    CalloutList, DescriptionList, DescriptionListItem, InlineNode, ListItem, ListItemCheckedStatus,
    OrderedList, UnorderedList,
};

use crate::{Error, TerminalVisitor};

/// Render a sequence of inline nodes.
///
/// Nodes are rendered directly without inserting additional spaces, since
/// the parser already includes whitespace within `PlainText` nodes.
#[tracing::instrument(skip(visitor))]
fn render_inline_nodes<V: WritableVisitor<Error = Error>>(
    nodes: &[InlineNode],
    visitor: &mut V,
) -> Result<(), Error> {
    for node in nodes {
        visitor.visit_inline_node(node)?;
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

/// Write `indent` spaces to the writer. Does nothing when `indent` is 0.
fn write_indent(w: &mut dyn Write, indent: usize) -> Result<(), Error> {
    if indent > 0 {
        write!(w, "{:indent$}", "", indent = indent)?;
    }
    Ok(())
}

/// Render list items with proper indentation.
///
/// All items in a single list are at the same nesting level. Nested lists appear
/// as `Block` children within individual items and are handled by the visitor
/// pattern (which reads `processor.list_indent` for the correct indentation).
#[tracing::instrument(skip(visitor, processor))]
fn render_list_items<V: WritableVisitor<Error = Error>>(
    items: &[ListItem],
    visitor: &mut V,
    indent: usize,
    is_ordered: bool,
    unicode: bool,
    processor: &crate::Processor,
) -> Result<(), Error> {
    for (idx, item) in items.iter().enumerate() {
        render_list_item(
            item,
            visitor,
            indent,
            is_ordered,
            idx + 1,
            unicode,
            processor,
        )?;
    }
    Ok(())
}

/// Render a single list item with the specified indentation.
///
/// After rendering the item's principal text, child blocks (which may include
/// nested lists) are visited with an increased `list_indent` on the processor.
#[tracing::instrument(skip(visitor, processor))]
fn render_list_item<V: WritableVisitor<Error = Error>>(
    item: &ListItem,
    visitor: &mut V,
    indent: usize,
    is_ordered: bool,
    item_number: usize,
    unicode: bool,
    processor: &crate::Processor,
) -> Result<(), Error> {
    let w = visitor.writer_mut();
    write_indent(w, indent)?;

    if is_ordered {
        write!(w, "{item_number}.")?;
    } else {
        write!(w, "*")?;
    }

    render_checked_status(item.checked.as_ref(), w, unicode)?;
    write!(w, " ")?;
    let _ = w;

    render_inline_nodes(&item.principal, visitor)?;

    let w = visitor.writer_mut();
    writeln!(w)?;
    let _ = w;

    // Render attached blocks with increased indentation.
    // Set list_indent so nested lists rendered via the visitor pick up the right depth.
    let nested_indent = indent + 2;
    let old_indent = processor.list_indent.get();
    processor.list_indent.set(nested_indent);
    for block in &item.blocks {
        visitor.visit_block(block)?;
    }
    processor.list_indent.set(old_indent);

    Ok(())
}

#[tracing::instrument(skip(w))]
fn render_checked_status<W: Write + ?Sized>(
    checked: Option<&ListItemCheckedStatus>,
    w: &mut W,
    unicode: bool,
) -> Result<(), Error> {
    if let Some(checked) = checked {
        write!(w, " ")?;
        if checked == &ListItemCheckedStatus::Checked {
            if unicode {
                w.queue(PrintStyledContent("[✔]".bold()))?;
            } else {
                w.queue(PrintStyledContent("[x]".bold()))?;
            }
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
    let indent = processor.list_indent.get();
    render_styled_title(&list.title, visitor, processor)?;
    // Only emit a leading newline for top-level lists (not nested ones)
    if indent == 0 {
        let w = visitor.writer_mut();
        writeln!(w)?;
    }
    let unicode = processor.appearance.capabilities.unicode;
    render_list_items(&list.items, visitor, indent, false, unicode, processor)?;
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
    let indent = processor.list_indent.get();
    render_styled_title(&list.title, visitor, processor)?;
    // Only emit a leading newline for top-level lists (not nested ones)
    if indent == 0 {
        let w = visitor.writer_mut();
        writeln!(w)?;
    }
    let unicode = processor.appearance.capabilities.unicode;
    render_list_items(&list.items, visitor, indent, true, unicode, processor)?;
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
        render_inline_nodes(&item.principal, visitor)?;

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
/// Supports three styles:
/// - **default**: Terms in bold on one line, definitions indented on next line
/// - **horizontal**: Terms and definitions on same line separated by `::`
/// - **qanda**: Terms prefixed with "Q: ", definitions with "A: "
pub(crate) fn visit_description_list<V: WritableVisitor<Error = Error>>(
    list: &DescriptionList,
    visitor: &mut V,
    processor: &crate::Processor,
) -> Result<(), Error> {
    render_styled_title(&list.title, visitor, processor)?;
    let w = visitor.writer_mut();
    writeln!(w)?;
    let _ = w;

    let style = list.metadata.style.as_deref();

    for item in &list.items {
        match style {
            Some("horizontal") => {
                visit_horizontal_description_list_item(item, visitor, processor)?;
            }
            Some("qanda") => {
                visit_qanda_description_list_item(item, visitor, processor)?;
            }
            _ => {
                visit_description_list_item(item, visitor, processor)?;
            }
        }
    }
    Ok(())
}

/// Renders a single description list item (term and definition) in default style.
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
    render_inline_nodes(&item.term, &mut temp_visitor)?;
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
        render_inline_nodes(&item.principal_text, visitor)?;
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

/// Renders a single description list item in horizontal style.
///
/// Term and definition are on the same line, separated by `::`.
fn visit_horizontal_description_list_item<V: WritableVisitor<Error = Error>>(
    item: &DescriptionListItem,
    visitor: &mut V,
    processor: &crate::Processor,
) -> Result<(), Error> {
    // Render term in bold
    let processor = processor.clone();
    let buffer = Vec::new();
    let inner = BufWriter::new(buffer);
    let mut temp_visitor = TerminalVisitor::new(inner, processor);
    render_inline_nodes(&item.term, &mut temp_visitor)?;
    let buffer = temp_visitor
        .into_writer()
        .into_inner()
        .map_err(io::IntoInnerError::into_error)?;

    let mut w = visitor.writer_mut();
    w.queue(PrintStyledContent(
        String::from_utf8_lossy(&buffer).to_string().bold(),
    ))?;

    // Same line: term :: definition
    if !item.principal_text.is_empty() {
        write!(w, " :: ")?;
        let _ = w;
        render_inline_nodes(&item.principal_text, visitor)?;
        w = visitor.writer_mut();
    }
    writeln!(w)?;
    let _ = w;

    // Render description blocks indented
    for block in &item.description {
        let w = visitor.writer_mut();
        write!(w, "  ")?;
        let _ = w;
        visitor.visit_block(block)?;
    }

    Ok(())
}

/// Renders a single description list item in Q&A style.
///
/// Terms are prefixed with "Q: " and definitions with "A: ".
fn visit_qanda_description_list_item<V: WritableVisitor<Error = Error>>(
    item: &DescriptionListItem,
    visitor: &mut V,
    processor: &crate::Processor,
) -> Result<(), Error> {
    // Render "Q: " prefix + term in bold
    let processor = processor.clone();
    let buffer = Vec::new();
    let inner = BufWriter::new(buffer);
    let mut temp_visitor = TerminalVisitor::new(inner, processor);
    render_inline_nodes(&item.term, &mut temp_visitor)?;
    let buffer = temp_visitor
        .into_writer()
        .into_inner()
        .map_err(io::IntoInnerError::into_error)?;

    let mut w = visitor.writer_mut();
    w.queue(PrintStyledContent("Q: ".bold()))?;
    w.queue(PrintStyledContent(
        String::from_utf8_lossy(&buffer).to_string().bold(),
    ))?;
    writeln!(w)?;

    // Render "A: " prefix + principal text
    if !item.principal_text.is_empty() {
        w.queue(PrintStyledContent("A: ".dim()))?;
        let _ = w;
        render_inline_nodes(&item.principal_text, visitor)?;
        w = visitor.writer_mut();
        writeln!(w)?;
    }
    let _ = w;

    // Render description blocks indented
    for block in &item.description {
        let w = visitor.writer_mut();
        write!(w, "   ")?;
        let _ = w;
        visitor.visit_block(block)?;
    }

    Ok(())
}
