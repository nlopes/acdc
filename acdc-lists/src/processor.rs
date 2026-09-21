//! The pass that turns `list-of::` calls into lists.
//!
//! asciidoctor-lists does this in two halves: a block macro that leaves a
//! placeholder behind, and a treeprocessor that later swaps each placeholder
//! for the references it found. acdc has no extension registry, so both halves
//! run here, over the finished AST, between parsing and conversion.
//!
//! The work is ordered rather than done in one sweep because a list names
//! elements that may appear after the macro call: the requested element kinds
//! are gathered first, then every matching element in the document, and only
//! then are the calls replaced.

use std::collections::{BTreeMap, BTreeSet};

use acdc_converters_core::{Diagnostics, Warning, WarningSource};
use acdc_parser::{
    Block, CrossReference, DelimitedBlockType, Document, DocumentArena, InlineMacro, InlineNode,
    LineBreak, Location, Plain, SourceLocation, XrefStyle,
};

use crate::{
    element::{self, Element},
    entry::{self, Entry},
    macro_call,
    walk::{self, Descend},
};

/// Builds lists of figures, tables, and other captioned blocks.
#[derive(Debug)]
pub struct Processor {
    warning_source: WarningSource,
}

impl Default for Processor {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor {
    /// Build a processor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            warning_source: WarningSource::new("lists"),
        }
    }

    /// Replace every `list-of::` call in `document` with the list it asks for.
    ///
    /// A call naming an element kind that acdc does not know is left in place
    /// and reported; a call whose list comes out empty disappears, taking its
    /// section with it when `hide_empty_section` is set.
    ///
    /// `arena` must belong to the same parse as `document`;
    /// [`ParseResult::with_document_mut`](acdc_parser::ParseResult::with_document_mut)
    /// hands out both together.
    pub fn process<'arena>(
        &self,
        document: &mut Document<'arena>,
        arena: &'arena DocumentArena,
        warnings: &mut Vec<Warning>,
    ) {
        let mut diagnostics = Diagnostics::new(&self.warning_source, warnings);
        let wanted = requested(&mut document.blocks, &mut diagnostics);
        if wanted.is_empty() {
            return;
        }
        let entries = entry::collect(document, arena, &wanted);
        replace(&mut document.blocks, &entries);
    }
}

/// The element kinds the document's `list-of::` calls ask for.
///
/// Reporting an unknown kind here keeps it to one warning per call, however
/// many times the tree is walked afterwards.
fn requested(blocks: &mut Vec<Block<'_>>, diagnostics: &mut Diagnostics<'_>) -> BTreeSet<Element> {
    let mut wanted = BTreeSet::new();
    walk::walk_blocks(blocks, &mut |block| {
        let Some(line) = placeholder_line(block) else {
            return Descend::Yes;
        };
        let Some(call) = macro_call::parse(line) else {
            return Descend::Yes;
        };
        match Element::parse(call.element()) {
            Some(element) => {
                wanted.insert(element);
            }
            None => diagnostics.emit(
                Warning::new(
                    diagnostics.source().clone(),
                    format!("`list-of::{}[]` names an unknown element", call.element()),
                )
                .with_advice(element::names_advice())
                .at(SourceLocation::at_location(None, walk::location_of(block))),
            ),
        }
        // A macro call is a leaf: nothing inside a paragraph holds blocks.
        Descend::No
    });
    wanted
}

/// The single line of text a block consists of, when it is a lone paragraph.
///
/// A `list-of::` call reaches acdc as an ordinary paragraph, and only a
/// paragraph that is exactly one run of unformatted text can be one.
fn placeholder_line<'b>(block: &'b Block<'_>) -> Option<&'b str> {
    let Block::Paragraph(paragraph) = block else {
        return None;
    };
    match paragraph.content.as_slice() {
        [InlineNode::PlainText(text)] if !text.content.contains('\n') => Some(text.content),
        _ => None,
    }
}

/// What a container should do once its blocks have been rewritten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    /// Nothing further to do.
    Keep,
    /// A call in these blocks asked for its section to be removed.
    HideSection,
}

