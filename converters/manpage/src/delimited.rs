//! Delimited block rendering for manpages.
//!
//! Handles listing, literal, example, sidebar, quote, and other delimited blocks.

use std::{borrow::Cow, fmt::Write as _, io::Write};

use acdc_converters_core::{
    code::{default_line_comment, detect_language},
    shows_block_title,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{Block, BlockMetadata, DelimitedBlock, DelimitedBlockType, InlineNode};

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
                self.render_listing_block(inlines, &block.metadata)
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
                // Passthrough blocks contain backend-native roff by definition.
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
                writeln!(w, "{}", manify(stem.content, EscapeMode::Preserve))?;
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
    fn render_listing_block(
        &mut self,
        inlines: &[InlineNode<'_>],
        metadata: &BlockMetadata<'_>,
    ) -> Result<(), Error> {
        let w = self.writer_mut();
        writeln!(w, ".EX")?;
        let content = source_content(inlines, metadata);
        for line in content.lines() {
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

pub(crate) fn source_content(nodes: &[InlineNode<'_>], metadata: &BlockMetadata<'_>) -> String {
    let language = detect_language(metadata);
    let comment_prefix = default_line_comment(language);
    let mut output = String::new();

    for (index, node) in nodes.iter().enumerate() {
        if let InlineNode::VerbatimText(verbatim) = node {
            let mut content = Cow::Borrowed(verbatim.content);
            if index
                .checked_sub(1)
                .is_some_and(|previous| is_xml_callout(nodes, previous))
            {
                content = Cow::Owned(content.strip_prefix("-->").unwrap_or(&content).to_string());
            }
            if index
                .checked_add(1)
                .is_some_and(|next| matches!(nodes.get(next), Some(InlineNode::CalloutRef(_))))
            {
                content = if index
                    .checked_add(1)
                    .is_some_and(|next| is_xml_callout(nodes, next))
                {
                    Cow::Owned(content.strip_suffix("<!--").unwrap_or(&content).to_string())
                } else {
                    strip_callout_guard(content, comment_prefix)
                };
            }
            output.push_str(&manify(&content, EscapeMode::Preserve));
        } else if let InlineNode::CalloutRef(callout) = node {
            let _ = write!(output, "\\fB({})\\fP", callout.number);
        } else {
            let content = extract_verbatim_text(std::slice::from_ref(node));
            output.push_str(&manify(&content, EscapeMode::Preserve));
        }
    }

    output
}

fn is_xml_callout(nodes: &[InlineNode<'_>], index: usize) -> bool {
    matches!(nodes.get(index), Some(InlineNode::CalloutRef(_)))
        && index.checked_sub(1).is_some_and(|previous| {
            matches!(
                nodes.get(previous),
                Some(InlineNode::VerbatimText(text)) if text.content.ends_with("<!--")
            )
        })
        && index.checked_add(1).is_some_and(|next| {
            matches!(
                nodes.get(next),
                Some(InlineNode::VerbatimText(text)) if text.content.starts_with("-->")
            )
        })
}

fn strip_callout_guard<'a>(text: Cow<'a, str>, comment_prefix: Option<&str>) -> Cow<'a, str> {
    let Some(prefix) = comment_prefix else {
        return text;
    };
    let trimmed = text.trim_end();
    let Some(content) = trimmed.strip_suffix(prefix) else {
        return text;
    };
    Cow::Owned(format!("{} ", content.trim_end()))
}
