//! List rendering for manpages.
//!
//! Handles unordered, ordered, description, and callout lists using
//! `.IP`, `.TP`, `.RS`, and `.RE` macros.

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{
    CalloutList, DescriptionList, InlineNode, ListItemCheckedStatus, OrderedList, UnorderedList,
};

use crate::{Error, ManpageVisitor};

impl<W: Write> ManpageVisitor<'_, W> {
    fn with_list_scope(
        &mut self,
        title: &[InlineNode],
        render_items: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.render_title_with_wrapper(title, ".sp\n\\fB", "\\fP\n")?;

        let rs_indent = if self.list_depth > 0 { 4 } else { 0 };
        writeln!(self.writer_mut(), ".RS {rs_indent}")?;
        self.list_depth += 1;

        let result = render_items(self);

        self.list_depth -= 1;
        writeln!(self.writer_mut(), ".RE")?;

        result
    }

    /// Visit an unordered (bulleted) list.
    pub(crate) fn render_unordered_list(&mut self, list: &UnorderedList) -> Result<(), Error> {
        self.with_list_scope(&list.title, |visitor| {
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

            Ok(())
        })
    }

    /// Visit an ordered (numbered) list.
    pub(crate) fn render_ordered_list(&mut self, list: &OrderedList) -> Result<(), Error> {
        self.with_list_scope(&list.title, |visitor| {
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

            Ok(())
        })
    }

    /// Visit a description list (term/definition pairs).
    pub(crate) fn render_description_list(&mut self, list: &DescriptionList) -> Result<(), Error> {
        self.with_list_scope(&list.title, |visitor| {
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

            Ok(())
        })
    }

    /// Visit a callout list.
    pub(crate) fn render_callout_list(&mut self, list: &CalloutList) -> Result<(), Error> {
        self.with_list_scope(&list.title, |visitor| {
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

            Ok(())
        })
    }
}
