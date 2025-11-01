use std::io::Write;

use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{
    CalloutList, DescriptionList, ListItem, ListItemCheckedStatus, OrderedList, UnorderedList,
};

use crate::Error;

pub(crate) fn visit_unordered_list<V: WritableVisitor<Error = Error>>(
    list: &UnorderedList,
    visitor: &mut V,
) -> Result<(), Error> {
    let mut writer = visitor.writer_mut();
    write!(writer, "<div")?;
    // Use metadata.id if present, otherwise use first anchor
    if let Some(id) = &list.metadata.id {
        write!(writer, " id=\"{}\"", id.id)?;
    } else if let Some(anchor) = list.metadata.anchors.first() {
        write!(writer, " id=\"{}\"", anchor.id)?;
    }
    writeln!(writer, " class=\"ulist\">")?;
    writeln!(writer, "<ul>")?;
    let _ = writer;
    render_nested_list_items(&list.items, visitor, 1, false)?;
    writer = visitor.writer_mut();
    writeln!(writer, "</ul>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}

pub(crate) fn visit_ordered_list<V: WritableVisitor<Error = Error>>(
    list: &OrderedList,
    visitor: &mut V,
) -> Result<(), Error> {
    let mut writer = visitor.writer_mut();
    write!(writer, "<div")?;
    // Use metadata.id if present, otherwise use first anchor
    if let Some(id) = &list.metadata.id {
        write!(writer, " id=\"{}\"", id.id)?;
    } else if let Some(anchor) = list.metadata.anchors.first() {
        write!(writer, " id=\"{}\"", anchor.id)?;
    }
    writeln!(writer, " class=\"olist arabic\">")?;
    writeln!(writer, "<ol class=\"arabic\">")?;
    let _ = writer;
    render_nested_list_items(&list.items, visitor, 1, true)?;
    writer = visitor.writer_mut();
    writeln!(writer, "</ol>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}

pub(crate) fn visit_callout_list<V: WritableVisitor<Error = Error>>(
    list: &CalloutList,
    visitor: &mut V,
) -> Result<(), Error> {
    let mut writer = visitor.writer_mut();
    writeln!(writer, "<div class=\"colist arabic\">")?;
    if !list.title.is_empty() {
        write!(writer, "<div class=\"title\">")?;
        let _ = writer;
        visitor.visit_inline_nodes(&list.title)?;
        writer = visitor.writer_mut();
        writeln!(writer, "</div>")?;
    }
    writeln!(writer, "<ol>")?;
    let _ = writer;

    for item in &list.items {
        let mut writer = visitor.writer_mut();
        writeln!(writer, "<li>")?;
        // Render principal text as bare <p> (if not empty)
        if !item.principal.is_empty() {
            write!(writer, "<p>")?;
            let _ = writer;
            visitor.visit_inline_nodes(&item.principal)?;
            writer = visitor.writer_mut();
            writeln!(writer, "</p>")?;
        }
        let _ = writer;
        // Walk attached blocks using visitor
        for block in &item.blocks {
            visitor.visit_block(block)?;
        }
        writer = visitor.writer_mut();
        writeln!(writer, "</li>")?;
    }

    writer = visitor.writer_mut();
    writeln!(writer, "</ol>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}

/// Render nested list items hierarchically
#[tracing::instrument(skip(visitor))]
fn render_nested_list_items<V: WritableVisitor<Error = Error>>(
    items: &[ListItem],
    visitor: &mut V,
    expected_level: u8,
    is_ordered: bool,
) -> Result<(), Error> {
    let mut i = 0;
    while i < items.len() {
        let item = &items[i];

        if item.level < expected_level {
            // Item at lower level, return to parent
            break;
        }

        if item.level == expected_level {
            // Render item at current level
            let mut writer = visitor.writer_mut();
            writeln!(writer, "<li>")?;
            render_checked_status(item.checked.as_ref(), writer)?;
            // Render principal text as bare <p> (if not empty)
            if !item.principal.is_empty() {
                write!(writer, "<p>")?;
                let _ = writer;
                visitor.visit_inline_nodes(&item.principal)?;
                writer = visitor.writer_mut();
                writeln!(writer, "</p>")?;
            }
            let _ = writer;
            // Render attached blocks with their full wrapper divs
            for block in &item.blocks {
                visitor.visit_block(block)?;
            }

            // Check if next items are nested (higher level)
            if i + 1 < items.len() && items[i + 1].level > expected_level {
                // Find all items at the next level
                let next_level = items[i + 1].level;
                let inner_item = &items[i + 1];

                writer = visitor.writer_mut();
                // Open nested list
                if is_ordered {
                    writeln!(writer, "<div class=\"olist arabic")?;
                    if inner_item.checked.is_some() {
                        writeln!(writer, " checklist\">")?;
                    } else {
                        writeln!(writer, "\">")?;
                    }

                    write!(writer, "<ol class=\"arabic")?;
                    if inner_item.checked.is_some() {
                        writeln!(writer, " checklist\">")?;
                    } else {
                        writeln!(writer, "\">")?;
                    }
                } else {
                    // check if the item is a checkbox item
                    write!(writer, "<div class=\"ulist")?;
                    if inner_item.checked.is_some() {
                        writeln!(writer, " checklist\">")?;
                    } else {
                        writeln!(writer, "\">")?;
                    }
                    write!(writer, "<ul")?;
                    if inner_item.checked.is_some() {
                        writeln!(writer, " class=\"checklist\">")?;
                    } else {
                        writeln!(writer, ">")?;
                    }
                }
                let _ = writer;

                // Recursively render nested items
                i += 1;
                let nested_start = i;
                while i < items.len() && items[i].level >= next_level {
                    i += 1;
                }
                render_nested_list_items(&items[nested_start..i], visitor, next_level, is_ordered)?;

                writer = visitor.writer_mut();
                // Close nested list
                if is_ordered {
                    writeln!(writer, "</ol>")?;
                    writeln!(writer, "</div>")?;
                } else {
                    writeln!(writer, "</ul>")?;
                    writeln!(writer, "</div>")?;
                }
                let _ = writer;

                i -= 1; // Adjust because we'll increment at the end of the loop
            }

            writer = visitor.writer_mut();
            writeln!(writer, "</li>")?;
            i += 1;
        } else {
            // Item at higher level than expected, shouldn't happen in well-formed input
            // but handle gracefully by treating as same level - render the item inline
            let mut writer = visitor.writer_mut();
            writeln!(writer, "<li>")?;
            render_checked_status(item.checked.as_ref(), writer)?;
            if !item.principal.is_empty() {
                write!(writer, "<p>")?;
                let _ = writer;
                visitor.visit_inline_nodes(&item.principal)?;
                writer = visitor.writer_mut();
                writeln!(writer, "</p>")?;
            }
            let _ = writer;
            for block in &item.blocks {
                visitor.visit_block(block)?;
            }
            writer = visitor.writer_mut();
            writeln!(writer, "</li>")?;
            i += 1;
        }
    }
    Ok(())
}

pub(crate) fn visit_description_list<V: WritableVisitor<Error = Error>>(
    list: &DescriptionList,
    visitor: &mut V,
) -> Result<(), Error> {
    let mut writer = visitor.writer_mut();
    write!(writer, "<div")?;
    // Use metadata.id if present, otherwise use first anchor
    if let Some(id) = &list.metadata.id {
        write!(writer, " id=\"{}\"", id.id)?;
    } else if let Some(anchor) = list.metadata.anchors.first() {
        write!(writer, " id=\"{}\"", anchor.id)?;
    }
    writeln!(writer, " class=\"dlist\">")?;
    writeln!(writer, "<dl>")?;
    let _ = writer;

    for item in &list.items {
        let mut writer = visitor.writer_mut();
        writeln!(writer, "<dt class=\"hdlist1\">")?;
        let _ = writer;
        visitor.visit_inline_nodes(&item.term)?;
        writer = visitor.writer_mut();
        writeln!(writer, "</dt>")?;
        writeln!(writer, "<dd>")?;
        if !item.principal_text.is_empty() {
            write!(writer, "<p>")?;
            let _ = writer;
            visitor.visit_inline_nodes(&item.principal_text)?;
            writer = visitor.writer_mut();
            writeln!(writer, "</p>")?;
        }
        let _ = writer;
        for block in &item.description {
            visitor.visit_block(block)?;
        }
        writer = visitor.writer_mut();
        writeln!(writer, "</dd>")?;
    }

    writer = visitor.writer_mut();
    writeln!(writer, "</dl>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}

#[tracing::instrument(skip(w))]
fn render_checked_status<W: Write + ?Sized>(
    checked: Option<&ListItemCheckedStatus>,
    w: &mut W,
) -> Result<(), Error> {
    match checked {
        Some(ListItemCheckedStatus::Checked) => {
            write!(w, "&#10003; ")?; // Checked box
        }
        Some(ListItemCheckedStatus::Unchecked) => {
            write!(w, "&#10063; ")?; // Unchecked box
        }
        None => {}
    }
    Ok(())
}
