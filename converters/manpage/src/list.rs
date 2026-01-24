//! List rendering for manpages.
//!
//! Handles unordered, ordered, description, and callout lists using
//! `.IP`, `.TP`, `.RS`, and `.RE` macros.

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor, WritableVisitorExt};
use acdc_parser::{CalloutList, DescriptionList, OrderedList, UnorderedList};

use crate::{Error, ManpageVisitor};

/// Visit an unordered (bulleted) list.
pub(crate) fn visit_unordered_list<W: Write>(
    list: &UnorderedList,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    // Optional list title
    visitor.render_title_with_wrapper(&list.title, ".PP\n\\fB", "\\fP\n")?;

    // Increase nesting depth
    if visitor.list_depth > 0 {
        let w = visitor.writer_mut();
        writeln!(w, ".RS 4")?;
    }
    visitor.list_depth += 1;

    for item in &list.items {
        let w = visitor.writer_mut();
        // Bullet point with 2-character indent
        writeln!(w, ".IP \\(bu 2")?;

        // Visit principal text (inline content after marker)
        if !item.principal.is_empty() {
            visitor.visit_inline_nodes(&item.principal)?;
            let w = visitor.writer_mut();
            writeln!(w)?;
        }

        // Visit attached blocks
        for block in &item.blocks {
            visitor.visit_block(block)?;
        }
    }

    // Restore nesting depth
    visitor.list_depth -= 1;
    if visitor.list_depth > 0 {
        let w = visitor.writer_mut();
        writeln!(w, ".RE")?;
    }

    Ok(())
}

/// Visit an ordered (numbered) list.
pub(crate) fn visit_ordered_list<W: Write>(
    list: &OrderedList,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    // Optional list title
    visitor.render_title_with_wrapper(&list.title, ".PP\n\\fB", "\\fP\n")?;

    // Increase nesting depth
    if visitor.list_depth > 0 {
        let w = visitor.writer_mut();
        writeln!(w, ".RS 4")?;
    }
    visitor.list_depth += 1;

    for (i, item) in list.items.iter().enumerate() {
        let w = visitor.writer_mut();
        // Numbered item with 4-character indent
        writeln!(w, ".IP {}. 4", i + 1)?;

        // Visit principal text
        if !item.principal.is_empty() {
            visitor.visit_inline_nodes(&item.principal)?;
            let w = visitor.writer_mut();
            writeln!(w)?;
        }

        // Visit attached blocks
        for block in &item.blocks {
            visitor.visit_block(block)?;
        }
    }

    // Restore nesting depth
    visitor.list_depth -= 1;
    if visitor.list_depth > 0 {
        let w = visitor.writer_mut();
        writeln!(w, ".RE")?;
    }

    Ok(())
}

/// Visit a description list (term/definition pairs).
pub(crate) fn visit_description_list<W: Write>(
    list: &DescriptionList,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    // Optional list title
    visitor.render_title_with_wrapper(&list.title, ".PP\n\\fB", "\\fP\n")?;

    for item in &list.items {
        let w = visitor.writer_mut();
        // Tagged paragraph
        writeln!(w, ".TP")?;

        // Term (bold)
        write!(w, "\\fB")?;
        visitor.visit_inline_nodes(&item.term)?;
        let w = visitor.writer_mut();
        writeln!(w, "\\fP")?;

        // Definition
        for block in &item.description {
            visitor.visit_block(block)?;
        }
    }

    Ok(())
}

/// Visit a callout list.
pub(crate) fn visit_callout_list<W: Write>(
    list: &CalloutList,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    // Optional list title
    visitor.render_title_with_wrapper(&list.title, ".PP\n\\fB", "\\fP\n")?;

    for (i, item) in list.items.iter().enumerate() {
        let w = visitor.writer_mut();
        // Callout number in bold (use index since ListItem doesn't have ordinal)
        writeln!(w, ".IP \\fB({})\\fP 4", i + 1)?;

        // Visit principal text
        if !item.principal.is_empty() {
            visitor.visit_inline_nodes(&item.principal)?;
            let w = visitor.writer_mut();
            writeln!(w)?;
        }

        // Visit attached blocks
        for block in &item.blocks {
            visitor.visit_block(block)?;
        }
    }

    Ok(())
}
