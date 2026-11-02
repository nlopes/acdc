use std::io::{self, BufWriter, Write};

use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use acdc_converters_core::{TraversalContext, list::OrderedListNumbering};
use acdc_parser::{
    ListItemCheckedStatus,
    ast::{
        BlockKind, BlockRef, CalloutListRef, ContentRef, DescriptionListItemRef,
        DescriptionListRef, InlineKind, InlineRef, ListItemRef, OrderedListRef, UnorderedListRef,
    },
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
    Bullet(UnorderedMarker),
    Numbered(OrderedListNumbering),
    Hidden,
}

#[derive(Clone, Copy, Debug)]
enum UnorderedMarker {
    Default,
    Disc,
    Circle,
    Square,
}

impl UnorderedMarker {
    const fn text(self, unicode: bool) -> &'static str {
        match (self, unicode) {
            (Self::Default | Self::Disc, true) => "•",
            (Self::Circle, true) => "◦",
            (Self::Square, true) => "▪",
            (Self::Default | Self::Disc, false) => "*",
            (Self::Circle, false) => "o",
            (Self::Square, false) => "+",
        }
    }
}

fn style_suppresses_marker(style: Option<&str>) -> bool {
    matches!(style, Some("none" | "no-bullet" | "unstyled"))
}

impl<'a, W: Write> TerminalVisitor<'a, '_, W> {
    /// Render a title with italic styling.
    #[tracing::instrument(skip(self, traversal))]
    fn render_styled_title(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
    ) -> Result<(), Error> {
        if let Some(title) = title {
            let processor = self.processor;
            let buffer = Vec::new();
            let inner = BufWriter::new(buffer);
            let mut temp_visitor =
                TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());
            for node in title.inlines() {
                temp_visitor.write_inline(traversal, node)?;
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
    #[tracing::instrument(skip(self, traversal, items))]
    fn render_list_items(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        items: impl Iterator<Item = ListItemRef<'a>>,
        indent: usize,
        marker: ListMarker,
        start: usize,
        reversed: bool,
    ) -> Result<(), Error> {
        let unicode = self.processor.appearance.capabilities.unicode;
        for (idx, item) in items.enumerate() {
            let number = if reversed {
                start.saturating_sub(idx)
            } else {
                start.saturating_add(idx)
            };
            self.render_list_item(traversal, item, indent, marker, number, unicode)?;
        }
        Ok(())
    }

    /// Render a single list item with the specified indentation.
    ///
    /// After rendering the item's principal text, child blocks (which may include
    /// nested lists) are visited with an increased `list_indent` on the processor.
    #[tracing::instrument(skip(self, traversal))]
    fn render_list_item(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        item: ListItemRef<'a>,
        indent: usize,
        marker: ListMarker,
        item_number: usize,
        unicode: bool,
    ) -> Result<(), Error> {
        write_indent(&mut self.writer, indent)?;

        let marker_written = match marker {
            ListMarker::Bullet(marker) => {
                write!(self.writer, "{}", marker.text(unicode))?;
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

        render_checked_status(item.checked().as_ref(), &mut self.writer, unicode)?;
        if item.checked().is_some() {
            write!(self.writer, " ")?;
        }

        for node in item.principal().inlines() {
            self.write_inline(traversal, node)?;
        }

        writeln!(self.writer)?;

        // Render attached blocks with increased indentation.
        // Set list_indent so nested lists rendered via the visitor pick up the right depth.
        let nested_indent = indent + 2;
        let old_indent = self.processor.list_indent.get();
        self.processor.list_indent.set(nested_indent);
        for block in item.blocks() {
            traversal.visit_block(self, block)?;
        }
        self.processor.list_indent.set(old_indent);

        Ok(())
    }

    pub(crate) fn render_unordered_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: UnorderedListRef<'a>,
    ) -> Result<(), Error> {
        if list.style() == Some("bibliography") {
            return self.render_bibliography_list(traversal, list);
        }

        let indent = self.processor.list_indent.get();
        self.render_styled_title(traversal, list.title())?;
        // Only emit a leading newline for top-level lists (not nested ones)
        if indent == 0 {
            let w = self.writer_mut();
            writeln!(w)?;
        }
        let marker = if style_suppresses_marker(list.style()) {
            ListMarker::Hidden
        } else {
            ListMarker::Bullet(match list.style() {
                Some("disc") => UnorderedMarker::Disc,
                Some("circle") => UnorderedMarker::Circle,
                Some("square") => UnorderedMarker::Square,
                Some(_) | None => UnorderedMarker::Default,
            })
        };
        self.render_list_items(traversal, list.items(), indent, marker, 1, false)?;
        Ok(())
    }

    fn render_bibliography_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: UnorderedListRef<'a>,
    ) -> Result<(), Error> {
        let indent = self.processor.list_indent.get();
        self.render_styled_title(traversal, list.title())?;
        if indent == 0 {
            writeln!(self.writer)?;
        }
        for item in list.items() {
            write_indent(&mut self.writer, indent)?;
            let mut content = item.principal().inlines();
            if let Some(InlineKind::Anchor(anchor)) = content.clone().next().map(InlineRef::kind)
                && anchor.is_bibliography()
            {
                content.next();
                if let Some(label) = anchor.bibliography_label() {
                    write!(self.writer, "[")?;
                    self.write_content(traversal, label)?;
                    write!(self.writer, "]")?;
                } else {
                    self.writer
                        .queue(PrintStyledContent(format!("[{}]", anchor.id()).bold()))?;
                }
            }
            self.write_inlines(traversal, content)?;
            writeln!(self.writer)?;
            let previous = self.processor.list_indent.replace(indent + 2);
            for block in item.blocks() {
                traversal.visit_block(self, block)?;
            }
            self.processor.list_indent.set(previous);
        }
        Ok(())
    }

    /// Renders an ordered list in terminal format.
    ///
    /// Items are numbered from 1 by default. `start` and `%reversed` change the
    /// sequence, and nested lists start a new sequence. Markerless styles omit
    /// the item markers.
    ///
    /// # Format
    /// ```text
    /// 1. First item
    /// 2. Second item
    ///    1. Nested item
    /// 3. Third item
    /// ```
    pub(crate) fn render_ordered_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: OrderedListRef<'a>,
    ) -> Result<(), Error> {
        let indent = self.processor.list_indent.get();
        self.render_styled_title(traversal, list.title())?;
        // Only emit a leading newline for top-level lists (not nested ones)
        if indent == 0 {
            let w = self.writer_mut();
            writeln!(w)?;
        }
        let marker = if style_suppresses_marker(list.style()) || list.style() == Some("unnumbered")
        {
            ListMarker::Hidden
        } else {
            let numbering = list
                .style()
                .and_then(OrderedListNumbering::from_explicit_style)
                .unwrap_or_default();
            ListMarker::Numbered(numbering)
        };
        let reversed = list.options().iter().any(|value| value == "reversed");
        let start = super::terminal_visitor::attribute_text(list.attributes(), "start")
            .and_then(|start| start.parse::<usize>().ok())
            .filter(|start| *start > 0)
            .unwrap_or(if reversed { list.items().count() } else { 1 });
        self.render_list_items(traversal, list.items(), indent, marker, start, reversed)?;
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
    pub(crate) fn render_callout_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: CalloutListRef<'a>,
    ) -> Result<(), Error> {
        self.render_styled_title(traversal, list.title())?;
        if list.title().is_some() {
            writeln!(self.writer)?;
        }

        for item in list.items() {
            let item_number = item.callout().number();
            write!(self.writer, "<{item_number}>")?;
            write!(self.writer, " ")?;

            // Render principal text inline
            for node in item.principal().inlines() {
                self.write_inline(traversal, node)?;
            }

            writeln!(self.writer)?;

            // Render attached blocks with indentation
            for block in item.blocks() {
                write!(self.writer, "  ")?;
                traversal.visit_block(self, block)?;
            }
        }
        Ok(())
    }

    /// Renders a description list in terminal format.
    ///
    /// Supports five styles:
    /// - **default**: Terms in bold on one line, definitions indented on next line
    /// - **horizontal**: Terms and definitions on same line separated by `::`
    /// - **qanda**: Terms prefixed with "Q: ", definitions with "A: "
    /// - **ordered**: Terms use numbered markers
    /// - **unordered**: Terms use bullet markers
    pub(crate) fn render_description_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: DescriptionListRef<'a>,
    ) -> Result<(), Error> {
        let indent = self.processor.list_indent.get();
        self.render_styled_title(traversal, list.title())?;
        if indent == 0 || list.title().is_some() {
            writeln!(self.writer)?;
        }

        let style = list.style();

        for (index, item) in list.items().enumerate() {
            match style {
                Some("horizontal") => {
                    self.render_horizontal_description_list_item(traversal, item)?;
                }
                Some("qanda") => {
                    self.render_qanda_description_list_item(traversal, item)?;
                }
                Some("ordered") => {
                    self.render_description_list_item(
                        traversal,
                        item,
                        Some(&format!("{}.", index + 1)),
                    )?;
                }
                Some("unordered") => {
                    let marker = if self.processor.appearance.capabilities.unicode {
                        "•"
                    } else {
                        "*"
                    };
                    self.render_description_list_item(traversal, item, Some(marker))?;
                }
                _ => {
                    self.render_description_list_item(traversal, item, None)?;
                }
            }
        }
        Ok(())
    }

    /// Renders a single description list item (term and definition) in default style.
    ///
    /// The term is rendered in bold, followed by the principal text (if present)
    /// indented with 2 spaces, and any additional description blocks.
    fn render_description_list_item(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        item: DescriptionListItemRef<'a>,
        marker: Option<&str>,
    ) -> Result<(), Error> {
        let indent = self.processor.list_indent.get();
        // Render term in bold
        let processor = self.processor;
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());
        for node in item.term().inlines() {
            temp_visitor.write_inline(traversal, node)?;
        }
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;

        write_indent(&mut self.writer, indent)?;
        if let Some(marker) = marker {
            write!(self.writer, "{marker} ")?;
        }
        self.writer.queue(PrintStyledContent(
            String::from_utf8_lossy(&buffer).to_string().bold(),
        ))?;
        writeln!(self.writer)?;

        // Render principal text with indentation if present
        if !item.principal().is_empty() {
            write_indent(&mut self.writer, indent + 2)?;
            for node in item.principal().inlines() {
                self.write_inline(traversal, node)?;
            }
            writeln!(self.writer)?;
        }

        self.render_description_blocks(traversal, item.blocks(), indent + 2)
    }

    /// Renders a single description list item in horizontal style.
    ///
    /// Term and definition are on the same line, separated by `::`.
    fn render_horizontal_description_list_item(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        item: DescriptionListItemRef<'a>,
    ) -> Result<(), Error> {
        let indent = self.processor.list_indent.get();
        // Render term in bold
        let processor = self.processor;
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());
        for node in item.term().inlines() {
            temp_visitor.write_inline(traversal, node)?;
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
        if !item.principal().is_empty() {
            write!(self.writer, " :: ")?;
            for node in item.principal().inlines() {
                self.write_inline(traversal, node)?;
            }
        }
        writeln!(self.writer)?;

        // Render description blocks indented
        self.render_description_blocks(traversal, item.blocks(), indent + 2)
    }

    /// Renders a single description list item in Q&A style.
    ///
    /// Terms are prefixed with "Q: " and definitions with "A: ".
    fn render_qanda_description_list_item(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        item: DescriptionListItemRef<'a>,
    ) -> Result<(), Error> {
        let indent = self.processor.list_indent.get();
        // Render "Q: " prefix + term in bold
        let processor = self.processor;
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());
        for node in item.term().inlines() {
            temp_visitor.write_inline(traversal, node)?;
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
        if !item.principal().is_empty() {
            write_indent(&mut self.writer, indent)?;
            self.writer.queue(PrintStyledContent("A: ".dim()))?;
            for node in item.principal().inlines() {
                self.write_inline(traversal, node)?;
            }
            writeln!(self.writer)?;
        }

        // Render description blocks indented
        self.render_description_blocks(traversal, item.blocks(), indent + 3)
    }

    fn render_description_blocks(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        mut blocks: impl Iterator<Item = BlockRef<'a>>,
        indent: usize,
    ) -> Result<(), Error> {
        let previous_indent = self.processor.list_indent.replace(indent);
        let result = blocks.try_for_each(|block| {
            if !matches!(
                block.kind(),
                BlockKind::DescriptionList(_)
                    | BlockKind::OrderedList(_)
                    | BlockKind::UnorderedList(_)
            ) {
                write_indent(&mut self.writer, indent)?;
            }
            traversal.visit_block(self, block)
        });
        self.processor.list_indent.set(previous_indent);
        result
    }
}
