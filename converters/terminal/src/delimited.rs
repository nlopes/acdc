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

use crate::{Error, Processor};

/// Visit a delimited block in terminal format.
///
/// Renders different block types with appropriate terminal styling:
/// - Tables: rendered with borders using comfy-table
/// - Listings/Literals: monospace with indentation
/// - Examples: numbered with "Example N." prefix
/// - Quotes: indented with quote styling
/// - Sidebars: boxed content
/// - Open blocks: transparent containers
pub(crate) fn visit_delimited_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    block: &DelimitedBlock,
    processor: &Processor,
) -> Result<(), Error> {
    match &block.inner {
        DelimitedBlockType::DelimitedTable(t) => crate::table::visit_table(t, visitor, processor),
        DelimitedBlockType::DelimitedListing(inlines) => render_preformatted_block(
            visitor,
            &block.title,
            inlines,
            &block.metadata,
            "listing",
            processor,
        ),
        DelimitedBlockType::DelimitedLiteral(inlines) => render_preformatted_block(
            visitor,
            &block.title,
            inlines,
            &block.metadata,
            "literal",
            processor,
        ),
        DelimitedBlockType::DelimitedExample(blocks) => {
            render_example_block(visitor, &block.title, blocks, processor)
        }
        DelimitedBlockType::DelimitedQuote(blocks) => {
            render_quote_block(visitor, &block.title, blocks, processor)
        }
        DelimitedBlockType::DelimitedSidebar(blocks) => {
            render_sidebar_block(visitor, &block.title, blocks)
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
            render_verse_block(visitor, &block.title, inlines)
        }
        DelimitedBlockType::DelimitedPass(inlines) => {
            // Passthrough content is rendered as-is
            render_title_if_present(visitor, &block.title)?;
            visitor.visit_inline_nodes(inlines)?;
            let w = visitor.writer_mut();
            writeln!(w)?;
            Ok(())
        }
        DelimitedBlockType::DelimitedStem(stem) => {
            // STEM/math content - show placeholder in terminal
            render_title_if_present(visitor, &block.title)?;
            let w = visitor.writer_mut();
            writeln!(w, "  [STEM({}): {}]", stem.notation, stem.content)?;
            Ok(())
        }
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
///
/// If the block has `[source,language]` metadata and the language is recognized,
/// syntax highlighting will be applied. Uses simple horizontal separators (mdcat style).
fn render_preformatted_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    inlines: &[InlineNode],
    metadata: &BlockMetadata,
    _block_type: &str,
    processor: &Processor,
) -> Result<(), Error> {
    use std::io::BufWriter;

    // Detect language for syntax highlighting
    let language = detect_language(metadata);

    // Title if present
    if !title.is_empty() {
        visitor.render_title_with_wrapper(title, "\n", "\n")?;
    }

    // Simple top separator (mdcat style)
    let separator = "─".repeat(20);
    let color = processor.appearance.colors.label_listing;
    let w = visitor.writer_mut();
    writeln!(w, "{}", separator.clone().with(color))?;

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
    writeln!(w, "{}", separator.with(color))?;

    Ok(())
}

/// Render an example block with "Example N." numbering.
fn render_example_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    blocks: &[Block],
    processor: &Processor,
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    // Start marker with "EXAMPLE N." label if there's a title
    let caption = processor
        .document_attributes
        .get("example-caption")
        .and_then(|v| match v {
            AttributeValue::String(s) => Some(s.to_uppercase()),
            AttributeValue::Bool(_) | AttributeValue::None | _ => None,
        })
        .unwrap_or_else(|| "EXAMPLE".to_string());

    if title.is_empty() {
        let styled_label = caption.cyan().bold();
        QueueableCommand::queue(w, PrintStyledContent(styled_label))?;
        writeln!(w)?;
    } else {
        let count = processor.example_counter.get() + 1;
        processor.example_counter.set(count);
        let label = format!("{caption} {count}.");
        let styled_label = label.cyan().bold();
        QueueableCommand::queue(w, PrintStyledContent(styled_label))?;
        write!(w, " ")?;
    }

    visitor.render_title_with_wrapper(title, "", "\n\n")?;

    // Render content blocks
    for nested_block in blocks {
        visitor.visit_block(nested_block)?;
    }

    // End marker with three dots
    let w = visitor.writer_mut();
    let end_marker = "• • •".cyan().bold();
    QueueableCommand::queue(w, PrintStyledContent(end_marker))?;
    writeln!(w)?;

    Ok(())
}

/// Render a quote block with quote styling (mdcat style with indentation).
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

    // Add indentation to each line (mdcat style - 4 spaces)
    let w = visitor.writer_mut();
    for line in content.lines() {
        let styled_line = line.italic();
        write!(w, "    ")?; // 4-space indent
        QueueableCommand::queue(w, PrintStyledContent(styled_line))?;
        writeln!(w)?;
    }

    // Add final newline if content didn't end with one
    if !content.ends_with('\n') {
        writeln!(w)?;
    }

    Ok(())
}

/// Render a sidebar block.
fn render_sidebar_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    blocks: &[Block],
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    // Start marker with "SIDEBAR" label
    let styled_label = "SIDEBAR".blue().bold();
    QueueableCommand::queue(w, PrintStyledContent(styled_label))?;
    writeln!(w)?;

    visitor.render_title_with_wrapper(title, "", "\n\n")?;

    // Render content blocks
    for nested_block in blocks {
        visitor.visit_block(nested_block)?;
    }

    // End marker with three dots
    let w = visitor.writer_mut();
    let end_marker = "• • •".blue().bold();
    QueueableCommand::queue(w, PrintStyledContent(end_marker))?;
    writeln!(w)?;

    Ok(())
}

