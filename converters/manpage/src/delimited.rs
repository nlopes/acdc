//! Delimited block rendering for manpages.
//!
//! Handles listing, literal, example, sidebar, quote, and other delimited blocks.

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{DelimitedBlock, DelimitedBlockType};

use crate::{
    Error, ManpageVisitor,
    document::extract_plain_text,
    escape::{EscapeMode, manify},
};

/// Visit a delimited block.
#[allow(clippy::too_many_lines)]
pub(crate) fn visit_delimited_block<W: Write>(
    block: &DelimitedBlock,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    // Handle title if present
    if !block.title.is_empty() {
        let w = visitor.writer_mut();
        writeln!(w, ".sp")?;
        write!(w, "\\fB")?;
        visitor.visit_inline_nodes(&block.title)?;
        let w = visitor.writer_mut();
        writeln!(w, "\\fP")?;
    }

    match &block.inner {
        DelimitedBlockType::DelimitedListing(inlines) => {
            // Listing blocks contain Vec<InlineNode> - extract text
            let content = extract_plain_text(inlines);
            visit_listing_block(&content, visitor)
        }

        DelimitedBlockType::DelimitedLiteral(inlines) => {
            // Literal blocks contain Vec<InlineNode> - extract text
            let content = extract_plain_text(inlines);
            visit_literal_block(&content, visitor)
        }

        DelimitedBlockType::DelimitedExample(blocks) => {
            // Example blocks - render nested content with indentation
            let w = visitor.writer_mut();
            writeln!(w, ".RS 4")?;

            for nested_block in blocks {
                visitor.visit_block(nested_block)?;
            }

            let w = visitor.writer_mut();
            writeln!(w, ".RE")?;
            Ok(())
        }

        DelimitedBlockType::DelimitedSidebar(blocks) => {
            // Sidebar - render with indentation
            let w = visitor.writer_mut();
            writeln!(w, ".RS 4")?;

            for nested_block in blocks {
                visitor.visit_block(nested_block)?;
            }

            let w = visitor.writer_mut();
            writeln!(w, ".RE")?;
            Ok(())
        }

        DelimitedBlockType::DelimitedOpen(blocks) => {
            // Open block - render content directly
            for nested_block in blocks {
                visitor.visit_block(nested_block)?;
            }
            Ok(())
        }

        DelimitedBlockType::DelimitedQuote(blocks) => {
            // Quote block - indented with optional attribution
            let w = visitor.writer_mut();
            writeln!(w, ".RS 4")?;

            for nested_block in blocks {
                visitor.visit_block(nested_block)?;
            }

            let w = visitor.writer_mut();
            writeln!(w, ".RE")?;

            // Render attribution if present (from [quote, author, citation] style)
            let attribution = block.metadata.attributes.get_string("attribution");
            let citation = block.metadata.attributes.get_string("citation");
            if attribution.is_some() || citation.is_some() {
                let w = visitor.writer_mut();
                writeln!(w, ".RS 5")?;
                writeln!(w, ".ll -.10i")?;

                if let Some(cite) = citation {
                    let escaped = manify(&cite, EscapeMode::Normalize);
                    write!(w, "{escaped}")?;
                    if attribution.is_some() {
                        write!(w, " ")?;
                    }
                }
                if let Some(author) = attribution {
                    let escaped = manify(&author, EscapeMode::Normalize);
                    write!(w, "\\(em {escaped}")?;
                }
                writeln!(w)?;
                writeln!(w, ".RE")?;
                writeln!(w, ".ll")?;
            }

            Ok(())
        }

        DelimitedBlockType::DelimitedVerse(inlines) => {
            // Verse block - preserve line breaks, extract text
            let w = visitor.writer_mut();
            writeln!(w, ".nf")?;

            // Extract and write content
            let content = extract_plain_text(inlines);
            let escaped = manify(&content, EscapeMode::Preserve);
            for line in escaped.lines() {
                writeln!(w, "{line}")?;
            }

            writeln!(w, ".fi")?;

            // Render verse attribution if present
            let attribution = block.metadata.attributes.get_string("attribution");
            let citation = block.metadata.attributes.get_string("citation");
            if attribution.is_some() || citation.is_some() {
                let w = visitor.writer_mut();
                writeln!(w, ".br")?;
                writeln!(w, ".in +.5i")?;
                writeln!(w, ".ll -.5i")?;

                if let Some(cite) = citation {
                    let escaped = manify(&cite, EscapeMode::Normalize);
                    write!(w, "{escaped}")?;
                    if attribution.is_some() {
                        write!(w, " ")?;
                    }
                }
                if let Some(author) = attribution {
                    let escaped = manify(&author, EscapeMode::Normalize);
                    write!(w, "\\(em {escaped}")?;
                }
                writeln!(w)?;
                writeln!(w, ".in")?;
                writeln!(w, ".ll")?;
            }

            Ok(())
        }

        DelimitedBlockType::DelimitedPass(inlines) => {
            // Passthrough - write content directly
            let w = visitor.writer_mut();
            let content = extract_plain_text(inlines);
            writeln!(w, "{content}")?;
            Ok(())
        }

        DelimitedBlockType::DelimitedComment(_) => {
            // Comment blocks - skip
            Ok(())
        }

        DelimitedBlockType::DelimitedTable(table) => {
            crate::table::visit_table(table, block, visitor)
        }

        DelimitedBlockType::DelimitedStem(stem) => {
            // STEM (math) - render content as-is
            let w = visitor.writer_mut();
            writeln!(w, ".sp")?;
            writeln!(w, "{}", stem.content)?;
            Ok(())
        }

        // Handle any future variants - skip unknown block types
        _ => Ok(()),
    }
}

/// Render a listing (code) block.
fn visit_listing_block<W: Write>(
    content: &str,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    // Use .EX/.EE for code examples (modern groff)
    writeln!(w, ".EX")?;

    // Escape and write content preserving whitespace
    let escaped = manify(content, EscapeMode::Preserve);
    for line in escaped.lines() {
        writeln!(w, "{line}")?;
    }

    writeln!(w, ".EE")?;

    Ok(())
}

/// Render a literal block.
fn visit_literal_block<W: Write>(
    content: &str,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    // Use .nf/.fi for literal (no-fill) mode
    writeln!(w, ".nf")?;

    // Escape and write content preserving whitespace
    let escaped = manify(content, EscapeMode::Preserve);
    for line in escaped.lines() {
        writeln!(w, "{line}")?;
    }

    writeln!(w, ".fi")?;

    Ok(())
}
