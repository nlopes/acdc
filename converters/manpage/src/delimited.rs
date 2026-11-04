//! Delimited block rendering for manpages.
//!
//! Handles listing, literal, example, sidebar, quote, and other delimited blocks.

use std::io::Write;

use acdc_converters_core::{InlineTextTransform, TraversalContext};
use acdc_parser::{
    SymbolKind,
    ast::{AttributesRef, BlockKind, BlockRef, ContentRef, InlineKind, InlineRef},
};

use crate::{
    Error, ManpageVisitor,
    document::extract_verbatim_text,
    escape::{EscapeMode, manify},
    inlines::{contains_link, replacements},
    manpage_visitor::{IndexCollection, attribute_text},
};

impl<'a, W: Write> ManpageVisitor<'a, '_, W> {
    pub(crate) fn render_delimited_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
    ) -> Result<(), Error> {
        if !matches!(block.kind(), BlockKind::Passthrough(_))
            && block.title().is_some()
            && let Some(attributes) = block.attributes()
        {
            writeln!(self.writer_mut(), ".sp")?;
            self.render_captioned_title(traversal, block.title(), attributes)?;
        }

        match block.kind() {
            BlockKind::Listing(node) => {
                self.collect_index_terms_from_inlines(traversal, node.content())?;
                self.render_listing_block(traversal, node.content(), node.attributes())
            }
            BlockKind::Literal(node) => {
                self.collect_index_terms_from_inlines(traversal, node.content())?;
                let content =
                    self.verbatim_content(traversal, node.content(), node.attributes())?;
                self.render_literal_block(&content)
            }
            BlockKind::Example(node) => self.render_indented_blocks(traversal, node.blocks(), 4),
            BlockKind::Sidebar(node) => self.render_indented_blocks(traversal, node.blocks(), 4),
            BlockKind::Open(node) => traversal.visit_blocks(self, node.blocks()),
            BlockKind::Quote(node) => {
                self.render_indented_blocks(traversal, node.blocks(), 4)?;
                self.render_attribution(
                    traversal,
                    node.attributes(),
                    &[".RS 5", ".ll -.10i"],
                    &[".RE", ".ll"],
                )
            }
            BlockKind::Verse(node) => {
                self.render_verse_content(traversal, node.content())?;
                self.render_attribution(
                    traversal,
                    node.attributes(),
                    &[".br", ".in +.5i", ".ll -.5i"],
                    &[".in", ".ll"],
                )
            }
            BlockKind::Passthrough(node) => {
                // Passthrough blocks contain backend-native roff by definition.
                writeln!(
                    self.writer_mut(),
                    "{}",
                    extract_verbatim_text(node.content())
                )?;
                Ok(())
            }
            BlockKind::Table(table) => crate::table::visit_table(traversal, table, self),
            BlockKind::Stem(node) => {
                let w = self.writer_mut();
                writeln!(w, ".sp")?;
                writeln!(w, "{}", manify(node.text(), EscapeMode::Preserve))?;
                Ok(())
            }
            BlockKind::DelimitedComment(_) => Ok(()),
            BlockKind::TableOfContents(_)
            | BlockKind::Admonition(_)
            | BlockKind::DiscreteHeader(_)
            | BlockKind::DocumentAttribute(_)
            | BlockKind::ThematicBreak(_)
            | BlockKind::PageBreak(_)
            | BlockKind::UnorderedList(_)
            | BlockKind::OrderedList(_)
            | BlockKind::CalloutList(_)
            | BlockKind::DescriptionList(_)
            | BlockKind::Section(_)
            | BlockKind::Paragraph(_)
            | BlockKind::Image(_)
            | BlockKind::Audio(_)
            | BlockKind::Video(_)
            | BlockKind::Comment(_)
            | _ => {
                self.warn_unsupported_parser_variant("delimited block");
                Ok(())
            }
        }
    }

    fn render_indented_blocks(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        blocks: impl Iterator<Item = BlockRef<'a>>,
        indent: usize,
    ) -> Result<(), Error> {
        writeln!(self.writer_mut(), ".RS {indent}")?;
        traversal.visit_blocks(self, blocks)?;
        writeln!(self.writer_mut(), ".RE")?;
        Ok(())
    }

    /// Render a listing (code) block.
    fn render_listing_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: ContentRef<'_>,
        attributes: AttributesRef<'_>,
    ) -> Result<(), Error> {
        let content = self.verbatim_content(traversal, inlines, attributes)?;
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
        nodes: ContentRef<'_>,
        attributes: AttributesRef<'_>,
    ) -> Result<String, Error> {
        let mut output = Vec::new();
        let mut visitor = self.nested_visitor(&mut output);
        // Block callers already collected terms; rendering link labels must not add them again.
        visitor.index_collection = IndexCollection::Disabled;
        visitor.write_verbatim_nodes(traversal, nodes, Some(attributes))?;
        Ok(String::from_utf8_lossy(&output).into_owned())
    }

    fn write_verbatim_nodes(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: ContentRef<'_>,
        attributes: Option<AttributesRef<'_>>,
    ) -> Result<(), Error> {
        let comment_prefix =
            attributes.and_then(|attributes| attribute_text(attributes, "line-comment"));
        let previous_boundaries = self.text_boundaries;
        let result = (|| {
            let siblings = nodes.inlines();
            for (index, node) in nodes.inlines().enumerate() {
                self.text_boundaries = previous_boundaries
                    .with_ordinary_replacements(false)
                    .for_inline_refs(
                        Some(node),
                        siblings.clone().take(index).rev(),
                        siblings.clone().skip(index + 1),
                    );
                if attributes.is_some()
                    && let InlineKind::Verbatim(verbatim) = node.kind()
                {
                    let mut content = verbatim.text();
                    if index
                        .checked_sub(1)
                        .is_some_and(|previous| is_xml_callout(nodes, previous))
                    {
                        content = content.strip_prefix("-->").unwrap_or(content);
                    }
                    if index.checked_add(1).is_some_and(|next| {
                        matches!(
                            nodes.inlines().nth(next).map(InlineRef::kind),
                            Some(InlineKind::Callout(_))
                        )
                    }) {
                        if index
                            .checked_add(1)
                            .is_some_and(|next| is_xml_callout(nodes, next))
                        {
                            content = content.strip_suffix("<!--").unwrap_or(content);
                        }
                        content = strip_callout_guard(content, comment_prefix);
                    }
                    self.write_verbatim_text(content)?;
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
        write!(self.writer_mut(), "{}", manify(text, EscapeMode::Preserve))?;
        Ok(())
    }

    fn write_verbatim_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        let (prefix, content, suffix) = match node.kind() {
            InlineKind::Plain(text) => return self.write_verbatim_text(text.text()),
            InlineKind::Verbatim(text) => return self.write_verbatim_text(text.text()),
            InlineKind::Symbol(symbol) => {
                let replacements = replacements();
                let text = replacements.symbol(symbol.kind());
                let text = if symbol.kind() == SymbolKind::SpacedEmDash
                    && self.text_boundaries.at_paragraph_end()
                {
                    text.strip_suffix(' ').unwrap_or(text)
                } else {
                    text
                };
                self.write_verbatim_text(text)?;
                return Ok(());
            }
            InlineKind::Callout(callout) => {
                write!(self.writer_mut(), "\\fB({})\\fP", callout.number())?;
                return Ok(());
            }
            InlineKind::Bold(text) => ("\\fB", text.content(), "\\fP"),
            InlineKind::Italic(text) => ("\\fI", text.content(), "\\fP"),
            InlineKind::Monospace(text) => ("\\f(CR", text.content(), "\\fP"),
            InlineKind::Highlight(text) => ("", text.content(), ""),
            InlineKind::Superscript(text) => ("", text.content(), ""),
            InlineKind::Subscript(text) => ("", text.content(), ""),
            InlineKind::CurvedQuotation(text) => ("\\(lq", text.content(), "\\(rq"),
            InlineKind::CurvedApostrophe(text) => ("\\(oq", text.content(), "\\(cq"),
            InlineKind::IndexTerm(term) if term.is_visible() => ("", term.term(), ""),
            _ if contains_link(node) => {
                // Link commands need their renderer; flattening keeps only the label.
                return self.render_inline_node(traversal, node);
            }
            InlineKind::Raw(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::LineBreak(_)
            | InlineKind::Anchor(_)
            | InlineKind::Footnote(_)
            | InlineKind::Icon(_)
            | InlineKind::Image(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Button(_)
            | InlineKind::Menu(_)
            | InlineKind::Url(_)
            | InlineKind::Link(_)
            | InlineKind::Mailto(_)
            | InlineKind::Autolink(_)
            | InlineKind::CrossReference(_)
            | InlineKind::Pass(_)
            | InlineKind::Stem(_)
            | InlineKind::IndexTerm(_)
            | _ => {
                let mut text = String::new();
                // Writing into a String cannot fail.
                let _ = InlineTextTransform::default()
                    .line_break("\n")
                    .decode_char_refs(true)
                    .rendered()
                    .text_boundaries(self.text_boundaries)
                    .write_inlines(&mut text, std::iter::once(node));
                write!(self.writer_mut(), "{}", manify(&text, EscapeMode::Preserve))?;
                return Ok(());
            }
        };
        let render = |visitor: &mut Self| visitor.write_verbatim_nodes(traversal, content, None);
        if matches!(
            node.kind(),
            InlineKind::Bold(_) | InlineKind::Italic(_) | InlineKind::Monospace(_)
        ) {
            self.render_font(prefix, false, render)
        } else {
            self.render_affixed(prefix, suffix, false, render)
        }
    }
}

fn is_xml_callout(nodes: ContentRef<'_>, index: usize) -> bool {
    matches!(nodes.inlines().nth(index).map(InlineRef::kind), Some(InlineKind::Callout(_)))
        && index.checked_sub(1).is_some_and(|previous| {
            matches!(nodes.inlines().nth(previous).map(InlineRef::kind), Some(InlineKind::Verbatim(text)) if text.text().ends_with("<!--"))
        })
        && index.checked_add(1).is_some_and(|next| {
            matches!(nodes.inlines().nth(next).map(InlineRef::kind), Some(InlineKind::Verbatim(text)) if text.text().starts_with("-->"))
        })
}

fn strip_callout_guard<'text>(text: &'text str, prefix: Option<&str>) -> &'text str {
    // Try one optional ASCII space; the prefix itself may also end in spaces.
    let candidate = text.strip_suffix(' ').unwrap_or(text);
    if let Some(prefix) = prefix {
        if prefix.is_empty() {
            text
        } else {
            candidate
                .strip_suffix(prefix)
                .or_else(|| text.strip_suffix(prefix))
                .unwrap_or(text)
        }
    } else {
        ["//", "#", "--", ";;"]
            .into_iter()
            .find_map(|prefix| candidate.strip_suffix(prefix))
            .unwrap_or(text)
    }
}
