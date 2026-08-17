use std::io::{self, BufWriter, Write};

use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use acdc_converters_core::{
    list::OrderedListNumbering,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{
    Block, CalloutList, DescriptionList, DescriptionListItem, InlineNode, ListItem,
    ListItemCheckedStatus, OrderedList, UnorderedList,
};

use crate::{Error, TerminalVisitor};

/// Write `indent` spaces to the writer. Does nothing when `indent` is 0.
fn write_indent(w: &mut dyn Write, indent: usize) -> Result<(), Error> {
    if indent > 0 {
        write!(w, "{:indent$}", "", indent = indent)?;
    }
    Ok(())
}

#[tracing::instrument(skip(w))]
fn render_checked_status<W: Write + ?Sized>(
    checked: Option<&ListItemCheckedStatus>,
    w: &mut W,
    unicode: bool,
) -> Result<(), Error> {
    if let Some(checked) = checked {
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

#[derive(Clone, Copy, Debug)]
enum ListMarker {
    Bullet,
    Numbered(OrderedListNumbering),
    Hidden,
}

fn style_suppresses_marker(style: Option<&str>) -> bool {
    matches!(style, Some("none" | "no-bullet" | "unstyled"))
}

impl<W: Write> TerminalVisitor<'_, '_, W> {
    /// Render a title with italic styling.
    ///
    /// This helper function renders inline nodes to a buffer, converts to a string,
    /// trims whitespace, and applies italic styling for terminal output.
    #[tracing::instrument(skip(self))]
    fn render_styled_title(&mut self, title: &[InlineNode]) -> Result<(), Error> {
        if !title.is_empty() {
            let processor = self.processor.clone();
            let buffer = Vec::new();
            let inner = BufWriter::new(buffer);
            let mut temp_visitor =
                TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());
            for node in title {
                temp_visitor.visit_inline_node(node)?;
            }
            let buffer = temp_visitor
                .into_writer()
                .into_inner()
                .map_err(io::IntoInnerError::into_error)?;
            let w = self.writer_mut();
            w.queue(PrintStyledContent(
                String::from_utf8_lossy(&buffer).trim().to_string().italic(),
            ))?;
        }
        Ok(())
    }

    /// Render list items with proper indentation.
    ///
    /// All items in a single list are at the same nesting level. Nested lists appear
    /// as `Block` children within individual items and are handled by the visitor
    /// pattern (which reads `processor.list_indent` for the correct indentation).
    #[tracing::instrument(skip(self))]
    fn render_list_items(
        &mut self,
        items: &[ListItem],
        indent: usize,
        marker: ListMarker,
        start: usize,
        unicode: bool,
    ) -> Result<(), Error> {
        for (idx, item) in items.iter().enumerate() {
            self.render_list_item(item, indent, marker, start.saturating_add(idx), unicode)?;
        }
        Ok(())
    }

    /// Render a single list item with the specified indentation.
    ///
    /// After rendering the item's principal text, child blocks (which may include
    /// nested lists) are visited with an increased `list_indent` on the processor.
    #[tracing::instrument(skip(self))]
    fn render_list_item(
        &mut self,
        item: &ListItem,
        indent: usize,
        marker: ListMarker,
        item_number: usize,
        unicode: bool,
    ) -> Result<(), Error> {
        write_indent(&mut self.writer, indent)?;

        let marker_written = match marker {
            ListMarker::Bullet => {
                write!(self.writer, "*")?;
                true
            }
            ListMarker::Numbered(numbering) => {
                write!(self.writer, "{}.", numbering.format(item_number))?;
                true
            }
            ListMarker::Hidden => false,
        };

        if marker_written {
            write!(self.writer, " ")?;
        }

        render_checked_status(item.checked.as_ref(), &mut self.writer, unicode)?;
        if item.checked.is_some() {
            write!(self.writer, " ")?;
        }

        for node in &item.principal {
            self.visit_inline_node(node)?;
        }

        writeln!(self.writer)?;

        // Render attached blocks with increased indentation.
        // Set list_indent so nested lists rendered via the visitor pick up the right depth.
        let nested_indent = indent + 2;
        let old_indent = self.processor.list_indent.get();
        self.processor.list_indent.set(nested_indent);
        for block in &item.blocks {
            self.visit_block(block)?;
        }
        self.processor.list_indent.set(old_indent);

        Ok(())
    }

    pub(crate) fn render_unordered_list(&mut self, list: &UnorderedList) -> Result<(), Error> {
        let indent = self.processor.list_indent.get();
        self.render_styled_title(&list.title)?;
        // Only emit a leading newline for top-level lists (not nested ones)
        if indent == 0 {
            let w = self.writer_mut();
            writeln!(w)?;
        }
        let unicode = self.processor.appearance.capabilities.unicode;
        let marker = if style_suppresses_marker(list.metadata.style) {
            ListMarker::Hidden
        } else {
            ListMarker::Bullet
        };
        self.render_list_items(&list.items, indent, marker, 1, unicode)?;
        Ok(())
    }

    /// Renders an ordered list in terminal format.
    ///
    /// Items are numbered from 1 by default, and nested lists restart numbering
    /// at each level. Markerless styles omit the item markers.
    ///
    /// # Format
    /// ```text
    /// 1. First item
    /// 2. Second item
    ///    1. Nested item
    /// 3. Third item
    /// ```
    pub(crate) fn render_ordered_list(&mut self, list: &OrderedList) -> Result<(), Error> {
        let indent = self.processor.list_indent.get();
        self.render_styled_title(&list.title)?;
        // Only emit a leading newline for top-level lists (not nested ones)
        if indent == 0 {
            let w = self.writer_mut();
            writeln!(w)?;
        }
        let unicode = self.processor.appearance.capabilities.unicode;
        let marker = if style_suppresses_marker(list.metadata.style)
            || list.metadata.style == Some("unnumbered")
        {
            ListMarker::Hidden
        } else {
            let numbering = list
                .metadata
                .style
                .and_then(OrderedListNumbering::from_explicit_style)
                .unwrap_or_default();
            ListMarker::Numbered(numbering)
        };
        let start = list
            .metadata
            .attributes
            .get_string("start")
            .and_then(|start| start.parse::<usize>().ok())
            .filter(|start| *start > 0)
            .unwrap_or(1);
        self.render_list_items(&list.items, indent, marker, start, unicode)?;
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
    pub(crate) fn render_callout_list(&mut self, list: &CalloutList) -> Result<(), Error> {
        self.render_styled_title(&list.title)?;
        if !list.title.is_empty() {
            writeln!(self.writer)?;
        }

        for (idx, item) in list.items.iter().enumerate() {
            let item_number = idx + 1;
            write!(self.writer, "<{item_number}>")?;
            write!(self.writer, " ")?;

            // Render principal text inline
            for node in &item.principal {
                self.visit_inline_node(node)?;
            }

            writeln!(self.writer)?;

            // Render attached blocks with indentation
            for block in &item.blocks {
                write!(self.writer, "  ")?;
                self.visit_block(block)?;
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
    pub(crate) fn render_description_list(&mut self, list: &DescriptionList) -> Result<(), Error> {
        let indent = self.processor.list_indent.get();
        self.render_styled_title(&list.title)?;
        if indent == 0 || !list.title.is_empty() {
            writeln!(self.writer)?;
        }

        let style = list.metadata.style;

        for item in &list.items {
            match style {
                Some("horizontal") => {
                    self.render_horizontal_description_list_item(item)?;
                }
                Some("qanda") => {
                    self.render_qanda_description_list_item(item)?;
                }
                _ => {
                    self.render_description_list_item(item)?;
                }
            }
        }
        Ok(())
    }

    /// Renders a single description list item (term and definition) in default style.
    ///
    /// The term is rendered in bold, followed by the principal text (if present)
    /// indented with 2 spaces, and any additional description blocks.
    fn render_description_list_item(&mut self, item: &DescriptionListItem) -> Result<(), Error> {
        let indent = self.processor.list_indent.get();
        // Render term in bold
        let processor = self.processor.clone();
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());
        for node in &item.term {
            temp_visitor.visit_inline_node(node)?;
        }
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;

        write_indent(&mut self.writer, indent)?;
        self.writer.queue(PrintStyledContent(
            String::from_utf8_lossy(&buffer).to_string().bold(),
        ))?;
        writeln!(self.writer)?;

        // Render principal text with indentation if present
        if !item.principal_text.is_empty() {
            write_indent(&mut self.writer, indent + 2)?;
            for node in &item.principal_text {
                self.visit_inline_node(node)?;
            }
            writeln!(self.writer)?;
        }

        self.render_description_blocks(&item.description, indent + 2)
    }

    /// Renders a single description list item in horizontal style.
    ///
    /// Term and definition are on the same line, separated by `::`.
    fn render_horizontal_description_list_item(
        &mut self,
        item: &DescriptionListItem,
    ) -> Result<(), Error> {
        let indent = self.processor.list_indent.get();
        // Render term in bold
        let processor = self.processor.clone();
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());
        for node in &item.term {
            temp_visitor.visit_inline_node(node)?;
        }
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;

        write_indent(&mut self.writer, indent)?;
        self.writer.queue(PrintStyledContent(
            String::from_utf8_lossy(&buffer).to_string().bold(),
        ))?;

        // Same line: term :: definition
        if !item.principal_text.is_empty() {
            write!(self.writer, " :: ")?;
            for node in &item.principal_text {
                self.visit_inline_node(node)?;
            }
        }
        writeln!(self.writer)?;

        // Render description blocks indented
        self.render_description_blocks(&item.description, indent + 2)
    }

    /// Renders a single description list item in Q&A style.
    ///
    /// Terms are prefixed with "Q: " and definitions with "A: ".
    fn render_qanda_description_list_item(
        &mut self,
        item: &DescriptionListItem,
    ) -> Result<(), Error> {
        let indent = self.processor.list_indent.get();
        // Render "Q: " prefix + term in bold
        let processor = self.processor.clone();
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());
        for node in &item.term {
            temp_visitor.visit_inline_node(node)?;
        }
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;

        write_indent(&mut self.writer, indent)?;
        self.writer.queue(PrintStyledContent("Q: ".bold()))?;
        self.writer.queue(PrintStyledContent(
            String::from_utf8_lossy(&buffer).to_string().bold(),
        ))?;
        writeln!(self.writer)?;

        // Render "A: " prefix + principal text
        if !item.principal_text.is_empty() {
            write_indent(&mut self.writer, indent)?;
            self.writer.queue(PrintStyledContent("A: ".dim()))?;
            for node in &item.principal_text {
                self.visit_inline_node(node)?;
            }
            writeln!(self.writer)?;
        }

        // Render description blocks indented
        self.render_description_blocks(&item.description, indent + 3)
    }

    fn render_description_blocks(
        &mut self,
        blocks: &[Block<'_>],
        indent: usize,
    ) -> Result<(), Error> {
        let previous_indent = self.processor.list_indent.replace(indent);
        let result = blocks.iter().try_for_each(|block| {
            if !matches!(
                block,
                Block::DescriptionList(_) | Block::OrderedList(_) | Block::UnorderedList(_)
            ) {
                write_indent(&mut self.writer, indent)?;
            }
            self.visit_block(block)
        });
        self.processor.list_indent.set(previous_indent);
        result
    }
}
