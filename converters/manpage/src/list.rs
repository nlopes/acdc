//! List rendering for manpages.
//!
//! Handles unordered, ordered, description, and callout lists using
//! `.IP`, `.TP`, `.RS`, and `.RE` macros.

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor, WritableVisitorExt};
use acdc_parser::{
    CalloutList, DescriptionList, ListItemCheckedStatus, OrderedList, UnorderedList,
};

use crate::{Error, ManpageVisitor};

/// Visit an unordered (bulleted) list.
pub(crate) fn visit_unordered_list<W: Write>(
    list: &UnorderedList,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    // Optional list title
    visitor.render_title_with_wrapper(&list.title, ".sp\n\\fB", "\\fP\n")?;

    // Wrap in RS/RE to scope .IP indentation — RS 4 for nested, RS 0 for
    // top-level (so .RE properly resets the indent after the list ends).
    let rs_indent = if visitor.list_depth > 0 { 4 } else { 0 };
    writeln!(visitor.writer_mut(), ".RS {rs_indent}")?;
    visitor.list_depth += 1;

    for item in &list.items {
        let w = visitor.writer_mut();
        // Bullet point with 2-character indent
        writeln!(w, ".IP \\(bu 2")?;

        // Render checklist marker if applicable
        if let Some(checked) = &item.checked {
            let w = visitor.writer_mut();
            match checked {
                ListItemCheckedStatus::Checked => write!(w, "\\(bu ")?,
                ListItemCheckedStatus::Unchecked => write!(w, "  ")?,
                _ => {}
            }
        }

        // Visit principal text (inline content after marker)
        if !item.principal.is_empty() {
            visitor.visit_inline_nodes(&item.principal)?;
            let w = visitor.writer_mut();
            writeln!(w)?;
        }

        // Visit attached blocks (list continuation content).
        // Wrap in RS 2/RE so continuation text aligns with the item's text
        // position, not the bullet position. After `.IP \(bu 2`, `.RS 0`
        // would save the bullet margin; `.RS 2` advances past the bullet
        // indent to match the text column.
        if !item.blocks.is_empty() {
            writeln!(visitor.writer_mut(), ".RS 2")?;
            for block in &item.blocks {
                visitor.visit_block(block)?;
            }
            writeln!(visitor.writer_mut(), ".RE")?;
        }
    }

    // Close the RS scope and restore nesting depth
    visitor.list_depth -= 1;
    let w = visitor.writer_mut();
    writeln!(w, ".RE")?;

    Ok(())
}

/// Visit an ordered (numbered) list.
pub(crate) fn visit_ordered_list<W: Write>(
    list: &OrderedList,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    // Optional list title
    visitor.render_title_with_wrapper(&list.title, ".sp\n\\fB", "\\fP\n")?;

    // Wrap in RS/RE to scope .IP indentation
    let rs_indent = if visitor.list_depth > 0 { 4 } else { 0 };
    writeln!(visitor.writer_mut(), ".RS {rs_indent}")?;
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

        // Visit attached blocks (list continuation content)
        if !item.blocks.is_empty() {
            writeln!(visitor.writer_mut(), ".RS 0")?;
            for block in &item.blocks {
                visitor.visit_block(block)?;
            }
            writeln!(visitor.writer_mut(), ".RE")?;
        }
    }

    // Close the RS scope and restore nesting depth
    visitor.list_depth -= 1;
    let w = visitor.writer_mut();
    writeln!(w, ".RE")?;

    Ok(())
}

/// Visit a description list (term/definition pairs).
pub(crate) fn visit_description_list<W: Write>(
    list: &DescriptionList,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    // Optional list title
    visitor.render_title_with_wrapper(&list.title, ".sp\n\\fB", "\\fP\n")?;

    // Wrap in RS/RE to scope .TP indentation
    let rs_indent = if visitor.list_depth > 0 { 4 } else { 0 };
    writeln!(visitor.writer_mut(), ".RS {rs_indent}")?;
    visitor.list_depth += 1;

    for item in &list.items {
        let w = visitor.writer_mut();
        // Tagged paragraph
        writeln!(w, ".TP")?;

        // Term (bold)
        write!(w, "\\fB")?;
        visitor.visit_inline_nodes(&item.term)?;
        let w = visitor.writer_mut();
        writeln!(w, "\\fP")?;

        // Principal text (inline content after :: on same line)
        if !item.principal_text.is_empty() {
            visitor.visit_inline_nodes(&item.principal_text)?;
            let w = visitor.writer_mut();
            writeln!(w)?;
        }

        // Description blocks (continuation content)
        if !item.description.is_empty() {
            writeln!(visitor.writer_mut(), ".RS 0")?;
            for block in &item.description {
                visitor.visit_block(block)?;
            }
            writeln!(visitor.writer_mut(), ".RE")?;
        }
    }

    // Close the RS scope and restore nesting depth
    visitor.list_depth -= 1;
    writeln!(visitor.writer_mut(), ".RE")?;

    Ok(())
}

/// Visit a callout list.
pub(crate) fn visit_callout_list<W: Write>(
    list: &CalloutList,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    // Optional list title
    visitor.render_title_with_wrapper(&list.title, ".sp\n\\fB", "\\fP\n")?;

    // Wrap in RS/RE to scope .IP indentation
    let rs_indent = if visitor.list_depth > 0 { 4 } else { 0 };
    writeln!(visitor.writer_mut(), ".RS {rs_indent}")?;
    visitor.list_depth += 1;

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

        // Visit attached blocks (continuation content)
        if !item.blocks.is_empty() {
            writeln!(visitor.writer_mut(), ".RS 0")?;
            for block in &item.blocks {
                visitor.visit_block(block)?;
            }
            writeln!(visitor.writer_mut(), ".RE")?;
        }
    }

    // Close the RS scope and restore nesting depth
    visitor.list_depth -= 1;
    writeln!(visitor.writer_mut(), ".RE")?;

    Ok(())
}
