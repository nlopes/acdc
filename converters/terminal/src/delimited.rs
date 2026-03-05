use acdc_converters_core::{
    code::detect_language,
    visitor::{Visitor, WritableVisitor, WritableVisitorExt},
};
use acdc_parser::{
    AttributeValue, Block, BlockMetadata, DelimitedBlock, DelimitedBlockType, InlineNode,
};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::wrap::{pad_to_width, wrap_ansi_text};
use crate::{Error, Processor};

struct BoxChars {
    tl: &'static str,
    tr: &'static str,
    bl: &'static str,
    br: &'static str,
    horiz: &'static str,
    vert: &'static str,
}

const ROUNDED_BOX: BoxChars = BoxChars {
    tl: "╭",
    tr: "╮",
    bl: "╰",
    br: "╯",
    horiz: "─",
    vert: "│",
};

const SQUARE_BOX: BoxChars = BoxChars {
    tl: "┌",
    tr: "┐",
    bl: "└",
    br: "┘",
    horiz: "─",
    vert: "│",
};

/// Render content inside a box with specified corner/border characters.
fn render_boxed_content<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    label: &str,
    content: &str,
    terminal_width: usize,
    chars: &BoxChars,
    color: crossterm::style::Color,
) -> Result<(), Error> {
    let inner_width = terminal_width.saturating_sub(4); // 2 for border + 2 for padding
    let horiz = chars.horiz;

    // Top border with label
    let w = visitor.writer_mut();
    let label_part = if label.is_empty() {
        horiz.repeat(inner_width + 2)
    } else {
        let label_len = label.len() + 3; // "─ label "
        let remaining = (inner_width + 2).saturating_sub(label_len);
        format!("{horiz} {label} {}", horiz.repeat(remaining))
    };
    w.queue(PrintStyledContent(
        format!("{}{label_part}{}", chars.tl, chars.tr).with(color),
    ))?;
    writeln!(w)?;

    // Word-wrap content to fit inside the box, then render each line
    let wrapped = wrap_ansi_text(content, inner_width);
    for line in wrapped.lines() {
        let padded = pad_to_width(line, inner_width);
        w.queue(PrintStyledContent(format!("{} ", chars.vert).with(color)))?;
        write!(w, "{padded}")?;
        w.queue(PrintStyledContent(format!(" {}", chars.vert).with(color)))?;
        writeln!(w)?;
    }

    // Bottom border
    w.queue(PrintStyledContent(
        format!("{}{}{}", chars.bl, horiz.repeat(inner_width + 2), chars.br).with(color),
    ))?;
    writeln!(w)?;

    Ok(())
}

/// Visit a delimited block in terminal format.
pub(crate) fn visit_delimited_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    block: &DelimitedBlock,
    processor: &Processor,
) -> Result<(), Error> {
    match &block.inner {
        DelimitedBlockType::DelimitedTable(t) => {
            render_title_if_present(visitor, &block.title)?;
            crate::table::visit_table(t, visitor, processor)
        }
        DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines) => {
            render_preformatted_block(visitor, &block.title, inlines, &block.metadata, processor)
        }
        DelimitedBlockType::DelimitedExample(blocks) => {
            render_example_block(visitor, &block.title, blocks, processor)
        }
        DelimitedBlockType::DelimitedQuote(blocks) => {
            render_quote_block(visitor, &block.title, blocks, processor)
        }
        DelimitedBlockType::DelimitedSidebar(blocks) => {
            render_sidebar_block(visitor, &block.title, blocks, processor)
        }
        DelimitedBlockType::DelimitedOpen(blocks) => {
            // Open blocks are transparent containers
            render_title_if_present(visitor, &block.title)?;
            for nested_block in blocks {
                visitor.visit_block(nested_block)?;
            }
            Ok(())
        }
        DelimitedBlockType::DelimitedVerse(inlines) => {
            render_verse_block(visitor, &block.title, inlines, processor)
        }
        DelimitedBlockType::DelimitedPass(inlines) => {
            // Passthrough content is rendered as-is
            render_title_if_present(visitor, &block.title)?;
            visitor.visit_inline_nodes(inlines)?;
            let w = visitor.writer_mut();
            writeln!(w)?;
            Ok(())
        }
        DelimitedBlockType::DelimitedStem(stem) => render_stem_block(
            visitor,
            &block.title,
            &stem.notation.to_string(),
            &stem.content,
            processor,
        ),
        DelimitedBlockType::DelimitedComment(_) => {
            // Comments are not rendered
            Ok(())
        }
        _ => {
            // Handle any future block types
            tracing::warn!(?block.inner, "Unknown delimited block type");
            Ok(())
        }
    }
}

