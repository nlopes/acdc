//! Delimited block rendering for manpages.
//!
//! Handles listing, literal, example, sidebar, quote, and other delimited blocks.

use std::io::Write;

use acdc_converters_core::{
    shows_block_title,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{Block, DelimitedBlock, DelimitedBlockType};

use crate::{
    Error, ManpageVisitor,
    document::extract_verbatim_text,
    escape::{EscapeMode, manify},
};

impl<W: Write> ManpageVisitor<'_, '_, W> {
    /// Visit a delimited block.
    pub(crate) fn render_delimited_block(&mut self, block: &DelimitedBlock) -> Result<(), Error> {
        if shows_block_title(&block.inner) && !block.title.is_empty() {
            let w = self.writer_mut();
            writeln!(w, ".sp")?;
            self.render_captioned_title(&block.title, &block.metadata)?;
        }

        match &block.inner {
            DelimitedBlockType::DelimitedListing(inlines) => {
                let content = extract_verbatim_text(inlines);
                self.render_listing_block(&content)
            }
            DelimitedBlockType::DelimitedLiteral(inlines) => {
                let content = extract_verbatim_text(inlines);
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
                let content = extract_verbatim_text(inlines);
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

        self.render_attribution(&block.metadata, &[".RS 5", ".ll -.10i"], &[".RE", ".ll"])
    }

    /// Render a verse delimited block with optional attribution.
    fn render_verse_delimited_block(
        &mut self,
        block: &DelimitedBlock,
        inlines: &[acdc_parser::InlineNode],
    ) -> Result<(), Error> {
        let w = self.writer_mut();
        writeln!(w, ".nf")?;
        let content = extract_verbatim_text(inlines);
        let escaped = manify(&content, EscapeMode::Preserve);
        for line in escaped.lines() {
            writeln!(w, "{line}")?;
        }
        writeln!(w, ".fi")?;

        self.render_attribution(
            &block.metadata,
            &[".br", ".in +.5i", ".ll -.5i"],
            &[".in", ".ll"],
        )
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
