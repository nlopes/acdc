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

impl<W: Write> TerminalVisitor<'_, W> {
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
            let mut temp_visitor = TerminalVisitor::new(inner, processor);
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
        is_ordered: bool,
        unicode: bool,
    ) -> Result<(), Error> {
        for (idx, item) in items.iter().enumerate() {
            self.render_list_item(item, indent, is_ordered, idx + 1, unicode)?;
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
        is_ordered: bool,
        item_number: usize,
        unicode: bool,
    ) -> Result<(), Error> {
        let w = self.writer_mut();
        write_indent(w, indent)?;

        if is_ordered {
            write!(w, "{item_number}.")?;
        } else {
            write!(w, "*")?;
        }

        render_checked_status(item.checked.as_ref(), w, unicode)?;
        write!(w, " ")?;
        let _ = w;

        for node in &item.principal {
            self.visit_inline_node(node)?;
        }

        let w = self.writer_mut();
        writeln!(w)?;
        let _ = w;

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
        self.render_list_items(&list.items, indent, false, unicode)?;
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
    pub(crate) fn render_ordered_list(&mut self, list: &OrderedList) -> Result<(), Error> {
        let indent = self.processor.list_indent.get();
        self.render_styled_title(&list.title)?;
        // Only emit a leading newline for top-level lists (not nested ones)
        if indent == 0 {
            let w = self.writer_mut();
            writeln!(w)?;
        }
        let unicode = self.processor.appearance.capabilities.unicode;
        self.render_list_items(&list.items, indent, true, unicode)?;
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
            let w = self.writer_mut();
            writeln!(w)?;
        }

        for (idx, item) in list.items.iter().enumerate() {
            let item_number = idx + 1;
            let mut w = self.writer_mut();
            write!(w, "<{item_number}>")?;
            write!(w, " ")?;
            let _ = w;

            // Render principal text inline
            for node in &item.principal {
                self.visit_inline_node(node)?;
            }

            w = self.writer_mut();
            writeln!(w)?;

            // Render attached blocks with indentation
            for block in &item.blocks {
                let w = self.writer_mut();
                write!(w, "  ")?;
                let _ = w;
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
        self.render_styled_title(&list.title)?;
        let w = self.writer_mut();
        writeln!(w)?;
        let _ = w;

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
        // Render term in bold
        let processor = self.processor.clone();
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor);
        for node in &item.term {
            temp_visitor.visit_inline_node(node)?;
        }
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;

        let mut w = self.writer_mut();
        w.queue(PrintStyledContent(
            String::from_utf8_lossy(&buffer).to_string().bold(),
        ))?;
        writeln!(w)?;

        // Render principal text with indentation if present
        if !item.principal_text.is_empty() {
            write!(w, "  ")?;
            let _ = w;
            for node in &item.principal_text {
                self.visit_inline_node(node)?;
            }
            w = self.writer_mut();
            writeln!(w)?;
        }
        let _ = w;

        // Render description blocks (without indentation as block.render handles formatting)
        for block in &item.description {
            self.visit_block(block)?;
        }

        Ok(())
    }

    /// Renders a single description list item in horizontal style.
    ///
    /// Term and definition are on the same line, separated by `::`.
    fn render_horizontal_description_list_item(
        &mut self,
        item: &DescriptionListItem,
    ) -> Result<(), Error> {
        // Render term in bold
        let processor = self.processor.clone();
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor);
        for node in &item.term {
            temp_visitor.visit_inline_node(node)?;
        }
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;

        let mut w = self.writer_mut();
        w.queue(PrintStyledContent(
            String::from_utf8_lossy(&buffer).to_string().bold(),
        ))?;

        // Same line: term :: definition
        if !item.principal_text.is_empty() {
            write!(w, " :: ")?;
            let _ = w;
            for node in &item.principal_text {
                self.visit_inline_node(node)?;
            }
            w = self.writer_mut();
        }
        writeln!(w)?;
        let _ = w;

        // Render description blocks indented
        for block in &item.description {
            let w = self.writer_mut();
            write!(w, "  ")?;
            let _ = w;
            self.visit_block(block)?;
        }

        Ok(())
    }

    /// Renders a single description list item in Q&A style.
    ///
    /// Terms are prefixed with "Q: " and definitions with "A: ".
    fn render_qanda_description_list_item(
        &mut self,
        item: &DescriptionListItem,
    ) -> Result<(), Error> {
        // Render "Q: " prefix + term in bold
        let processor = self.processor.clone();
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor);
        for node in &item.term {
            temp_visitor.visit_inline_node(node)?;
        }
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;

        let mut w = self.writer_mut();
        w.queue(PrintStyledContent("Q: ".bold()))?;
        w.queue(PrintStyledContent(
            String::from_utf8_lossy(&buffer).to_string().bold(),
        ))?;
        writeln!(w)?;

        // Render "A: " prefix + principal text
        if !item.principal_text.is_empty() {
            w.queue(PrintStyledContent("A: ".dim()))?;
            let _ = w;
            for node in &item.principal_text {
                self.visit_inline_node(node)?;
            }
            w = self.writer_mut();
            writeln!(w)?;
        }
        let _ = w;

        // Render description blocks indented
        for block in &item.description {
            let w = self.writer_mut();
            write!(w, "   ")?;
            let _ = w;
            self.visit_block(block)?;
        }

        Ok(())
    }
}
