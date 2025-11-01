use acdc_converters_common::visitor::{WritableVisitor, WritableVisitorExt};
use acdc_parser::{Block, DelimitedBlock, DelimitedBlockType, InlineNode};
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
    let w = visitor.writer_mut();
    writeln!(w)?;

    match &block.inner {
        DelimitedBlockType::DelimitedTable(t) => crate::table::visit_table(t, visitor, processor),
        DelimitedBlockType::DelimitedListing(inlines) => {
            render_preformatted_block(visitor, &block.title, inlines, "listing", processor)
        }
        DelimitedBlockType::DelimitedLiteral(inlines) => {
            render_preformatted_block(visitor, &block.title, inlines, "literal", processor)
        }
        DelimitedBlockType::DelimitedExample(blocks) => {
            render_example_block(visitor, &block.title, blocks, processor)
        }
        DelimitedBlockType::DelimitedQuote(blocks) => {
            render_quote_block(visitor, &block.title, blocks)
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
            writeln!(
                w,
                "  [STEM({}): {}]",
                stem.notation.to_string(),
                stem.content
            )?;
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

/// Render a preformatted block (listing or literal) with monospace styling.
fn render_preformatted_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    inlines: &[InlineNode],
    block_type: &str,
    processor: &Processor,
) -> Result<(), Error> {
    // Render title if present
    if !title.is_empty() {
        let w = visitor.writer_mut();
        write!(w, "  ")?;
        QueueableCommand::queue(
            w,
            PrintStyledContent(format!("[{block_type}]").dark_grey().bold()),
        )?;
        write!(w, " ")?;
        let _ = w;
        visitor.visit_inline_nodes(title)?;
        let w = visitor.writer_mut();
        writeln!(w)?;
    }

    // Render content with indentation and monospace background
    let w = visitor.writer_mut();
    writeln!(w, "  ┌{}", "─".repeat(76))?;
    write!(w, "  │ ")?;
    let _ = w;

    // Render inline nodes as plain text
    for inline in inlines {
        crate::inlines::visit_inline_node(inline, visitor, processor)?;
    }

    let w = visitor.writer_mut();
    writeln!(w)?;
    writeln!(w, "  └{}", "─".repeat(76))?;

    Ok(())
}

/// Render an example block with "Example N." numbering.
fn render_example_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    blocks: &[Block],
    _processor: &Processor,
) -> Result<(), Error> {
    // Render title with "Example" prefix if present
    if title.is_empty() {
        let w = visitor.writer_mut();
        QueueableCommand::queue(w, PrintStyledContent("  Example:".cyan().bold()))?;
        writeln!(w)?;
    } else {
        let w = visitor.writer_mut();
        write!(w, "  ")?;
        QueueableCommand::queue(w, PrintStyledContent("Example:".cyan().bold()))?;
        write!(w, " ")?;
        let _ = w;
        visitor.visit_inline_nodes(title)?;
        let w = visitor.writer_mut();
        writeln!(w)?;
    }

    // Render content with indentation
    let w = visitor.writer_mut();
    writeln!(w, "  ┌{}", "─".repeat(76))?;
    let _ = w;

    for nested_block in blocks {
        let w = visitor.writer_mut();
        write!(w, "  │ ")?;
        let _ = w;
        visitor.visit_block(nested_block)?;
    }
    let w = visitor.writer_mut();
    writeln!(w, "  └{}", "─".repeat(76))?;

    Ok(())
}

/// Render a quote block with quote styling.
fn render_quote_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    blocks: &[Block],
) -> Result<(), Error> {
    // Render title if present
    if !title.is_empty() {
        let w = visitor.writer_mut();
        write!(w, "  ")?;
        let _ = w;
        visitor.visit_inline_nodes(title)?;
        let w = visitor.writer_mut();
        writeln!(w)?;
    }

    // Render content with quote marks and indentation
    let w = visitor.writer_mut();
    writeln!(w, "  {}", "\u{201C}".italic().grey())?; // Opening quote
    let _ = w;

    for nested_block in blocks {
        let w = visitor.writer_mut();
        write!(w, "    ")?; // Extra indentation for quotes
        let _ = w;
        visitor.visit_block(nested_block)?;
    }
    let w = visitor.writer_mut();
    writeln!(w, "  {}", "\u{201D}".italic().grey())?; // Closing quote

    Ok(())
}

/// Render a sidebar block with a box.
fn render_sidebar_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    blocks: &[Block],
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    writeln!(w, "  ╔{}╗", "═".repeat(76))?;

    // Render title if present
    if !title.is_empty() {
        write!(w, "  ║ ")?;
        QueueableCommand::queue(w, PrintStyledContent(String::new().bold()))?;
        let _ = w;
        visitor.visit_inline_nodes(title)?;
        let w = visitor.writer_mut();
        writeln!(w, " ║")?;
        writeln!(w, "  ╠{}╣", "═".repeat(76))?;
    }

    // Render content
    for nested_block in blocks {
        let w = visitor.writer_mut();
        write!(w, "  ║ ")?;
        let _ = w;
        visitor.visit_block(nested_block)?;
    }
    let w = visitor.writer_mut();
    writeln!(w, "  ╚{}╝", "═".repeat(76))?;

    Ok(())
}

