//! List rendering for manpages.
//!
//! Handles unordered, ordered, description, and callout lists using
//! `.IP`, `.TP`, `.RS`, and `.RE` macros.

use std::io::Write;

use acdc_converters_core::{
    list::OrderedListNumbering,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{
    CalloutList, DescriptionList, DescriptionListItem, InlineNode, ListItem, ListItemCheckedStatus,
    OrderedList, UnorderedList,
};

use crate::{Error, ManpageVisitor};

fn style_suppresses_marker(style: Option<&str>) -> bool {
    matches!(style, Some("none" | "no-bullet" | "unstyled"))
}

impl<W: Write> ManpageVisitor<'_, '_, W> {
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
        if list.metadata.style == Some("bibliography") {
            return self.render_bibliography_list(list);
        }

        let suppress_marker = style_suppresses_marker(list.metadata.style);
        self.with_list_scope(&list.title, |visitor| {
            for item in &list.items {
                let w = visitor.writer_mut();
                if suppress_marker {
                    writeln!(w, ".IP \"\" 2")?;
                } else {
                    writeln!(w, ".IP \\(bu 2")?;
                }

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
        let suppress_marker = style_suppresses_marker(list.metadata.style)
            || list.metadata.style == Some("unnumbered");
        let numbering = list
            .metadata
            .style
            .and_then(OrderedListNumbering::from_explicit_style)
            .unwrap_or_default();
        let reversed = list.metadata.options.contains(&"reversed");
        let start = list
            .metadata
            .attributes
            .get_string("start")
            .and_then(|start| start.parse::<usize>().ok())
            .filter(|start| *start > 0)
            .unwrap_or(if reversed { list.items.len() } else { 1 });
        self.with_list_scope(&list.title, |visitor| {
            for (i, item) in list.items.iter().enumerate() {
                let w = visitor.writer_mut();
                if suppress_marker {
                    writeln!(w, ".IP \"\" 4")?;
                } else {
                    let number = if reversed {
                        start.saturating_sub(i)
                    } else {
                        start.saturating_add(i)
                    };
                    writeln!(w, ".IP {}. 4", numbering.format(number))?;
                }

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
            for (index, item) in list.items.iter().enumerate() {
                match list.metadata.style {
                    Some("horizontal") => visitor.render_horizontal_description_item(item)?,
                    Some("qanda") => visitor.render_qanda_description_item(item)?,
                    Some("ordered") => visitor
                        .render_tagged_description_item(item, Some(&format!("{}. ", index + 1)))?,
                    Some("unordered") => {
                        visitor.render_tagged_description_item(item, Some("\\(bu "))?;
                    }
                    Some(_) | None => visitor.render_tagged_description_item(item, None)?,
                }
            }

            Ok(())
        })
    }

    fn render_bibliography_list(&mut self, list: &UnorderedList) -> Result<(), Error> {
        self.with_list_scope(&list.title, |visitor| {
            for item in &list.items {
                let Some((InlineNode::InlineAnchor(anchor), content)) =
                    item.principal.split_first()
                else {
                    visitor.render_unordered_item(item, false)?;
                    continue;
                };
                if !anchor.is_bibliography() {
                    visitor.render_unordered_item(item, false)?;
                    continue;
                }

                let label = visitor
                    .processor
                    .references
                    .get(anchor.id)
                    .and_then(|reference| reference.xreflabel.as_deref())
                    .map(<[_]>::to_vec);
                writeln!(visitor.writer_mut(), ".TP")?;
                write!(visitor.writer_mut(), "\\fB")?;
                if let Some(label) = label {
                    visitor.visit_inline_nodes(&label)?;
                } else {
                    write!(visitor.writer_mut(), "[{}]", anchor.id)?;
                }
                writeln!(visitor.writer_mut(), "\\fP")?;
                visitor.render_list_item_content(content, &item.blocks, 0)?;
            }
            Ok(())
        })
    }

    fn render_unordered_item(&mut self, item: &ListItem<'_>, suppress: bool) -> Result<(), Error> {
        if suppress {
            writeln!(self.writer_mut(), ".IP \"\" 2")?;
        } else {
            writeln!(self.writer_mut(), ".IP \\(bu 2")?;
        }
        self.render_list_item_content(&item.principal, &item.blocks, 2)
    }

    fn render_list_item_content(
        &mut self,
        principal: &[InlineNode<'_>],
        blocks: &[acdc_parser::Block<'_>],
        indent: usize,
    ) -> Result<(), Error> {
        if !principal.is_empty() {
            self.visit_inline_nodes(principal)?;
            writeln!(self.writer_mut())?;
        }
        if !blocks.is_empty() {
            writeln!(self.writer_mut(), ".RS {indent}")?;
            for block in blocks {
                self.visit_block(block)?;
            }
            writeln!(self.writer_mut(), ".RE")?;
        }
        Ok(())
    }

    fn render_tagged_description_item(
        &mut self,
        item: &DescriptionListItem<'_>,
        marker: Option<&str>,
    ) -> Result<(), Error> {
        writeln!(self.writer_mut(), ".TP")?;
        write!(self.writer_mut(), "\\fB")?;
        if let Some(marker) = marker {
            write!(self.writer_mut(), "{marker}")?;
        }
        self.visit_inline_nodes(&item.term)?;
        writeln!(self.writer_mut(), "\\fP")?;
        self.render_description_content(item, None)
    }

    fn render_horizontal_description_item(
        &mut self,
        item: &DescriptionListItem<'_>,
    ) -> Result<(), Error> {
        writeln!(self.writer_mut(), ".sp")?;
        write!(self.writer_mut(), "\\fB")?;
        self.visit_inline_nodes(&item.term)?;
        write!(self.writer_mut(), "\\fP")?;
        if !item.principal_text.is_empty() {
            write!(self.writer_mut(), " \\(en ")?;
            self.visit_inline_nodes(&item.principal_text)?;
        }
        writeln!(self.writer_mut())?;
        self.render_description_blocks(&item.description, 4)
    }

    fn render_qanda_description_item(
        &mut self,
        item: &DescriptionListItem<'_>,
    ) -> Result<(), Error> {
        writeln!(self.writer_mut(), ".TP")?;
        write!(self.writer_mut(), "\\fBQ: ")?;
        self.visit_inline_nodes(&item.term)?;
        writeln!(self.writer_mut(), "\\fP")?;
        self.render_description_content(item, Some("\\fBA:\\fP "))
    }

    fn render_description_content(
        &mut self,
        item: &DescriptionListItem<'_>,
        prefix: Option<&str>,
    ) -> Result<(), Error> {
        if !item.principal_text.is_empty() || (!item.description.is_empty() && prefix.is_some()) {
            if let Some(prefix) = prefix {
                write!(self.writer_mut(), "{prefix}")?;
            }
            self.visit_inline_nodes(&item.principal_text)?;
            writeln!(self.writer_mut())?;
        }
        self.render_description_blocks(&item.description, 0)
    }

    fn render_description_blocks(
        &mut self,
        blocks: &[acdc_parser::Block<'_>],
        indent: usize,
    ) -> Result<(), Error> {
        if !blocks.is_empty() {
            writeln!(self.writer_mut(), ".RS {indent}")?;
            for block in blocks {
                self.visit_block(block)?;
            }
            writeln!(self.writer_mut(), ".RE")?;
        }
        Ok(())
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
