//! Delimited block rendering for manpages.
//!
//! Handles listing, literal, example, sidebar, quote, and other delimited blocks.

use std::{borrow::Cow, io::Write};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::{SubsFlags, effective_subs_flags};
use acdc_converters_core::{
    TraversalContext,
    code::{default_line_comment, detect_language},
    shows_block_title,
    substitutions::TextBoundaries,
    visitor::WritableVisitor,
};
use acdc_parser::{
    Block, BlockMetadata, DelimitedBlock, DelimitedBlockType, InlineMacro, InlineNode,
};

use crate::{
    Error, ManpageVisitor,
    document::extract_verbatim_text,
    escape::{EscapeMode, manify},
    inlines::contains_link,
    manpage_visitor::IndexCollection,
};

impl<'a, W: Write> ManpageVisitor<'a, '_, W> {
    /// Visit a delimited block.
    pub(crate) fn render_delimited_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: &'a DelimitedBlock<'a>,
    ) -> Result<(), Error> {
        if shows_block_title(&block.inner) && !block.title.is_empty() {
            let w = self.writer_mut();
            writeln!(w, ".sp")?;
            self.render_captioned_title(traversal, &block.title, &block.metadata)?;
        }

        match &block.inner {
            DelimitedBlockType::DelimitedListing(inlines) => {
                self.collect_index_terms_from_inlines(traversal, inlines)?;
                self.render_listing_block(traversal, inlines, &block.metadata)
            }
            DelimitedBlockType::DelimitedLiteral(inlines) => {
                self.collect_index_terms_from_inlines(traversal, inlines)?;
                let content = self.verbatim_content(traversal, inlines, &block.metadata, false)?;
                self.render_literal_block(&content)
            }
            DelimitedBlockType::DelimitedExample(blocks)
            | DelimitedBlockType::DelimitedSidebar(blocks) => {
                self.render_indented_blocks(traversal, blocks, 4)
            }
            DelimitedBlockType::DelimitedOpen(blocks) => {
                for nested_block in blocks {
                    traversal.visit_block(self, nested_block)?;
                }
                Ok(())
            }
            DelimitedBlockType::DelimitedQuote(blocks) => {
                self.render_quote_delimited_block(traversal, block, blocks)
            }
            DelimitedBlockType::DelimitedVerse(inlines) => {
                self.render_verse_delimited_block(traversal, block, inlines)
            }
            DelimitedBlockType::DelimitedPass(inlines) => {
                // Passthrough blocks contain backend-native roff by definition.
                let w = self.writer_mut();
                let content = extract_verbatim_text(inlines);
                writeln!(w, "{content}")?;
                Ok(())
            }
            DelimitedBlockType::DelimitedTable(table) => {
                crate::table::visit_table(traversal, table, block, self)
            }
            DelimitedBlockType::DelimitedStem(stem) => {
                let w = self.writer_mut();
                writeln!(w, ".sp")?;
                writeln!(w, "{}", manify(stem.content, EscapeMode::Preserve))?;
                Ok(())
            }
            DelimitedBlockType::DelimitedComment(_) => Ok(()),
            _ => {
                self.warn_unsupported_parser_variant("delimited block");
                Ok(())
            }
        }
    }

    /// Render blocks indented with RS/RE.
    fn render_indented_blocks(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        blocks: &'a [Block<'a>],
        indent: usize,
    ) -> Result<(), Error> {
        let w = self.writer_mut();
        writeln!(w, ".RS {indent}")?;
        for nested_block in blocks {
            traversal.visit_block(self, nested_block)?;
        }
        let w = self.writer_mut();
        writeln!(w, ".RE")?;
        Ok(())
    }

    /// Render a quote delimited block with optional attribution.
    fn render_quote_delimited_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: &'a DelimitedBlock<'a>,
        blocks: &'a [Block<'a>],
    ) -> Result<(), Error> {
        let w = self.writer_mut();
        writeln!(w, ".RS 4")?;
        for nested_block in blocks {
            traversal.visit_block(self, nested_block)?;
        }
        let w = self.writer_mut();
        writeln!(w, ".RE")?;

        self.render_attribution(
            traversal,
            &block.metadata,
            &[".RS 5", ".ll -.10i"],
            &[".RE", ".ll"],
        )
    }

    /// Render a verse delimited block with optional attribution.
    fn render_verse_delimited_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: &'a DelimitedBlock<'a>,
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
            traversal,
            &block.metadata,
            &[".br", ".in +.5i", ".ll -.5i"],
            &[".in", ".ll"],
        )
    }

    /// Render a listing (code) block.
    fn render_listing_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: &[InlineNode<'_>],
        metadata: &BlockMetadata<'_>,
    ) -> Result<(), Error> {
        let content = self.verbatim_content(traversal, inlines, metadata, true)?;
        let w = self.writer_mut();
        writeln!(w, ".EX")?;
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
        for line in content.lines() {
            writeln!(w, "{line}")?;
        }
        writeln!(w, ".fi")?;
        Ok(())
    }

    pub(crate) fn verbatim_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: &[InlineNode<'_>],
        metadata: &BlockMetadata<'_>,
        source_guards: bool,
    ) -> Result<String, Error> {
        // Link labels inherit the verbatim block's substitution settings.
        #[cfg(feature = "pre-spec-subs")]
        let previous_subs = self
            .processor
            .current_subs
            .replace(effective_subs_flags(metadata.substitutions.as_ref(), true));
        let mut output = Vec::new();
        let mut visitor = self.nested_visitor(&mut output);
        // Block callers already collected terms; rendering link labels must not add them again.
        visitor.index_collection = IndexCollection::Disabled;
        let result =
            visitor.write_verbatim_nodes(traversal, nodes, source_guards.then_some(metadata));
        #[cfg(feature = "pre-spec-subs")]
        visitor.processor.current_subs.set(previous_subs);
        result?;
        Ok(String::from_utf8_lossy(&output).into_owned())
    }

    fn write_verbatim_nodes(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: &[InlineNode<'_>],
        source_metadata: Option<&BlockMetadata<'_>>,
    ) -> Result<(), Error> {
        let comment_prefix = default_line_comment(source_metadata.and_then(detect_language));
        let previous_boundaries = self.text_boundaries;
        let result = (|| {
            for (index, node) in nodes.iter().enumerate() {
                self.text_boundaries = TextBoundaries::new(
                    previous_boundaries.at_paragraph_start() && index == 0,
                    previous_boundaries.at_paragraph_end() && index + 1 == nodes.len(),
                );
                if source_metadata.is_some()
                    && let InlineNode::VerbatimText(verbatim) = node
                {
                    let mut content = Cow::Borrowed(verbatim.content);
                    if index
                        .checked_sub(1)
                        .is_some_and(|previous| is_xml_callout(nodes, previous))
                    {
                        content =
                            Cow::Owned(content.strip_prefix("-->").unwrap_or(&content).to_string());
                    }
                    if index.checked_add(1).is_some_and(|next| {
                        matches!(nodes.get(next), Some(InlineNode::CalloutRef(_)))
                    }) {
                        content = if index
                            .checked_add(1)
                            .is_some_and(|next| is_xml_callout(nodes, next))
                        {
                            Cow::Owned(content.strip_suffix("<!--").unwrap_or(&content).to_string())
                        } else {
                            strip_callout_guard(content, comment_prefix)
                        };
                    }
                    self.write_verbatim_text(&content)?;
                } else if source_metadata.is_some()
                    && let InlineNode::CalloutRef(callout) = node
                {
                    write!(self.writer_mut(), "\\fB({})\\fP", callout.number)?;
                } else {
                    self.write_verbatim_node(traversal, node)?;
                }
            }
            Ok(())
        })();
        self.text_boundaries = previous_boundaries;
        result
    }

    fn write_verbatim_text(&mut self, text: &str) -> Result<(), Error> {
        #[cfg(feature = "pre-spec-subs")]
        let text = if self
            .processor
            .current_subs
            .get()
            .contains(SubsFlags::REPLACEMENTS)
        {
            let mut replaced =
                crate::inlines::replacements().transform_verbatim(text, self.text_boundaries);
            // Like prose, omit the space generated after a final em dash while
            // retaining authored trailing whitespace in verbatim content.
            if self.text_boundaries.at_paragraph_end()
                && text.ends_with("--")
                && replaced.ends_with(' ')
            {
                replaced.pop();
            }
            Cow::Owned(replaced)
        } else {
            Cow::Borrowed(text)
        };
        write!(self.writer_mut(), "{}", manify(&text, EscapeMode::Preserve))?;
        Ok(())
    }

    fn write_verbatim_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: &InlineNode<'_>,
    ) -> Result<(), Error> {
        let (prefix, content, suffix) = match node {
            InlineNode::PlainText(text) => return self.write_verbatim_text(text.content),
            InlineNode::VerbatimText(text) => return self.write_verbatim_text(text.content),
            InlineNode::BoldText(text) => ("\\fB", text.content.as_slice(), "\\fP"),
            InlineNode::ItalicText(text) => ("\\fI", text.content.as_slice(), "\\fP"),
            InlineNode::MonospaceText(text) => ("\\f(CR", text.content.as_slice(), "\\fP"),
            InlineNode::HighlightText(text) => ("", text.content.as_slice(), ""),
            InlineNode::SuperscriptText(text) => ("", text.content.as_slice(), ""),
            InlineNode::SubscriptText(text) => ("", text.content.as_slice(), ""),
            InlineNode::CurvedQuotationText(text) => ("\\(lq", text.content.as_slice(), "\\(rq"),
            InlineNode::CurvedApostropheText(text) => ("\\(oq", text.content.as_slice(), "\\(cq"),
            InlineNode::Macro(InlineMacro::IndexTerm(term)) if term.is_visible() => {
                ("", term.term(), "")
            }
            InlineNode::Macro(_) if contains_link(node) => {
                // Link commands need their renderer; flattening keeps only the label.
                return self.render_inline_node(traversal, node);
            }
            InlineNode::RawText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::Macro(_)
            | InlineNode::CalloutRef(_)
            | _ => {
                let text = extract_verbatim_text(std::slice::from_ref(node));
                write!(self.writer_mut(), "{}", manify(&text, EscapeMode::Preserve))?;
                return Ok(());
            }
        };
        write!(self.writer_mut(), "{prefix}")?;
        self.write_verbatim_nodes(traversal, content, None)?;
        write!(self.writer_mut(), "{suffix}")?;
        Ok(())
    }
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
