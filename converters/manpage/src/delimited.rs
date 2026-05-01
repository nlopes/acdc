//! Delimited block rendering for manpages.
//!
//! Handles listing, literal, example, sidebar, quote, and other delimited blocks.

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{Block, DelimitedBlock, DelimitedBlockType, inlines_to_string};

use crate::{
    Error, ManpageVisitor,
    document::extract_plain_text,
    escape::{EscapeMode, manify},
};

impl<W: Write> ManpageVisitor<'_, '_, W> {
    /// Visit a delimited block.
    pub(crate) fn render_delimited_block(&mut self, block: &DelimitedBlock) -> Result<(), Error> {
        // Handle title if present
        if !block.title.is_empty() {
            let w = self.writer_mut();
            writeln!(w, ".sp")?;
            write!(w, "\\fB")?;
            self.visit_inline_nodes(&block.title)?;
            let w = self.writer_mut();
            writeln!(w, "\\fP")?;
        }

        match &block.inner {
            DelimitedBlockType::DelimitedListing(inlines) => {
                let content = extract_plain_text(inlines);
                self.render_listing_block(&content)
            }
            DelimitedBlockType::DelimitedLiteral(inlines) => {
                let content = extract_plain_text(inlines);
                self.render_literal_block(&content)
            }
            DelimitedBlockType::DelimitedExample(blocks)
            | DelimitedBlockType::DelimitedSidebar(blocks) => {
                self.render_indented_blocks(blocks, 4)
            }
            DelimitedBlockType::DelimitedOpen(blocks) => {
                for nested_block in &blocks.clone() {
                    self.visit_block(nested_block)?;
                }
                Ok(())
            }
            DelimitedBlockType::DelimitedQuote(blocks) => {
                self.render_quote_delimited_block(block, blocks)
            }
            DelimitedBlockType::DelimitedVerse(inlines) => {
                self.render_verse_delimited_block(block, inlines)
            }
            DelimitedBlockType::DelimitedPass(inlines) => {
                let w = self.writer_mut();
                let content = extract_plain_text(inlines);
                writeln!(w, "{content}")?;
                Ok(())
            }
            DelimitedBlockType::DelimitedTable(table) => {
                crate::table::visit_table(table, block, self)
            }
            DelimitedBlockType::DelimitedStem(stem) => {
                let w = self.writer_mut();
                writeln!(w, ".sp")?;
                writeln!(w, "{}", stem.content)?;
                Ok(())
            }
            // Comments and any future variants produce no output
            DelimitedBlockType::DelimitedComment(_) | _ => Ok(()),
        }
    }

    /// Render blocks indented with RS/RE.
    fn render_indented_blocks(&mut self, blocks: &[Block], indent: usize) -> Result<(), Error> {
        let w = self.writer_mut();
        writeln!(w, ".RS {indent}")?;
        for nested_block in &blocks.to_vec() {
            self.visit_block(nested_block)?;
        }
        let w = self.writer_mut();
        writeln!(w, ".RE")?;
        Ok(())
    }

    /// Render a quote delimited block with optional attribution.
    fn render_quote_delimited_block(
        &mut self,
        block: &DelimitedBlock,
        blocks: &[Block],
    ) -> Result<(), Error> {
        let w = self.writer_mut();
        writeln!(w, ".RS 4")?;
        for nested_block in &blocks.to_vec() {
            self.visit_block(nested_block)?;
        }
        let w = self.writer_mut();
        writeln!(w, ".RE")?;

        let attribution = block
            .metadata
            .attribution
            .as_ref()
            .map(|a| inlines_to_string(a));
        let citation = block
            .metadata
            .citetitle
            .as_ref()
            .map(|c| inlines_to_string(c));

        if attribution.is_some() || citation.is_some() {
            let w = self.writer_mut();
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

    /// Render a verse delimited block with optional attribution.
    fn render_verse_delimited_block(
        &mut self,
        block: &DelimitedBlock,
        inlines: &[acdc_parser::InlineNode],
    ) -> Result<(), Error> {
        let w = self.writer_mut();
        writeln!(w, ".nf")?;
        let content = extract_plain_text(inlines);
        let escaped = manify(&content, EscapeMode::Preserve);
        for line in escaped.lines() {
            writeln!(w, "{line}")?;
        }
        writeln!(w, ".fi")?;

        let attribution = block
            .metadata
            .attribution
            .as_ref()
            .map(|a| inlines_to_string(a));
        let citation = block
            .metadata
            .citetitle
            .as_ref()
            .map(|c| inlines_to_string(c));

        if attribution.is_some() || citation.is_some() {
            let w = self.writer_mut();
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

    /// Render a listing (code) block.
    fn render_listing_block(&mut self, content: &str) -> Result<(), Error> {
        let w = self.writer_mut();
        writeln!(w, ".EX")?;
        let escaped = manify(content, EscapeMode::Preserve);
        for line in escaped.lines() {
            writeln!(w, "{line}")?;
        }
        writeln!(w, ".EE")?;
        Ok(())
    }

    /// Render a literal block.
    fn render_literal_block(&mut self, content: &str) -> Result<(), Error> {
        let w = self.writer_mut();
        writeln!(w, ".nf")?;
        let escaped = manify(content, EscapeMode::Preserve);
        for line in escaped.lines() {
            writeln!(w, "{line}")?;
        }
        writeln!(w, ".fi")?;
        Ok(())
    }
}