/// Render a preformatted block (listing or literal) with optional syntax highlighting.
fn render_preformatted_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    inlines: &[InlineNode],
    metadata: &BlockMetadata,
    processor: &Processor,
) -> Result<(), Error> {
    use std::io::BufWriter;

    // Detect language for syntax highlighting
    let language = detect_language(metadata);

    // Title if present
    if !title.is_empty() {
        visitor.render_title_with_wrapper(title, "\n", "\n")?;
    }

    let tw = processor.terminal_width;
    let color = processor.appearance.colors.label_listing;

    // Top separator with optional language label
    let top_sep = if let Some(lang) = language {
        let label = format!("[ {lang} ]");
        let half = tw.saturating_sub(label.len()) / 2;
        format!(
            "{}{label}{}",
            "─".repeat(half),
            "─".repeat(tw.saturating_sub(half + label.len()))
        )
    } else {
        "─".repeat(tw)
    };
    let w = visitor.writer_mut();
    writeln!(w, "{}", top_sep.clone().with(color))?;

    // Render code content to buffer
    let buffer = Vec::new();
    let inner = BufWriter::new(buffer);
    let mut code_buffer = inner;

    if let Some(lang) = language {
        crate::syntax::highlight_code(&mut code_buffer, inlines, lang, processor)?;
    } else {
        // Fallback to plain text
        use std::io::Write;
        for node in inlines {
            match node {
                InlineNode::VerbatimText(v) => {
                    write!(code_buffer, "{}", v.content)?;
                }
                InlineNode::RawText(r) => {
                    write!(code_buffer, "{}", r.content)?;
                }
                InlineNode::PlainText(p) => {
                    write!(code_buffer, "{}", p.content)?;
                }
                InlineNode::LineBreak(_) => {
                    writeln!(code_buffer)?;
                }
                InlineNode::CalloutRef(callout) => {
                    write!(code_buffer, "<{}>", callout.number)?;
                }
                InlineNode::BoldText(_)
                | InlineNode::ItalicText(_)
                | InlineNode::HighlightText(_)
                | InlineNode::MonospaceText(_)
                | InlineNode::SuperscriptText(_)
                | InlineNode::SubscriptText(_)
                | InlineNode::CurvedQuotationText(_)
                | InlineNode::CurvedApostropheText(_)
                | InlineNode::StandaloneCurvedApostrophe(_)
                | InlineNode::InlineAnchor(_)
                | InlineNode::Macro(_)
                | _ => {}
            }
        }
    }

    let buffer = code_buffer
        .into_inner()
        .map_err(std::io::IntoInnerError::into_error)?;

    // Render code content directly (no left border)
    let content = String::from_utf8_lossy(&buffer);
    let w = visitor.writer_mut();
    write!(w, "{content}")?;
    if !content.ends_with('\n') {
        writeln!(w)?;
    }

    // Bottom separator
    writeln!(w, "{}", "─".repeat(tw).with(color))?;

    Ok(())
}

