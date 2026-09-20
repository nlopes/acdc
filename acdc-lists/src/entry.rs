//! Collecting the elements a list is built from, and giving them ids.
//!
//! An entry is one line of a generated list: a cross-reference to an element,
//! plus that element's title when the reference shows only its caption. An
//! element earns an entry when it has a title or a caption, which is the test
//! asciidoctor-lists applies.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use acdc_parser::{Anchor, Block, Caption, Document, DocumentArena, Reference, Title};

use crate::{
    element::Element,
    walk::{self, Descend},
};

/// One line of a generated list.
pub(crate) struct Entry<'a> {
    /// The id to link to.
    pub(crate) id: &'a str,
    /// The element's title, when it has one.
    pub(crate) title: Option<Title<'a>>,
    /// Whether the element's caption supplies a prefix such as `Figure 1`.
    ///
    /// When it does, the cross-reference shows the prefix and the title
    /// follows it as text; when it does not, the title is the link text.
    pub(crate) captioned: bool,
}

/// Collect the entries for every requested element kind, in document order.
///
/// Elements that have no id acquire one, both on the block and in the
/// document's reference catalog, so the generated cross-references resolve and
/// the rendered element carries the matching anchor.
pub(crate) fn collect<'a>(
    document: &mut Document<'a>,
    arena: &'a DocumentArena,
    wanted: &BTreeSet<Element>,
) -> BTreeMap<Element, Vec<Entry<'a>>> {
    let mut entries: BTreeMap<Element, Vec<Entry<'a>>> = BTreeMap::new();
    // Ids the pass hands out have to avoid everything already in the catalog,
    // and each other.
    let mut taken: HashSet<&'a str> = document.references.keys().copied().collect();
    let mut new_references = Vec::new();

    walk::walk_blocks(&mut document.blocks, &mut |block| {
        let Some(element) = wanted.iter().copied().find(|kind| kind.matches(block)) else {
            return Descend::Yes;
        };
        let captioned = has_caption_prefix(caption_of(block));
        if walk::title_of(block).is_none() && !captioned {
            return Descend::Yes;
        }

        let ordinal = entries.get(&element).map_or(1, |listed| listed.len() + 1);
        let Some(id) = ensure_id(block, arena, element, ordinal, &mut taken) else {
            return Descend::Yes;
        };
        let title = walk::title_of(block).cloned();
        if !document.references.contains_key(id) {
            new_references.push((
                id,
                Reference::for_target(
                    title.clone(),
                    caption_of(block).cloned(),
                    walk::location_of(block),
                ),
            ));
        }
        entries.entry(element).or_default().push(Entry {
            id,
            title,
            captioned,
        });
        Descend::Yes
    });

    for (id, reference) in new_references {
        document.references.insert(id, reference);
    }
    entries
}

/// The element's resolved caption, when it has metadata at all.
fn caption_of<'m, 'a>(block: &'m Block<'a>) -> Option<&'m Caption<'a>> {
    match block {
        Block::Section(b) => b.metadata.caption.as_ref(),
        Block::DelimitedBlock(b) => b.metadata.caption.as_ref(),
        Block::Admonition(b) => b.metadata.caption.as_ref(),
        Block::Paragraph(b) => b.metadata.caption.as_ref(),
        Block::Image(b) => b.metadata.caption.as_ref(),
        Block::Audio(b) => b.metadata.caption.as_ref(),
        Block::Video(b) => b.metadata.caption.as_ref(),
        Block::UnorderedList(b) => b.metadata.caption.as_ref(),
        Block::OrderedList(b) => b.metadata.caption.as_ref(),
        Block::CalloutList(b) => b.metadata.caption.as_ref(),
        Block::DescriptionList(b) => b.metadata.caption.as_ref(),
        // The remaining kinds carry no caption-bearing metadata.
        Block::TableOfContents(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::Comment(_)
        | _ => None,
    }
}

/// Whether a caption yields a visible prefix.
///
/// A block that had no title when ordinals were assigned carries a caption
/// with no number, which renders as nothing; so does `Unnumbered`.
fn has_caption_prefix(caption: Option<&Caption<'_>>) -> bool {
    match caption {
        Some(Caption::Numbered { number, .. }) => number.is_some(),
        Some(Caption::Custom(prefix)) => !prefix.is_empty(),
        _ => false,
    }
}

/// The element's id, assigning one when it has none.
///
/// asciidoctor-lists assigns a UUID here. A generated id ends up in the
/// output as an anchor and in every link to it, so this uses the element name
/// and its position in the list instead — `image-1`, `table-2` — which is
/// readable, stable between runs, and survives `:reproducible:`.
fn ensure_id<'a>(
    block: &mut Block<'a>,
    arena: &'a DocumentArena,
    element: Element,
    ordinal: usize,
    taken: &mut HashSet<&'a str>,
) -> Option<&'a str> {
    let metadata = walk::metadata_mut(block)?;
    if let Some(anchor) = metadata.id.as_ref().or_else(|| metadata.anchors.first()) {
        return Some(anchor.id);
    }

    let id = unique_id(arena, element, ordinal, taken);
    taken.insert(id);
    metadata.id = Some(Anchor::new(
        id,
        metadata.location.clone().unwrap_or_default(),
    ));
    Some(id)
}

/// Allocate an id that nothing else in the document uses.
fn unique_id<'a>(
    arena: &'a DocumentArena,
    element: Element,
    ordinal: usize,
    taken: &HashSet<&'a str>,
) -> &'a str {
    let base = format!("{}-{ordinal}", element.as_str());
    let mut candidate = arena.alloc_str(&base);
    let mut suffix = 2_usize;
    while taken.contains(candidate) {
        candidate = arena.alloc_str(&format!("{base}-{suffix}"));
        suffix += 1;
    }
    candidate
}
