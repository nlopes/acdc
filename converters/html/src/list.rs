use std::io::Write;

use acdc_converters_core::visitor::{WritableVisitor, WritableVisitorExt};
use acdc_parser::{
    CalloutList, DescriptionList, ListItem, ListItemCheckedStatus, OrderedList, UnorderedList,
};

use crate::{Error, Processor, build_class};

/// Check if any list item has a checkbox
fn has_checklist_items(items: &[ListItem]) -> bool {
    items.iter().any(|item| item.checked.is_some())
}

/// Get the ordered list style for a given nesting depth
/// Cycles through: arabic -> loweralpha -> lowerroman -> upperalpha -> upperroman -> arabic...
///
/// Note: This differs from asciidoctor, which stops cycling after depth 5 and uses
/// `arabic` for all depths > 5. We intentionally continue cycling to provide better
/// visual distinction in deeply nested lists (6+ levels). This is a design decision
/// that improves usability for deep hierarchies while maintaining consistency.
fn ordered_list_style(depth: u8) -> (&'static str, Option<&'static str>) {
    match depth % 5 {
        2 => ("loweralpha", Some("a")), // a, b, c
        3 => ("lowerroman", Some("i")), // i, ii, iii
        4 => ("upperalpha", Some("A")), // A, B, C
        0 => ("upperroman", Some("I")), // I, II, III (depth 5, 10, 15...)
        _ => ("arabic", None),          // 1, 2, 3 (default type for depth 1, 6, 11...)
    }
}