/// Render an example block with box borders.
fn render_example_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    blocks: &[Block],
    processor: &Processor,
) -> Result<(), Error> {
    use std::io::BufWriter;

    let caption = processor
        .document_attributes
        .get("example-caption")
        .and_then(|v| match v {
            AttributeValue::String(s) => Some(s.clone()),
            AttributeValue::Bool(_) | AttributeValue::None | _ => None,
        })
        .unwrap_or_else(|| "Example".to_string());

    // Build label
    let label = if title.is_empty() {
        caption
    } else {
        let count = processor.example_counter.get() + 1;
        processor.example_counter.set(count);

        let title_text = extract_inline_text(title);
        format!("{caption} {count}. {title_text}")
    };

    // Render content to buffer
    let buffer = Vec::new();
    let inner = BufWriter::new(buffer);
    let mut temp_visitor = crate::TerminalVisitor::new(inner, processor.clone());
    for nested_block in blocks {
        temp_visitor.visit_block(nested_block)?;
    }
    let buffer = temp_visitor
        .into_writer()
        .into_inner()
        .map_err(std::io::IntoInnerError::into_error)?;
    let content = String::from_utf8_lossy(&buffer);

    let color = crossterm::style::Color::Cyan;
    render_boxed_content(
        visitor,
        &label,
        content.trim_end(),
        processor.terminal_width,
        &SQUARE_BOX,
        color,
    )?;

    Ok(())
}

/// Render a quote block with `│` left border.
fn render_quote_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    blocks: &[Block],
    processor: &Processor,
) -> Result<(), Error> {
    use std::io::BufWriter;

    // Render title if present
    visitor.render_title_with_wrapper(title, "", "\n")?;

    // Render content to temporary buffer
    let buffer = Vec::new();
    let inner = BufWriter::new(buffer);
    let mut temp_visitor = crate::TerminalVisitor::new(inner, processor.clone());

    for nested_block in blocks {
        temp_visitor.visit_block(nested_block)?;
    }

    let buffer = temp_visitor
        .into_writer()
        .into_inner()
        .map_err(std::io::IntoInnerError::into_error)?;

    let content = String::from_utf8_lossy(&buffer);
    let color = processor.appearance.colors.admon_tip; // Green for quotes

    // Word-wrap content to fit within the "│ " prefix
    let available = processor.terminal_width.saturating_sub(2);
    let wrapped = wrap_ansi_text(&content, available);

    // Left border with `│` on each line, content in italic
    let w = visitor.writer_mut();
    for line in wrapped.lines() {
        w.queue(PrintStyledContent("│ ".with(color)))?;
        let styled_line = line.italic();
        QueueableCommand::queue(w, PrintStyledContent(styled_line))?;
        writeln!(w)?;
    }

    // Empty closing border line
    if !content.is_empty() {
        w.queue(PrintStyledContent("│".with(color)))?;
        writeln!(w)?;
    }

    Ok(())
}

/// Render a sidebar block with rounded box borders.
fn render_sidebar_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    blocks: &[Block],
    processor: &Processor,
) -> Result<(), Error> {
    use std::io::BufWriter;

    let label = if title.is_empty() {
        String::new()
    } else {
        extract_inline_text(title)
    };

    // Render content to buffer
    let buffer = Vec::new();
    let inner = BufWriter::new(buffer);
    let mut temp_visitor = crate::TerminalVisitor::new(inner, processor.clone());
    for nested_block in blocks {
        temp_visitor.visit_block(nested_block)?;
    }
    let buffer = temp_visitor
        .into_writer()
        .into_inner()
        .map_err(std::io::IntoInnerError::into_error)?;
    let content = String::from_utf8_lossy(&buffer);

    let color = crossterm::style::Color::Blue;
    render_boxed_content(
        visitor,
        &label,
        content.trim_end(),
        processor.terminal_width,
        &ROUNDED_BOX,
        color,
    )?;

    Ok(())
}

/// Render a verse block (poetry) with `┊` left border preserving line breaks.
fn render_verse_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    inlines: &[InlineNode],
    processor: &Processor,
) -> Result<(), Error> {
    use std::io::BufWriter;

    visitor.render_title_with_wrapper(title, "", "\n")?;

    // Render verse content to buffer to process line by line
    let buffer = Vec::new();
    let inner = BufWriter::new(buffer);
    let mut temp_visitor = crate::TerminalVisitor::new(inner, processor.clone());
    temp_visitor.visit_inline_nodes(inlines)?;
    let buffer = temp_visitor
        .into_writer()
        .into_inner()
        .map_err(std::io::IntoInnerError::into_error)?;

    let content = String::from_utf8_lossy(&buffer);
    let color = crossterm::style::Color::Magenta;

    let w = visitor.writer_mut();
    for line in content.lines() {
        w.queue(PrintStyledContent("┊ ".with(color)))?;
        write!(w, "{line}")?;
        writeln!(w)?;
    }
    // Closing border
    w.queue(PrintStyledContent("┊".with(color)))?;
    writeln!(w)?;

    Ok(())
}

