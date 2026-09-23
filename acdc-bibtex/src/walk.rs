//! Reaching every piece of prose in a document.
//!
//! A citation can appear anywhere text can: in a paragraph, a block or section
//! title, a list item, a table cell, and inside inline formatting in any of
//! them. asciidoctor-bibtex asks Asciidoctor for the blocks whose content
//! model is simple, plus list items, table cells and titles; the same reach is
//! spelled out here, because acdc hands over a tree rather than source lines.

use acdc_parser::{Block, DelimitedBlockType, Document, InlineMacro, InlineNode, Table};

/// Run `visit` over every list of inline nodes in `document`.
///
/// A document carries its footnotes twice: once where they were written, and
/// once in the catalog the backends render the definitions from. Both copies
/// are visited, so a citation inside a footnote is replaced in the note the
/// reader actually sees.
pub(crate) fn document<'a, F>(document: &mut Document<'a>, visit: &mut F)
where
    F: FnMut(&mut Vec<InlineNode<'a>>),
{
    inline_containers(&mut document.blocks, visit);
    for footnote in &mut document.footnotes {
        nodes(&mut footnote.content, visit);
    }
}

/// Run `visit` over every list of inline nodes in `blocks`, in document order.
pub(crate) fn inline_containers<'a, F>(blocks: &mut [Block<'a>], visit: &mut F)
where
    F: FnMut(&mut Vec<InlineNode<'a>>),
{
    for block in blocks.iter_mut() {
        match block {
            Block::Section(section) => {
                title(&mut section.title, visit);
                inline_containers(&mut section.content, visit);
            }
            Block::Paragraph(paragraph) => {
                title(&mut paragraph.title, visit);
                nodes(&mut paragraph.content, visit);
            }
            Block::Admonition(admonition) => {
                title(&mut admonition.title, visit);
                inline_containers(&mut admonition.blocks, visit);
            }
            Block::DiscreteHeader(header) => title(&mut header.title, visit),
            Block::Image(image) => title(&mut image.title, visit),
            Block::Audio(audio) => title(&mut audio.title, visit),
            Block::Video(video) => title(&mut video.title, visit),
            Block::ThematicBreak(rule) => title(&mut rule.title, visit),
            Block::PageBreak(page) => title(&mut page.title, visit),
            Block::UnorderedList(list) => {
                title(&mut list.title, visit);
                for item in &mut list.items {
                    nodes(&mut item.principal, visit);
                    inline_containers(&mut item.blocks, visit);
                }
            }
            Block::OrderedList(list) => {
                title(&mut list.title, visit);
                for item in &mut list.items {
                    nodes(&mut item.principal, visit);
                    inline_containers(&mut item.blocks, visit);
                }
            }
            Block::CalloutList(list) => {
                title(&mut list.title, visit);
                for item in &mut list.items {
                    nodes(&mut item.principal, visit);
                    inline_containers(&mut item.blocks, visit);
                }
            }
            Block::DescriptionList(list) => {
                title(&mut list.title, visit);
                for item in &mut list.items {
                    nodes(&mut item.term, visit);
                    nodes(&mut item.principal_text, visit);
                    inline_containers(&mut item.description, visit);
                }
            }
            Block::DelimitedBlock(delimited) => {
                title(&mut delimited.title, visit);
                match &mut delimited.inner {
                    DelimitedBlockType::DelimitedExample(blocks)
                    | DelimitedBlockType::DelimitedOpen(blocks)
                    | DelimitedBlockType::DelimitedSidebar(blocks)
                    | DelimitedBlockType::DelimitedQuote(blocks) => {
                        inline_containers(blocks, visit);
                    }
                    // A verse keeps its line breaks but is still prose.
                    DelimitedBlockType::DelimitedVerse(content) => nodes(content, visit),
                    DelimitedBlockType::DelimitedTable(table) => cells(table, visit),
                    // Listing, literal, passthrough and stem content is
                    // verbatim: a macro written there is meant to be seen.
                    DelimitedBlockType::DelimitedComment(_)
                    | DelimitedBlockType::DelimitedListing(_)
                    | DelimitedBlockType::DelimitedLiteral(_)
                    | DelimitedBlockType::DelimitedPass(_)
                    | DelimitedBlockType::DelimitedStem(_)
                    | _ => {}
                }
            }
            // Nothing else in a document holds prose of its own.
            Block::TableOfContents(_) | Block::DocumentAttribute(_) | Block::Comment(_) | _ => {}
        }
    }
}

fn cells<'a, F>(table: &mut Table<'a>, visit: &mut F)
where
    F: FnMut(&mut Vec<InlineNode<'a>>),
{
    let rows = table
        .header
        .iter_mut()
        .chain(table.rows.iter_mut())
        .chain(table.footer.iter_mut());
    for row in rows {
        for column in &mut row.columns {
            inline_containers(&mut column.content, visit);
        }
    }
}

fn title<'a, F>(title: &mut acdc_parser::Title<'a>, visit: &mut F)
where
    F: FnMut(&mut Vec<InlineNode<'a>>),
{
    if title.is_empty() {
        return;
    }
    let mut content = std::mem::take(title).into_inlines();
    nodes(&mut content, visit);
    *title = acdc_parser::Title::new(content);
}

/// Visit one list of inline nodes, and everything formatted inside it.
fn nodes<'a, F>(content: &mut Vec<InlineNode<'a>>, visit: &mut F)
where
    F: FnMut(&mut Vec<InlineNode<'a>>),
{
    for node in content.iter_mut() {
        match node {
            InlineNode::BoldText(span) => nodes(&mut span.content, visit),
            InlineNode::ItalicText(span) => nodes(&mut span.content, visit),
            InlineNode::MonospaceText(span) => nodes(&mut span.content, visit),
            InlineNode::HighlightText(span) => nodes(&mut span.content, visit),
            InlineNode::SubscriptText(span) => nodes(&mut span.content, visit),
            InlineNode::SuperscriptText(span) => nodes(&mut span.content, visit),
            InlineNode::CurvedQuotationText(span) => nodes(&mut span.content, visit),
            InlineNode::CurvedApostropheText(span) => nodes(&mut span.content, visit),
            InlineNode::Macro(InlineMacro::Footnote(footnote)) => {
                nodes(&mut footnote.content, visit);
            }
            // Everything else is either plain text the caller looks at
            // itself, or a node with no prose nested inside it.
            InlineNode::PlainText(_)
            | InlineNode::RawText(_)
            | InlineNode::VerbatimText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::Macro(_)
            | InlineNode::CalloutRef(_)
            | _ => {}
        }
    }
    visit(content);
}