pub(crate) fn visit_unordered_list<V: WritableVisitor<Error = Error>>(
    list: &UnorderedList,
    visitor: &mut V,
    section_style: Option<&str>,
) -> Result<(), Error> {
    let is_checklist = has_checklist_items(&list.items);
    let is_bibliography = section_style == Some("bibliography");
    let writer = visitor.writer_mut();
    write!(writer, "<div")?;
    // Use metadata.id if present, otherwise use first anchor
    if let Some(id) = &list.metadata.id {
        write!(writer, " id=\"{}\"", id.id)?;
    } else if let Some(anchor) = list.metadata.anchors.first() {
        write!(writer, " id=\"{}\"", anchor.id)?;
    }
    if is_checklist {
        writeln!(writer, " class=\"ulist checklist\">")?;
    } else if is_bibliography {
        writeln!(writer, " class=\"ulist bibliography\">")?;
    } else {
        writeln!(writer, " class=\"ulist\">")?;
    }
    let _ = writer;
    visitor.render_title_with_wrapper(&list.title, "<div class=\"title\">", "</div>\n")?;
    let mut writer = visitor.writer_mut();
    if is_checklist {
        writeln!(writer, "<ul class=\"checklist\">")?;
    } else if is_bibliography {
        writeln!(writer, "<ul class=\"bibliography\">")?;
    } else {
        writeln!(writer, "<ul>")?;
    }
    let _ = writer;
    render_nested_list_items(&list.items, visitor, 1, false, 1)?;
    writer = visitor.writer_mut();
    writeln!(writer, "</ul>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}

pub(crate) fn visit_ordered_list<V: WritableVisitor<Error = Error>>(
    list: &OrderedList,
    visitor: &mut V,
) -> Result<(), Error> {
    let raw_depth = list.marker.matches('.').count().max(1);
    if raw_depth > usize::from(u8::MAX) {
        tracing::warn!(raw_depth, "ordered list marker depth exceeds 255, clamping");
    }
    let depth = u8::try_from(raw_depth).unwrap_or(u8::MAX);
    let (style, type_attr) = ordered_list_style(depth);
    let writer = visitor.writer_mut();
    write!(writer, "<div")?;
    // Use metadata.id if present, otherwise use first anchor
    if let Some(id) = &list.metadata.id {
        write!(writer, " id=\"{}\"", id.id)?;
    } else if let Some(anchor) = list.metadata.anchors.first() {
        write!(writer, " id=\"{}\"", anchor.id)?;
    }
    writeln!(writer, " class=\"olist {style}\">")?;
    let _ = writer;
    visitor.render_title_with_wrapper(&list.title, "<div class=\"title\">", "</div>\n")?;
    let mut writer = visitor.writer_mut();
    if let Some(t) = type_attr {
        writeln!(writer, "<ol class=\"{style}\" type=\"{t}\">")?;
    } else {
        writeln!(writer, "<ol class=\"{style}\">")?;
    }
    let _ = writer;
    render_nested_list_items(&list.items, visitor, 1, true, 1)?;
    writer = visitor.writer_mut();
    writeln!(writer, "</ol>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}

pub(crate) fn visit_callout_list<V: WritableVisitor<Error = Error>>(
    list: &CalloutList,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    let writer = visitor.writer_mut();
    writeln!(writer, "<div class=\"colist arabic\">")?;
    let _ = writer;
    visitor.render_title_with_wrapper(&list.title, "<div class=\"title\">", "</div>\n")?;

    if processor.is_font_icons_mode() {
        let writer = visitor.writer_mut();
        writeln!(writer, "<table>")?;
        let _ = writer;

        for item in &list.items {
            let num = item.callout.number;
            let writer = visitor.writer_mut();
            writeln!(writer, "<tr>")?;
            writeln!(
                writer,
                "<td><i class=\"conum\" data-value=\"{num}\"></i><b>{num}</b></td>"
            )?;
            write!(writer, "<td>")?;
            let _ = writer;
            visitor.visit_inline_nodes(&item.principal)?;
            for block in &item.blocks {
                visitor.visit_block(block)?;
            }
            let writer = visitor.writer_mut();
            writeln!(writer, "</td>")?;
            writeln!(writer, "</tr>")?;
        }

        let writer = visitor.writer_mut();
        writeln!(writer, "</table>")?;
    } else {
        let writer = visitor.writer_mut();
        writeln!(writer, "<ol>")?;
        let _ = writer;

        for item in &list.items {
            let writer = visitor.writer_mut();
            write!(writer, "<li>")?;
            write!(writer, "<p>")?;
            let _ = writer;
            visitor.visit_inline_nodes(&item.principal)?;
            let writer = visitor.writer_mut();
            write!(writer, "</p>")?;
            let _ = writer;
            for block in &item.blocks {
                visitor.visit_block(block)?;
            }
            let writer = visitor.writer_mut();
            writeln!(writer, "</li>")?;
        }

        let writer = visitor.writer_mut();
        writeln!(writer, "</ol>")?;
    }

    let writer = visitor.writer_mut();
    writeln!(writer, "</div>")?;
    Ok(())
}

fn render_checked_status_list<W: Write + ?Sized>(
    is_ordered: bool,
    checked: Option<&ListItemCheckedStatus>,
    depth: u8,
    writer: &mut W,
) -> Result<(), Error> {
    // Open nested list
    if is_ordered {
        let (style, type_attr) = ordered_list_style(depth);
        write!(writer, "<div class=\"olist {style}")?;
        if checked.is_some() {
            writeln!(writer, " checklist\">")?;
        } else {
            writeln!(writer, "\">")?;
        }

        write!(writer, "<ol class=\"{style}")?;
        if checked.is_some() {
            write!(writer, " checklist\"")?;
        } else {
            write!(writer, "\"")?;
        }
        if let Some(t) = type_attr {
            writeln!(writer, " type=\"{t}\">")?;
        } else {
            writeln!(writer, ">")?;
        }
    } else {
        // check if the item is a checkbox item
        write!(writer, "<div class=\"ulist")?;
        if checked.is_some() {
            writeln!(writer, " checklist\">")?;
        } else {
            writeln!(writer, "\">")?;
        }
        write!(writer, "<ul")?;
        if checked.is_some() {
            writeln!(writer, " class=\"checklist\">")?;
        } else {
            writeln!(writer, ">")?;
        }
    }
    Ok(())
}

/// Render nested list items hierarchically
/// `depth` tracks the nesting level for ordered list style cycling (1 = top level)
#[tracing::instrument(skip(visitor))]
fn render_nested_list_items<V: WritableVisitor<Error = Error>>(
    items: &[ListItem],
    visitor: &mut V,
    expected_level: u8,
    is_ordered: bool,
    depth: u8,
) -> Result<(), Error> {
    let mut i = 0;
    while i < items.len() {
        let item = items.get(i).ok_or(Error::IndexOutOfBounds(
            "Index out of bounds while rendering nested list items",
            i,
        ))?;

        if item.level < expected_level {
            // Item at lower level, return to parent
            break;
        }

        if item.level == expected_level {
            // Render item at current level
            let mut writer = visitor.writer_mut();
            writeln!(writer, "<li>")?;
            // Render principal text as bare <p> (if not empty)
            // Checkbox goes inside the <p> tag
            if !item.principal.is_empty() || item.checked.is_some() {
                write!(writer, "<p>")?;
                render_checked_status(item.checked.as_ref(), writer)?;
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
            if i + 1 < items.len()
                && let Some(inner_item) = items.get(i + 1)
                && inner_item.level > expected_level
            {
                // Find all items at the next level
                let next_level = inner_item.level;

                writer = visitor.writer_mut();
                render_checked_status_list(
                    is_ordered,
                    inner_item.checked.as_ref(),
                    depth + 1,
                    writer,
                )?;
                let _ = writer;

                // Recursively render nested items
                i += 1;
                let nested_start = i;
                // Find all consecutive items at or deeper than next_level
                while i < items.len() && items.get(i).is_some_and(|item| item.level >= next_level) {
                    i += 1;
                }
                if let Some(inner_items) = items.get(nested_start..i) {
                    render_nested_list_items(
                        inner_items,
                        visitor,
                        next_level,
                        is_ordered,
                        depth + 1,
                    )?;
                }
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
            // Checkbox goes inside the <p> tag
            if !item.principal.is_empty() || item.checked.is_some() {
                write!(writer, "<p>")?;
                render_checked_status(item.checked.as_ref(), writer)?;
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
    // Start the description list outer div
    let writer = visitor.writer_mut();
    write!(writer, "<div")?;
    // Use metadata.id if present, otherwise use first anchor
    if let Some(id) = &list.metadata.id {
        write!(writer, " id=\"{}\"", id.id)?;
    } else if let Some(anchor) = list.metadata.anchors.first() {
        write!(writer, " id=\"{}\"", anchor.id)?;
    }

    // Description list
    let is_horizontal = list.metadata.style.as_deref() == Some("horizontal");
    if is_horizontal {
        visit_horizontal_description_list(list, visitor)?;
    } else {
        visit_standard_description_list(list, visitor)?;
    }

    let writer = visitor.writer_mut();
    // Close the description list
    writeln!(writer, "</div>")?;
    Ok(())
}

/// Renders a horizontal description list as an HTML table with `hdlist` class.
/// This matches asciidoctor's output for `[horizontal]` style description lists.
fn visit_horizontal_description_list<V: WritableVisitor<Error = Error>>(
    list: &DescriptionList,
    visitor: &mut V,
) -> Result<(), Error> {
    let writer = visitor.writer_mut();

    let class = build_class("hdlist", &list.metadata.roles);
    writeln!(writer, " class=\"{class}\">")?;
    let _ = writer;
    visitor.render_title_with_wrapper(&list.title, "<div class=\"title\">", "</div>\n")?;
    let mut writer = visitor.writer_mut();
    writeln!(writer, "<table>")?;
    let _ = writer;

    for item in &list.items {
        let mut writer = visitor.writer_mut();
        writeln!(writer, "<tr>")?;
        writeln!(writer, "<td class=\"hdlist1\">")?;
        let _ = writer;
        visitor.visit_inline_nodes(&item.term)?;
        writer = visitor.writer_mut();
        writeln!(writer, "</td>")?;
        writeln!(writer, "<td class=\"hdlist2\">")?;
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
        writeln!(writer, "</td>")?;
        writeln!(writer, "</tr>")?;
    }

    writer = visitor.writer_mut();
    writeln!(writer, "</table>")?;
    Ok(())
}

/// Renders a standard description list as an HTML `<dl>` with `dlist` class.
fn visit_standard_description_list<V: WritableVisitor<Error = Error>>(
    list: &DescriptionList,
    visitor: &mut V,
) -> Result<(), Error> {
    let writer = visitor.writer_mut();

    // Check for ordered/unordered style (affects dt class)
    let is_marker_style = list
        .metadata
        .style
        .as_deref()
        .is_some_and(|s| s == "ordered" || s == "unordered");

    // Build class including style if present
    let base_class = if let Some(style) = &list.metadata.style {
        format!("dlist {style}")
    } else {
        "dlist".to_string()
    };
    let class = build_class(&base_class, &list.metadata.roles);
    writeln!(writer, " class=\"{class}\">")?;
    let _ = writer;
    visitor.render_title_with_wrapper(&list.title, "<div class=\"title\">", "</div>\n")?;
    let mut writer = visitor.writer_mut();
    writeln!(writer, "<dl>")?;
    let _ = writer;

    for item in &list.items {
        let mut writer = visitor.writer_mut();
        // Only add hdlist1 class when NOT ordered/unordered
        if is_marker_style {
            writeln!(writer, "<dt>")?;
        } else {
            writeln!(writer, "<dt class=\"hdlist1\">")?;
        }
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
        Some(_) | None => {}
    }
    Ok(())
}