/// Render a STEM/math block with styled borders.
fn render_stem_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    notation: &str,
    content: &str,
    processor: &Processor,
) -> Result<(), Error> {
    render_title_if_present(visitor, title)?;

    let tw = processor.terminal_width;
    let color = processor.appearance.colors.label_listing;

    // Top separator with notation label
    let label = format!(" {notation} ");
    let half = tw.saturating_sub(label.len()) / 2;
    let top = format!(
        "{}{}{}",
        "─".repeat(half),
        label,
        "─".repeat(tw.saturating_sub(half + label.len()))
    );

    let w = visitor.writer_mut();
    writeln!(w, "{}", top.with(color))?;
    writeln!(w, "{content}")?;
    writeln!(w, "{}", "─".repeat(tw).with(color))?;
    Ok(())
}

/// Helper to render title if present.
fn render_title_if_present<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
) -> Result<(), Error> {
    visitor.render_title_with_wrapper(title, "  ", "\n")
}

/// Extract plain text from inline nodes (for labels/titles).
fn extract_inline_text(nodes: &[InlineNode]) -> String {
    nodes
        .iter()
        .map(|node| match node {
            InlineNode::PlainText(p) => p.content.clone(),
            InlineNode::BoldText(b) => extract_inline_text(&b.content),
            InlineNode::ItalicText(i) => extract_inline_text(&i.content),
            InlineNode::MonospaceText(m) => extract_inline_text(&m.content),
            InlineNode::HighlightText(h) => extract_inline_text(&h.content),
            InlineNode::SuperscriptText(_)
            | InlineNode::SubscriptText(_)
            | InlineNode::CurvedQuotationText(_)
            | InlineNode::CurvedApostropheText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::VerbatimText(_)
            | InlineNode::RawText(_)
            | InlineNode::LineBreak(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::Macro(_)
            | InlineNode::CalloutRef(_)
            | _ => String::new(),
        })
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Options, TerminalVisitor};
    use acdc_converters_core::visitor::Visitor;
    use acdc_parser::{DocumentAttributes, Location, Paragraph, Plain, Title};

    /// Create simple plain text inline nodes for testing
    fn create_test_inlines(content: &str) -> Vec<InlineNode> {
        vec![InlineNode::PlainText(Plain {
            content: content.to_string(),
            location: Location::default(),
            escaped: false,
        })]
    }

    /// Create simple plain text title for testing
    fn create_test_title(content: &str) -> Title {
        Title::new(vec![InlineNode::PlainText(Plain {
            content: content.to_string(),
            location: Location::default(),
            escaped: false,
        })])
    }

    /// Create test processor with default options
    fn create_test_processor() -> Processor {
        use crate::Appearance;
        use acdc_converters_core::section::{
            AppendixTracker, PartNumberTracker, SectionNumberTracker,
        };
        use std::{cell::Cell, rc::Rc};
        let options = Options::default();
        let document_attributes = DocumentAttributes::default();
        let appearance = Appearance::detect();
        let section_number_tracker = SectionNumberTracker::new(&document_attributes);
        let part_number_tracker =
            PartNumberTracker::new(&document_attributes, section_number_tracker.clone());
        let appendix_tracker =
            AppendixTracker::new(&document_attributes, section_number_tracker.clone());
        Processor {
            options,
            document_attributes,
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
            appearance,
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            terminal_width: crate::FALLBACK_TERMINAL_WIDTH,
            index_entries: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            has_valid_index_section: false,
            list_indent: std::rc::Rc::new(std::cell::Cell::new(0)),
        }
    }

    #[test]
    fn test_listing_block_basic() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code content here")),
            "----".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("────"),
            "Should have horizontal separators"
        );
        assert!(
            output_str.contains("code content here"),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_listing_block_with_title() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            "----".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("My Code Listing"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("My Code Listing"),
            "Should contain title"
        );
        assert!(output_str.contains("code here"), "Should contain content");
        assert!(
            output_str.contains("────"),
            "Should have horizontal separators"
        );

        Ok(())
    }

    #[test]
    fn test_literal_block_basic() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedLiteral(create_test_inlines("literal text")),
            "....".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("────"),
            "Should have horizontal separators"
        );
        assert!(
            output_str.contains("literal text"),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_literal_block_with_title() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedLiteral(create_test_inlines("literal content")),
            "....".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("Literal Block Title"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Literal Block Title"),
            "Should contain title"
        );
        assert!(
            output_str.contains("literal content"),
            "Should contain content"
        );
        assert!(
            output_str.contains("────"),
            "Should have horizontal separators"
        );

        Ok(())
    }

    #[test]
    fn test_example_block_basic() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("example text"),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(content),
            "====".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Example"), "Should have Example label");
        assert!(output_str.contains("┌"), "Should have box top border");
        assert!(output_str.contains("└"), "Should have box bottom border");
        assert!(
            output_str.contains("example text"),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_example_block_with_title() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("example content"),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(content),
            "====".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("Custom Example Title"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Example"), "Should have Example label");
        assert!(
            output_str.contains("Custom Example Title"),
            "Should contain custom title"
        );
        assert!(
            output_str.contains("example content"),
            "Should contain content"
        );
        assert!(output_str.contains("┌"), "Should have box border");

        Ok(())
    }

    #[test]
    fn test_quote_block_basic() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("This is a quote."),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedQuote(content),
            "____".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("This is a quote."),
            "Should contain content"
        );
        assert!(output_str.contains("│"), "Should have left border");

        Ok(())
    }

    #[test]
    fn test_quote_block_with_title() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("Quote content here."),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedQuote(content),
            "____".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("Quote Title"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Quote Title"), "Should contain title");
        assert!(
            output_str.contains("Quote content here."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_quote_block_multiple_paragraphs() -> Result<(), Error> {
        let content = vec![
            Block::Paragraph(Paragraph::new(
                create_test_inlines("First paragraph."),
                Location::default(),
            )),
            Block::Paragraph(Paragraph::new(
                create_test_inlines("Second paragraph."),
                Location::default(),
            )),
        ];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedQuote(content),
            "____".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("First paragraph."),
            "Should contain first paragraph"
        );
        assert!(
            output_str.contains("Second paragraph."),
            "Should contain second paragraph"
        );

        Ok(())
    }

    #[test]
    fn test_sidebar_block_basic() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("Sidebar content."),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedSidebar(content),
            "****".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("╭"), "Should have rounded top border");
        assert!(
            output_str.contains("╰"),
            "Should have rounded bottom border"
        );
        assert!(
            output_str.contains("Sidebar content."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_sidebar_block_with_title() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("Sidebar text here."),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedSidebar(content),
            "****".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("Sidebar Title"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("╭"), "Should have rounded border");
        assert!(output_str.contains("Sidebar Title"), "Should contain title");
        assert!(
            output_str.contains("Sidebar text here."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_sidebar_block_multiple_paragraphs() -> Result<(), Error> {
        let content = vec![
            Block::Paragraph(Paragraph::new(
                create_test_inlines("First sidebar paragraph."),
                Location::default(),
            )),
            Block::Paragraph(Paragraph::new(
                create_test_inlines("Second sidebar paragraph."),
                Location::default(),
            )),
        ];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedSidebar(content),
            "****".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("First sidebar paragraph."),
            "Should contain first paragraph"
        );
        assert!(
            output_str.contains("Second sidebar paragraph."),
            "Should contain second paragraph"
        );

        Ok(())
    }

    #[test]
    fn test_open_block_basic() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("Open block content."),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedOpen(content),
            "--".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Open block content."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_open_block_with_title() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("Content here."),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedOpen(content),
            "--".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("Open Block Title"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Open Block Title"),
            "Should contain title"
        );
        assert!(
            output_str.contains("Content here."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_verse_block_basic() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedVerse(create_test_inlines(
                "Roses are red\nViolets are blue",
            )),
            "____".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Roses are red"),
            "Should contain verse content"
        );
        assert!(output_str.contains("┊"), "Should have dotted left border");

        Ok(())
    }

    #[test]
    fn test_verse_block_with_title() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedVerse(create_test_inlines("Poetry line 1\nPoetry line 2")),
            "____".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("Poem Title"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Poem Title"), "Should contain title");
        assert!(
            output_str.contains("Poetry line 1"),
            "Should contain verse content"
        );

        Ok(())
    }

    #[test]
    fn test_pass_block_basic() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedPass(create_test_inlines("<raw>passthrough</raw>")),
            "++++".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("<raw>passthrough</raw>"),
            "Should contain passthrough content as-is"
        );

        Ok(())
    }

    #[test]
    fn test_stem_block_placeholder() -> Result<(), Error> {
        use acdc_parser::{StemContent, StemNotation};

        let stem_content = StemContent::new("x = y^2".to_string(), StemNotation::Latexmath);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedStem(stem_content),
            "++++".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("latexmath"),
            "Should show notation type"
        );
        assert!(output_str.contains("x = y^2"), "Should show STEM content");
        assert!(output_str.contains("───"), "Should have styled borders");

        Ok(())
    }

    #[test]
    fn test_comment_block_not_rendered() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedComment(create_test_inlines("This is a comment")),
            "////".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            !output_str.contains("This is a comment"),
            "Comment content should not be rendered"
        );

        Ok(())
    }

    #[test]
    fn test_empty_listing_block() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(Vec::new()),
            "----".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("────"),
            "Should have horizontal separators even when empty"
        );

        Ok(())
    }

    #[test]
    fn test_empty_quote_block() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedQuote(Vec::new()),
            "____".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        // Empty quote should produce empty or whitespace output
        assert!(
            output_str.is_empty() || output_str.trim().is_empty(),
            "Empty quote block should produce empty or whitespace output"
        );

        Ok(())
    }

    #[test]
    fn test_listing_with_special_characters() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines(
                "<html>&amp; special chars \"quotes\" 'apostrophes'",
            )),
            "----".to_string(),
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("<html>&amp; special chars"),
            "Should preserve special characters"
        );

        Ok(())
    }

    #[test]
    fn test_nested_example_with_listing() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("This example shows: code snippet"),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(content),
            "====".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("Nested Content"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Nested Content"),
            "Should contain title"
        );
        assert!(
            output_str.contains("This example shows: code snippet"),
            "Should contain nested content"
        );

        Ok(())
    }

    #[test]
    fn test_example_block_numbering_sequence() -> Result<(), Error> {
        let processor = create_test_processor();

        let block1 = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![Block::Paragraph(Paragraph::new(
                create_test_inlines("first example"),
                Location::default(),
            ))]),
            "====".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("First Example"));

        let block2 = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![Block::Paragraph(Paragraph::new(
                create_test_inlines("second example"),
                Location::default(),
            ))]),
            "====".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("Second Example"));

        let block3 = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![Block::Paragraph(Paragraph::new(
                create_test_inlines("third example"),
                Location::default(),
            ))]),
            "====".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("Third Example"));

        let mut buffer1 = Vec::new();
        let mut visitor1 = TerminalVisitor::new(&mut buffer1, processor.clone());
        visitor1.visit_delimited_block(&block1)?;

        let mut buffer2 = Vec::new();
        let mut visitor2 = TerminalVisitor::new(&mut buffer2, processor.clone());
        visitor2.visit_delimited_block(&block2)?;

        let mut buffer3 = Vec::new();
        let mut visitor3 = TerminalVisitor::new(&mut buffer3, processor.clone());
        visitor3.visit_delimited_block(&block3)?;

        let output1 = String::from_utf8_lossy(&buffer1);
        let output2 = String::from_utf8_lossy(&buffer2);
        let output3 = String::from_utf8_lossy(&buffer3);

        assert!(
            output1.contains("Example 1."),
            "First example should have number 1, got: {output1}"
        );
        assert!(
            output2.contains("Example 2."),
            "Second example should have number 2, got: {output2}"
        );
        assert!(
            output3.contains("Example 3."),
            "Third example should have number 3, got: {output3}"
        );

        Ok(())
    }
}