/// Render a verse block (poetry) preserving line breaks.
fn render_verse_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    title: &[InlineNode],
    inlines: &[InlineNode],
) -> Result<(), Error> {
    // Render title if present
    if !title.is_empty() {
        let w = visitor.writer_mut();
        write!(w, "  ")?;
        let _ = w;
        visitor.visit_inline_nodes(title)?;
        let w = visitor.writer_mut();
        writeln!(w)?;
    }

    // Render verse content with indentation
    let w = visitor.writer_mut();
    write!(w, "    ")?;
    let _ = w;
    visitor.visit_inline_nodes(inlines)?;
    let w = visitor.writer_mut();
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
    use acdc_converters_common::visitor::Visitor;
    use acdc_parser::{BlockMetadata, DocumentAttributes, Location, Paragraph, Plain};

    /// Create simple plain text inline nodes for testing
    fn create_test_inlines(content: &str) -> Vec<InlineNode> {
        vec![InlineNode::PlainText(Plain {
            content: content.to_string(),
            location: Location::default(),
        })]
    }

    /// Create test processor with default options
    fn create_test_processor() -> Processor {
        let options = Options::default();
        let document_attributes = DocumentAttributes::default();
        Processor {
            options,
            document_attributes,
            toc_entries: vec![],
        }
    }

    #[test]
    fn test_listing_block_basic() -> Result<(), Error> {
        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedListing(create_test_inlines("code content here")),
            title: Vec::new(),
            delimiter: "----".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("┌"), "Should have top-left border");
        assert!(output_str.contains("└"), "Should have bottom-left border");
        assert!(
            output_str.contains("code content here"),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_listing_block_with_title() -> Result<(), Error> {
        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            title: create_test_inlines("My Code Listing"),
            delimiter: "----".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("[listing]"),
            "Should show [listing] label"
        );
        assert!(
            output_str.contains("My Code Listing"),
            "Should contain title"
        );
        assert!(output_str.contains("code here"), "Should contain content");

        Ok(())
    }

    #[test]
    fn test_literal_block_basic() -> Result<(), Error> {
        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedLiteral(create_test_inlines("literal text")),
            title: Vec::new(),
            delimiter: "....".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("┌"), "Should have top-left border");
        assert!(output_str.contains("└"), "Should have bottom-left border");
        assert!(
            output_str.contains("literal text"),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_literal_block_with_title() -> Result<(), Error> {
        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedLiteral(create_test_inlines("literal content")),
            title: create_test_inlines("Literal Block Title"),
            delimiter: "....".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("[literal]"),
            "Should show [literal] label"
        );
        assert!(
            output_str.contains("Literal Block Title"),
            "Should contain title"
        );
        assert!(
            output_str.contains("literal content"),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_example_block_basic() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph {
            content: create_test_inlines("example text"),
            location: Location::default(),
            metadata: BlockMetadata::default(),
            title: Vec::new(),
        })];

        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedExample(content),
            title: Vec::new(),
            delimiter: "====".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Example:"),
            "Should show Example: label"
        );
        assert!(output_str.contains("┌"), "Should have top-left border");
        assert!(output_str.contains("└"), "Should have bottom-left border");
        assert!(
            output_str.contains("example text"),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_example_block_with_title() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph {
            content: create_test_inlines("example content"),
            location: Location::default(),
            metadata: BlockMetadata::default(),
            title: Vec::new(),
        })];

        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedExample(content),
            title: create_test_inlines("Custom Example Title"),
            delimiter: "====".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Example:"),
            "Should show Example: prefix"
        );
        assert!(
            output_str.contains("Custom Example Title"),
            "Should contain custom title"
        );
        assert!(
            output_str.contains("example content"),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_quote_block_basic() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph {
            content: create_test_inlines("This is a quote."),
            location: Location::default(),
            metadata: BlockMetadata::default(),
            title: Vec::new(),
        })];

        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedQuote(content),
            title: Vec::new(),
            delimiter: "____".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("\u{201C}"),
            "Should have opening curly quote"
        );
        assert!(
            output_str.contains("\u{201D}"),
            "Should have closing curly quote"
        );
        assert!(
            output_str.contains("This is a quote."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_quote_block_with_title() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph {
            content: create_test_inlines("Quote content here."),
            location: Location::default(),
            metadata: BlockMetadata::default(),
            title: Vec::new(),
        })];

        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedQuote(content),
            title: create_test_inlines("Quote Title"),
            delimiter: "____".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Quote Title"), "Should contain title");
        assert!(
            output_str.contains("\u{201C}"),
            "Should have opening curly quote"
        );
        assert!(
            output_str.contains("\u{201D}"),
            "Should have closing curly quote"
        );
        assert!(
            output_str.contains("Quote content here."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_quote_block_multiple_paragraphs() -> Result<(), Error> {
        let content = vec![
            Block::Paragraph(Paragraph {
                content: create_test_inlines("First paragraph."),
                location: Location::default(),
                metadata: BlockMetadata::default(),
                title: Vec::new(),
            }),
            Block::Paragraph(Paragraph {
                content: create_test_inlines("Second paragraph."),
                location: Location::default(),
                metadata: BlockMetadata::default(),
                title: Vec::new(),
            }),
        ];

        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedQuote(content),
            title: Vec::new(),
            delimiter: "____".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

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
        let content = vec![Block::Paragraph(Paragraph {
            content: create_test_inlines("Sidebar content."),
            location: Location::default(),
            metadata: BlockMetadata::default(),
            title: Vec::new(),
        })];

        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedSidebar(content),
            title: Vec::new(),
            delimiter: "****".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("╔"),
            "Should have top-left double border"
        );
        assert!(
            output_str.contains("╚"),
            "Should have bottom-left double border"
        );
        assert!(
            output_str.contains("Sidebar content."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_sidebar_block_with_title() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph {
            content: create_test_inlines("Sidebar text here."),
            location: Location::default(),
            metadata: BlockMetadata::default(),
            title: Vec::new(),
        })];

        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedSidebar(content),
            title: create_test_inlines("Sidebar Title"),
            delimiter: "****".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Sidebar Title"), "Should contain title");
        assert!(output_str.contains("╠"), "Should have title separator");
        assert!(
            output_str.contains("Sidebar text here."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_sidebar_block_multiple_paragraphs() -> Result<(), Error> {
        let content = vec![
            Block::Paragraph(Paragraph {
                content: create_test_inlines("First sidebar paragraph."),
                location: Location::default(),
                metadata: BlockMetadata::default(),
                title: Vec::new(),
            }),
            Block::Paragraph(Paragraph {
                content: create_test_inlines("Second sidebar paragraph."),
                location: Location::default(),
                metadata: BlockMetadata::default(),
                title: Vec::new(),
            }),
        ];

        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedSidebar(content),
            title: Vec::new(),
            delimiter: "****".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

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
        let content = vec![Block::Paragraph(Paragraph {
            content: create_test_inlines("Open block content."),
            location: Location::default(),
            metadata: BlockMetadata::default(),
            title: Vec::new(),
        })];

        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedOpen(content),
            title: Vec::new(),
            delimiter: "--".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

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
        let content = vec![Block::Paragraph(Paragraph {
            content: create_test_inlines("Content here."),
            location: Location::default(),
            metadata: BlockMetadata::default(),
            title: Vec::new(),
        })];

        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedOpen(content),
            title: create_test_inlines("Open Block Title"),
            delimiter: "--".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

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
        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedVerse(create_test_inlines(
                "Roses are red\nViolets are blue",
            )),
            title: Vec::new(),
            delimiter: "____".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

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
        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedVerse(create_test_inlines(
                "Poetry line 1\nPoetry line 2",
            )),
            title: create_test_inlines("Poem Title"),
            delimiter: "____".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

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
        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedPass(create_test_inlines("<raw>passthrough</raw>")),
            title: Vec::new(),
            delimiter: "++++".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

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

        let stem_content = StemContent {
            content: "x = y^2".to_string(),
            notation: StemNotation::Latexmath,
        };

        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedStem(stem_content),
            title: Vec::new(),
            delimiter: "++++".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

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
        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedComment(create_test_inlines("This is a comment")),
            title: Vec::new(),
            delimiter: "////".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

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
        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedListing(Vec::new()),
            title: Vec::new(),
            delimiter: "----".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        // Empty blocks should still render borders
        assert!(
            output_str.contains("┌"),
            "Should have top border even when empty"
        );
        assert!(
            output_str.contains("└"),
            "Should have bottom border even when empty"
        );

        Ok(())
    }

    #[test]
    fn test_empty_quote_block() -> Result<(), Error> {
        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedQuote(Vec::new()),
            title: Vec::new(),
            delimiter: "____".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_delimited_block(&block)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        // Empty quote should still have quote marks
        assert!(output_str.contains("\u{201C}"), "Should have opening quote");
        assert!(output_str.contains("\u{201D}"), "Should have closing quote");

        Ok(())
    }

    #[test]
    fn test_listing_with_special_characters() -> Result<(), Error> {
        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedListing(create_test_inlines(
                "<html>&amp; special chars \"quotes\" 'apostrophes'",
            )),
            title: Vec::new(),
            delimiter: "----".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

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
        let content = vec![Block::Paragraph(Paragraph {
            content: create_test_inlines("This example shows: code snippet"),
            location: Location::default(),
            metadata: BlockMetadata::default(),
            title: Vec::new(),
        })];

        let block = DelimitedBlock {
            inner: DelimitedBlockType::DelimitedExample(content),
            title: create_test_inlines("Nested Content"),
            delimiter: "====".to_string(),
            location: Location::default(),
            metadata: BlockMetadata::default(),
        };

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
}