/// Rewrite the calls in `blocks`, and in everything nested inside them.
///
/// Returns whether a call directly in `blocks` asked for the section holding
/// them to be removed, which only the owner of that section can do.
fn replace<'a>(
    blocks: &mut Vec<Block<'a>>,
    entries: &BTreeMap<Element, Vec<Entry<'a>>>,
) -> Outcome {
    let mut outcome = Outcome::Keep;
    let mut index = 0;
    while index < blocks.len() {
        if let Some(call) = blocks.get(index).and_then(call_at) {
            let listed = entries
                .get(&call.element)
                .map_or([].as_slice(), Vec::as_slice);
            if listed.is_empty() {
                // An empty list leaves nothing behind, as asciidoctor-lists
                // does; `hide_empty_section` additionally asks the enclosing
                // section to go.
                if call.hide_empty_section {
                    outcome = Outcome::HideSection;
                }
                blocks.remove(index);
                continue;
            }
            if let Some(Block::Paragraph(paragraph)) = blocks.get_mut(index) {
                paragraph.content = render(listed, &paragraph.location);
            }
            index += 1;
            continue;
        }

        if let Some(block) = blocks.get_mut(index)
            && descend(block, entries) == Outcome::HideSection
        {
            blocks.remove(index);
            continue;
        }
        index += 1;
    }
    outcome
}

/// Rewrite the calls nested inside one block.
///
/// Returns `HideSection` only for a section whose own content asked to be
/// hidden, so the request travels exactly one level — from the call to the
/// section around it — and no further. Every other container absorbs it,
/// because `hide_empty_section` is defined for sections alone.
fn descend<'a>(block: &mut Block<'a>, entries: &BTreeMap<Element, Vec<Entry<'a>>>) -> Outcome {
    match block {
        Block::Section(section) => return replace(&mut section.content, entries),
        Block::Admonition(admonition) => {
            replace(&mut admonition.blocks, entries);
        }
        Block::UnorderedList(list) => {
            for item in &mut list.items {
                replace(&mut item.blocks, entries);
            }
        }
        Block::OrderedList(list) => {
            for item in &mut list.items {
                replace(&mut item.blocks, entries);
            }
        }
        Block::CalloutList(list) => {
            for item in &mut list.items {
                replace(&mut item.blocks, entries);
            }
        }
        Block::DescriptionList(list) => {
            for item in &mut list.items {
                replace(&mut item.description, entries);
            }
        }
        Block::DelimitedBlock(delimited) => match &mut delimited.inner {
            DelimitedBlockType::DelimitedExample(blocks)
            | DelimitedBlockType::DelimitedOpen(blocks)
            | DelimitedBlockType::DelimitedSidebar(blocks)
            | DelimitedBlockType::DelimitedQuote(blocks) => {
                replace(blocks, entries);
            }
            DelimitedBlockType::DelimitedTable(table) => {
                let rows = table
                    .header
                    .iter_mut()
                    .chain(table.rows.iter_mut())
                    .chain(table.footer.iter_mut());
                for row in rows {
                    for column in &mut row.columns {
                        replace(&mut column.content, entries);
                    }
                }
            }
            // Verbatim content holds inline nodes, not blocks.
            DelimitedBlockType::DelimitedComment(_)
            | DelimitedBlockType::DelimitedListing(_)
            | DelimitedBlockType::DelimitedLiteral(_)
            | DelimitedBlockType::DelimitedPass(_)
            | DelimitedBlockType::DelimitedVerse(_)
            | DelimitedBlockType::DelimitedStem(_)
            | _ => {}
        },
        // The remaining kinds hold no blocks of their own.
        Block::TableOfContents(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::Paragraph(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_)
        | _ => {}
    }
    Outcome::Keep
}

/// A recognised call on this block, if it is one.
struct Call {
    element: Element,
    hide_empty_section: bool,
}

fn call_at(block: &Block<'_>) -> Option<Call> {
    let line = placeholder_line(block)?;
    let call = macro_call::parse(line)?;
    Some(Call {
        element: Element::parse(call.element())?,
        hide_empty_section: call.flag("hide_empty_section"),
    })
}

/// Build the inline content of one list.
///
/// A captioned element shows its caption as the link and its title after it,
/// which is the shape asciidoctor-lists produces; an element with only a title
/// puts the title in the link. Lines are separated by hard breaks, so the list
/// reads as one block however the backend lays paragraphs out.
fn render<'a>(entries: &[Entry<'a>], location: &Location) -> Vec<InlineNode<'a>> {
    let mut nodes = Vec::with_capacity(entries.len() * 3);
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            nodes.push(InlineNode::LineBreak(LineBreak {
                location: location.clone(),
            }));
        }

        let mut xref = CrossReference::new(entry.id, location.clone());
        if entry.captioned {
            xref.xrefstyle = XrefStyle::Short;
        }
        nodes.push(InlineNode::Macro(InlineMacro::CrossReference(xref)));

        if entry.captioned
            && let Some(title) = &entry.title
        {
            nodes.push(InlineNode::PlainText(Plain {
                content: " ",
                location: location.clone(),
                escaped: false,
            }));
            nodes.extend(title.iter().cloned());
        }
    }
    nodes
}