/// Render a verse block (poetry) preserving line breaks.
fn render_verse_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    inlines: &[InlineNode],
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    // Start marker with "VERSE" label
    let styled_label = "VERSE".magenta().bold();
    QueueableCommand::queue(w, PrintStyledContent(styled_label))?;
    writeln!(w)?;

    visitor.render_title_with_wrapper(title, "", "\n\n")?;

    // Render verse content
    visitor.visit_inline_nodes(inlines)?;
    let w = visitor.writer_mut();
    writeln!(w)?;

    // End marker with three dots
    let end_marker = "• • •".magenta().bold();
    QueueableCommand::queue(w, PrintStyledContent(end_marker))?;
    writeln!(w)?;

    Ok(())
}

/// Helper to render title if present.
fn render_title_if_present<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
) -> Result<(), Error> {
    visitor.render_title_with_wrapper(title, "  ", "\n")
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
        })]
    }

    /// Create simple plain text title for testing
    fn create_test_title(content: &str) -> Title {
        Title::new(vec![InlineNode::PlainText(Plain {
            content: content.to_string(),
            location: Location::default(),
        })])
    }

    /// Create test processor with default options
    fn create_test_processor() -> Processor {
        use crate::Appearance;
        use std::{cell::Cell, rc::Rc};
        let options = Options::default();
        let document_attributes = DocumentAttributes::default();
        let appearance = Appearance::detect();
        Processor {
            options,
            document_attributes,
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
            appearance,
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
        // Check for horizontal separators (mdcat style)
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
        assert!(output_str.contains("EXAMPLE"), "Should have EXAMPLE label");
        assert!(output_str.contains("• • •"), "Should have end marker");
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
        assert!(output_str.contains("EXAMPLE"), "Should have EXAMPLE label");
        assert!(
            output_str.contains("Custom Example Title"),
            "Should contain custom title"
        );
        assert!(
            output_str.contains("example content"),
            "Should contain content"
        );
        assert!(output_str.contains("• • •"), "Should have end marker");

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
        // Quotes now use indentation (mdcat style)
        assert!(
            output_str.contains("This is a quote."),
            "Should contain content"
        );

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
        // Quotes now use indentation (mdcat style)
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
        assert!(output_str.contains("SIDEBAR"), "Should have SIDEBAR label");
        assert!(output_str.contains("• • •"), "Should have end marker");
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
        assert!(output_str.contains("SIDEBAR"), "Should have SIDEBAR label");
        assert!(output_str.contains("Sidebar Title"), "Should contain title");
        assert!(output_str.contains("• • •"), "Should have end marker");
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
        // Open blocks are transparent - content rendered without decoration
        assert!(
            output_str.contains("Open block content."),
            "Should contain content"
        );
        // Should not have box borders like listing/literal
        assert!(
            !output_str.contains("┌") && !output_str.contains("╔"),
            "Should not have borders"
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
            output_str.contains("[STEM(latexmath): x = y^2]"),
            "Should show placeholder for STEM content"
        );

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
        // Comments should not render any content
        assert!(
            !output_str.contains("This is a comment"),
            "Comment content should not be rendered"
        );

        Ok(())
    }

    // Edge Case Tests

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
        // Empty blocks should still render with horizontal separators
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
        // Empty quote should render (though may be empty or just whitespace)
        // Just verify it doesn't crash
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
        // Special characters should be preserved in listings
        assert!(
            output_str.contains("<html>&amp; special chars"),
            "Should preserve special characters"
        );

        Ok(())
    }

    #[test]
    fn test_nested_example_with_listing() -> Result<(), Error> {
        // Test an example block containing a paragraph with listing-like content
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

        // Create first example with title
        let block1 = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![Block::Paragraph(Paragraph::new(
                create_test_inlines("first example"),
                Location::default(),
            ))]),
            "====".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("First Example"));

        // Create second example with title
        let block2 = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![Block::Paragraph(Paragraph::new(
                create_test_inlines("second example"),
                Location::default(),
            ))]),
            "====".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("Second Example"));

        // Create third example with title
        let block3 = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![Block::Paragraph(Paragraph::new(
                create_test_inlines("third example"),
                Location::default(),
            ))]),
            "====".to_string(),
            Location::default(),
        )
        .with_title(create_test_title("Third Example"));

        // Render all three examples
        let mut buffer1 = Vec::new();
        let mut visitor1 = TerminalVisitor::new(&mut buffer1, processor.clone());
        visitor1.visit_delimited_block(&block1)?;

        let mut buffer2 = Vec::new();
        let mut visitor2 = TerminalVisitor::new(&mut buffer2, processor.clone());
        visitor2.visit_delimited_block(&block2)?;

        let mut buffer3 = Vec::new();
        let mut visitor3 = TerminalVisitor::new(&mut buffer3, processor.clone());
        visitor3.visit_delimited_block(&block3)?;

        // Check outputs
        let output1 = String::from_utf8_lossy(&buffer1);
        let output2 = String::from_utf8_lossy(&buffer2);
        let output3 = String::from_utf8_lossy(&buffer3);

        assert!(
            output1.contains("EXAMPLE 1."),
            "First example should have number 1, got: {output1}"
        );
        assert!(
            output2.contains("EXAMPLE 2."),
            "Second example should have number 2, got: {output2}"
        );
        assert!(
            output3.contains("EXAMPLE 3."),
            "Third example should have number 3, got: {output3}"
        );

        Ok(())
    }
}
