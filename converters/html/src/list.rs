use std::io::Write;

use acdc_parser::{
    DescriptionList, DescriptionListItem, ListItem, ListItemCheckedStatus, OrderedList,
    UnorderedList,
};

use crate::{Processor, Render, RenderOptions};

impl Render for UnorderedList {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<div class=\"ulist\">")?;
        writeln!(w, "<ul>")?;
        render_nested_list_items(&self.items, w, processor, options, 1, false)?;
        writeln!(w, "</ul>")?;
        writeln!(w, "</div>")?;
        Ok(())
    }
}

impl Render for OrderedList {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<div class=\"olist arabic\">")?;
        writeln!(w, "<ol class=\"arabic\">")?;
        render_nested_list_items(&self.items, w, processor, options, 1, true)?;
        writeln!(w, "</ol>")?;
        writeln!(w, "</div>")?;
        Ok(())
    }
}

/// Render nested list items hierarchically
#[tracing::instrument(skip(w, processor))]
fn render_nested_list_items<W: Write>(
    items: &[ListItem],
    w: &mut W,
    processor: &Processor,
    options: &RenderOptions,
    expected_level: u8,
    is_ordered: bool,
) -> Result<(), crate::Error> {
    let mut i = 0;
    while i < items.len() {
        let item = &items[i];

        if item.level < expected_level {
            // Item at lower level, return to parent
            break;
        }

        if item.level == expected_level {
            // Render item at current level
            writeln!(w, "<li>")?;
            render_checked_status(item.checked.as_ref(), w)?;
            // Render principal text as bare <p> (if not empty)
            if !item.principal.is_empty() {
                writeln!(w, "<p>")?;
                crate::inlines::render_inlines(&item.principal, w, processor, options)?;
                writeln!(w, "</p>")?;
            }
            // Render attached blocks with their full wrapper divs
            for block in &item.blocks {
                block.render(w, processor, options)?;
            }

            // Check if next items are nested (higher level)
            if i + 1 < items.len() && items[i + 1].level > expected_level {
                // Find all items at the next level
                let next_level = items[i + 1].level;
                let inner_item = &items[i + 1];

                // Open nested list
                if is_ordered {
                    writeln!(w, "<div class=\"olist arabic")?;
                    if inner_item.checked.is_some() {
                        writeln!(w, " checklist\">")?;
                    } else {
                        writeln!(w, "\">")?;
                    }

                    write!(w, "<ol class=\"arabic")?;
                    if inner_item.checked.is_some() {
                        writeln!(w, " checklist\">")?;
                    } else {
                        writeln!(w, "\">")?;
                    }
                } else {
                    // check if the item is a checkbox item
                    write!(w, "<div class=\"ulist")?;
                    if inner_item.checked.is_some() {
                        writeln!(w, " checklist\">")?;
                    } else {
                        writeln!(w, "\">")?;
                    }
                    write!(w, "<ul")?;
                    if inner_item.checked.is_some() {
                        writeln!(w, " class=\"checklist\">")?;
                    } else {
                        writeln!(w, ">")?;
                    }
                }

                // Recursively render nested items
                i += 1;
                let nested_start = i;
                while i < items.len() && items[i].level >= next_level {
                    i += 1;
                }
                render_nested_list_items(
                    &items[nested_start..i],
                    w,
                    processor,
                    options,
                    next_level,
                    is_ordered,
                )?;

                // Close nested list
                if is_ordered {
                    writeln!(w, "</ol>")?;
                    writeln!(w, "</div>")?;
                } else {
                    writeln!(w, "</ul>")?;
                    writeln!(w, "</div>")?;
                }

                i -= 1; // Adjust because we'll increment at the end of the loop
            }

            writeln!(w, "</li>")?;
            i += 1;
        } else {
            // Item at higher level than expected, shouldn't happen in well-formed input
            // but handle gracefully by treating as same level
            item.render(w, processor, options)?;
            i += 1;
        }
    }
    Ok(())
}

impl Render for ListItem {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<li>")?;
        render_checked_status(self.checked.as_ref(), w)?;
        // Render principal text as bare <p> (if not empty)
        if !self.principal.is_empty() {
            writeln!(w, "<p>")?;
            crate::inlines::render_inlines(&self.principal, w, processor, options)?;
            writeln!(w, "</p>")?;
        }
        // Render attached blocks with their full wrapper divs
        for block in &self.blocks {
            block.render(w, processor, options)?;
        }
        writeln!(w, "</li>")?;
        Ok(())
    }
}

impl Render for DescriptionList {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<div class=\"dlist\">")?;
        writeln!(w, "<dl>")?;
        for item in &self.items {
            item.render(w, processor, options)?;
        }
        writeln!(w, "</dl>")?;
        writeln!(w, "</div>")?;
        Ok(())
    }
}

impl Render for DescriptionListItem {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<dt class=\"hdlist1\">")?;
        crate::inlines::render_inlines(&self.term, w, processor, options)?;
        writeln!(w, "</dt>")?;
        writeln!(w, "<dd>")?;
        if !self.principal_text.is_empty() {
            writeln!(w, "<p>")?;
            crate::inlines::render_inlines(&self.principal_text, w, processor, options)?;
            writeln!(w, "</p>")?;
        }
        for block in &self.description {
            block.render(w, processor, options)?;
        }
        writeln!(w, "</dd>")?;
        Ok(())
    }
}

#[tracing::instrument(skip(w))]
fn render_checked_status<W: Write>(
    checked: Option<&ListItemCheckedStatus>,
    w: &mut W,
) -> Result<(), crate::Error> {
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
