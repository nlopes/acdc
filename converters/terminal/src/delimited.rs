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
        DelimitedBlockType::DelimitedStem(_stem) => {
            // STEM/math content - show placeholder in terminal
            render_title_if_present(visitor, &block.title)?;
            let w = visitor.writer_mut();
            writeln!(w, "  [STEM content - not rendered in terminal]")?;
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
