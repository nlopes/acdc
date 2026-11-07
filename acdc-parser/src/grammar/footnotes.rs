use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    convert::Infallible,
};

use crate::{
    Block, Document, Footnote, FootnoteDefinition, Header, InlineMacro, InlineNode, Location,
    grammar::location_walk::{
        try_walk_content_footnote_roots_mut, try_walk_document_inline_nodes_mut,
        try_walk_document_reftext_inlines_mut, try_walk_footnote_roots_mut,
        try_walk_owned_inline_nodes_mut, try_walk_scoped_footnote_roots_mut,
        try_walk_scoped_inline_nodes_mut,
    },
};

/// A prepared footnote cannot be bound without guessing or following a cycle.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum FootnoteRebuildError {
    #[error("footnote {id:?} (number {number}) has no retained definition")]
    MissingDefinition {
        id: Option<String>,
        number: u32,
        location: Location,
    },
    #[error("anonymous footnote number {number} identifies multiple definitions")]
    AmbiguousDefinition { number: u32, location: Location },
    #[error("footnote {id:?} (number {number}) has a cyclic definition")]
    CyclicDefinition {
        id: Option<String>,
        number: u32,
        location: Location,
    },
    #[error("footnote {id:?} (number {number}) belongs to multiple retained scopes")]
    AmbiguousOwner {
        id: Option<String>,
        number: u32,
        location: Location,
    },
    #[error("too many footnote definitions")]
    NumberOverflow { location: Location },
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Key<'a> {
    Named(&'a str),
    Anonymous(u32),
}

#[derive(Clone, Copy)]
enum Registration {
    Visiting,
    Assigned(u32),
}

#[derive(Default)]
struct Bindings<'a> {
    numbers: HashMap<(u32, Option<&'a str>), Option<u32>>,
}

impl<'a> Bindings<'a> {
    fn record(&mut self, old: u32, id: Option<&'a str>, new: u32) {
        self.numbers
            .entry((old, id))
            .and_modify(|value| {
                if *value != Some(new) {
                    *value = None;
                }
            })
            .or_insert(Some(new));
    }

    fn rebind(&self, node: &mut InlineNode<'a>) -> Result<(), FootnoteRebuildError> {
        if let InlineNode::Macro(InlineMacro::Footnote(note)) = node {
            match self.numbers.get(&(note.number, note.id)) {
                Some(Some(number)) => note.number = *number,
                Some(None) => {
                    return Err(FootnoteRebuildError::AmbiguousOwner {
                        id: note.id.map(str::to_owned),
                        number: note.number,
                        location: note.location.clone(),
                    });
                }
                None => {}
            }
        }
        Ok(())
    }
}

struct Rebuilder<'a, 'work> {
    last_number: &'work Cell<u32>,
    bindings: &'work RefCell<Bindings<'a>>,
    named: HashMap<&'a str, FootnoteDefinition<'a>>,
    anonymous: HashMap<u32, Vec<FootnoteDefinition<'a>>>,
    registrations: HashMap<Key<'a>, Registration>,
    definitions: Vec<FootnoteDefinition<'a>>,
}

impl<'a, 'work> Rebuilder<'a, 'work> {
    fn new(last_number: &'work Cell<u32>, bindings: &'work RefCell<Bindings<'a>>) -> Self {
        Self {
            last_number,
            bindings,
            named: HashMap::new(),
            anonymous: HashMap::new(),
            registrations: HashMap::new(),
            definitions: Vec::new(),
        }
    }

    fn collect(&mut self, note: &mut Footnote<'a>, authored: bool) {
        let Some(content) = &mut note.content else {
            return;
        };
        let definition = FootnoteDefinition {
            registration_substitutions: None,
            id: note.id,
            content: content.clone(),
            number: note.number,
            location: note.location.clone(),
        };
        if let Some(id) = note.id {
            if self.named.contains_key(id) {
                return;
            }
            self.named.insert(id, definition);
        } else if authored || !self.anonymous.contains_key(&note.number) {
            self.anonymous
                .entry(note.number)
                .or_default()
                .push(definition);
        } else {
            return;
        }
        try_walk_content_footnote_roots_mut(content, &mut |note| {
            self.collect(note, authored);
            Ok::<_, Infallible>(())
        })
        .unwrap_or_else(|never| match never {});
    }

    fn add_catalog(
        &mut self,
        definition: FootnoteDefinition<'a>,
        authored_anonymous: &HashSet<u32>,
    ) {
        if let Some(id) = definition.id {
            self.named.entry(id).or_insert(definition);
        } else if !authored_anonymous.contains(&definition.number) {
            self.anonymous
                .entry(definition.number)
                .or_default()
                .push(definition);
        }
    }

    fn key(&self, note: &Footnote<'a>) -> Result<Option<Key<'a>>, FootnoteRebuildError> {
        if let Some(id) = note.id {
            return Ok(Some(Key::Named(id)));
        }
        let candidates = self.anonymous.get(&note.number);
        if candidates.is_some_and(|values| values.len() > 1) {
            if note.content.is_some() {
                return Ok(None);
            }
            return Err(FootnoteRebuildError::AmbiguousDefinition {
                number: note.number,
                location: note.location.clone(),
            });
        }
        Ok(Some(Key::Anonymous(note.number)))
    }

    fn definition(
        &self,
        key: Option<Key<'a>>,
        note: &Footnote<'a>,
    ) -> Option<FootnoteDefinition<'a>> {
        match key {
            Some(Key::Named(id)) => self.named.get(id).cloned(),
            Some(Key::Anonymous(number)) => self.anonymous.get(&number)?.first().cloned(),
            None => Some(FootnoteDefinition {
                registration_substitutions: None,
                id: note.id,
                content: note.content.clone()?,
                number: note.number,
                location: note.location.clone(),
            }),
        }
    }

    fn visit(&mut self, note: &mut Footnote<'a>) -> Result<(), FootnoteRebuildError> {
        let key = self.key(note)?;
        if let Some(state) = key.and_then(|key| self.registrations.get(&key)) {
            match *state {
                Registration::Assigned(number) => {
                    self.bindings
                        .borrow_mut()
                        .record(note.number, note.id, number);
                    note.number = number;
                    note.content = None;
                    return Ok(());
                }
                Registration::Visiting => {
                    return Err(FootnoteRebuildError::CyclicDefinition {
                        id: note.id.map(str::to_owned),
                        number: note.number,
                        location: note.location.clone(),
                    });
                }
            }
        }
        let mut definition =
            self.definition(key, note)
                .ok_or_else(|| FootnoteRebuildError::MissingDefinition {
                    id: note.id.map(str::to_owned),
                    number: note.number,
                    location: note.location.clone(),
                })?;
        if let Some(key) = key {
            self.registrations.insert(key, Registration::Visiting);
        }
        try_walk_content_footnote_roots_mut(&mut definition.content, &mut |child| {
            self.visit(child)
        })?;
        let number = self.last_number.get().checked_add(1).ok_or_else(|| {
            FootnoteRebuildError::NumberOverflow {
                location: note.location.clone(),
            }
        })?;
        self.last_number.set(number);
        self.bindings
            .borrow_mut()
            .record(definition.number, definition.id, number);
        self.bindings
            .borrow_mut()
            .record(note.number, note.id, number);
        definition.number = number;
        note.number = number;
        note.content = Some(definition.content.clone());
        note.definition_source = None;
        note.registration_substitutions = None;
        self.definitions.push(definition);
        if let Some(key) = key {
            self.registrations
                .insert(key, Registration::Assigned(number));
        }
        Ok(())
    }

    fn rebind(&self, node: &mut InlineNode<'a>) -> Result<(), FootnoteRebuildError> {
        if let InlineNode::Macro(InlineMacro::Footnote(note)) = node {
            if note.id.is_none()
                && self
                    .anonymous
                    .get(&note.number)
                    .is_some_and(|values| values.len() > 1)
            {
                return Err(FootnoteRebuildError::AmbiguousDefinition {
                    number: note.number,
                    location: note.location.clone(),
                });
            }
            let key = self.key(note)?;
            if let Some(Registration::Assigned(number)) =
                key.and_then(|key| self.registrations.get(&key))
            {
                note.number = *number;
            }
        }
        Ok(())
    }
}

fn repair_owner<'a>(
    header: &mut Option<Header<'a>>,
    blocks: &mut [Block<'a>],
    catalog: &mut Vec<FootnoteDefinition<'a>>,
    last_number: &Cell<u32>,
    bindings: &RefCell<Bindings<'a>>,
) -> Result<(), FootnoteRebuildError> {
    let mut rebuilder = Rebuilder::new(last_number, bindings);
    try_walk_footnote_roots_mut(header, blocks, &mut |note| {
        rebuilder.collect(note, true);
        Ok::<_, Infallible>(())
    })
    .unwrap_or_else(|never| match never {});
    let authored_anonymous = rebuilder.anonymous.keys().copied().collect();
    for definition in catalog.iter().cloned() {
        rebuilder.add_catalog(definition, &authored_anonymous);
    }
    for definition in catalog.iter() {
        let mut content = definition.content.clone();
        try_walk_content_footnote_roots_mut(&mut content, &mut |note| {
            rebuilder.collect(note, false);
            Ok::<_, Infallible>(())
        })
        .unwrap_or_else(|never| match never {});
    }
    try_walk_scoped_footnote_roots_mut(
        header,
        blocks,
        &mut |note| rebuilder.visit(note),
        &mut |cell| {
            let mut definitions = cell.footnotes.take().unwrap_or_default();
            repair_owner(
                &mut None,
                &mut cell.content,
                &mut definitions,
                last_number,
                bindings,
            )?;
            cell.footnotes = Some(definitions);
            Ok(())
        },
    )?;
    let mut definitions = std::mem::take(&mut rebuilder.definitions);
    try_walk_scoped_inline_nodes_mut(header, blocks, &mut definitions, &mut |node| {
        if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node
            && let Some(catalog) = &mut term.catalog
        {
            catalog.try_for_each_label_mut(|nodes| {
                for node in nodes {
                    try_walk_owned_inline_nodes_mut(node, &mut |node| rebuilder.rebind(node))?;
                }
                Ok(())
            })?;
        }
        Ok(())
    })?;
    *catalog = definitions;
    Ok(())
}

impl Document<'_> {
    /// Rebuild footnotes from prepared content without reparsing source.
    ///
    /// Each cell owner is rebuilt independently. Numbers remain unique across the document.
    /// Authored definitions win over stored catalog bodies; the first named body
    /// wins. Surviving references can reuse a catalog definition after its original
    /// occurrence is removed. Anonymous references bind by their old assigned number,
    /// which must identify one definition. Unreferenced catalog entries are removed.
    ///
    /// Metadata, titles and bodies follow stored tree order; nested notes receive
    /// numbers before enclosing notes. The first occurrence receives the selected
    /// body and later occurrences become references. Empty definitions remain present.
    /// Source spans and declaration snapshots stay unchanged. Derived copies are not
    /// counted, but their retained number bindings are refreshed where available.
    /// Rebuild reference/TOC and caption catalogs separately after structural edits.
    ///
    /// # Errors
    /// Missing or ambiguous definitions or owner bindings, cycles, and number
    /// overflow leave the document unchanged.
    pub fn rebuild_footnotes(&mut self) -> Result<(), FootnoteRebuildError> {
        let mut header = self.header.clone();
        let mut blocks = self.blocks.clone();
        let mut footnotes = self.footnotes.clone();
        let last_number = Cell::new(0);
        let bindings = RefCell::new(Bindings::default());
        repair_owner(
            &mut header,
            &mut blocks,
            &mut footnotes,
            &last_number,
            &bindings,
        )?;
        let mut derived = Document {
            toc_entries: self.toc_entries.clone(),
            references: self.references.clone(),
            ..Document::default()
        };
        try_walk_document_inline_nodes_mut(&mut derived, &mut |node| {
            bindings.borrow().rebind(node)
        })?;
        let mut prepared = Document {
            header,
            blocks,
            footnotes,
            ..Document::default()
        };
        try_walk_document_reftext_inlines_mut(&mut prepared, &mut |node| {
            bindings.borrow().rebind(node)
        })?;
        self.header = prepared.header;
        self.blocks = prepared.blocks;
        self.footnotes = prepared.footnotes;
        self.toc_entries = derived.toc_entries;
        self.references = derived.references;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Block, Document, Footnote, IndexTerm, IndexTermKind, InlineMacro, InlineNode, Location,
        Paragraph, Plain, grammar::walk_document_inline_nodes_mut,
    };

    fn reference() -> InlineNode<'static> {
        InlineNode::Macro(InlineMacro::Footnote(Footnote::reference(
            None,
            1,
            Location::default(),
        )))
    }

    #[test]
    fn nested_index_snapshots_rebind_each_anonymous_number_once()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut inner = IndexTerm::new(IndexTermKind::Flow(vec![reference()]), Location::default());
        inner.catalog = Some(Box::new(inner.clone()));
        let mut outer = IndexTerm::new(IndexTermKind::Flow(Vec::new()), Location::default());
        outer.catalog = Some(Box::new(IndexTerm::new(
            IndexTermKind::Flow(vec![InlineNode::Macro(InlineMacro::IndexTerm(Box::new(
                inner,
            )))]),
            Location::default(),
        )));
        let mut document = Document::default();
        let notes = [2, 1].map(|number| {
            InlineNode::Macro(InlineMacro::Footnote(Footnote::new(
                vec![InlineNode::PlainText(Plain::new(
                    "body",
                    Location::default(),
                ))],
                number,
                Location::default(),
            )))
        });
        document.set_blocks([Block::Paragraph(Paragraph::new(
            notes
                .into_iter()
                .chain([InlineNode::Macro(InlineMacro::IndexTerm(Box::new(outer)))])
                .collect(),
            Location::default(),
        ))]);
        document.rebuild_footnotes()?;
        let mut numbers = Vec::new();
        walk_document_inline_nodes_mut(&mut document, &mut |inline| {
            if let InlineNode::Macro(InlineMacro::Footnote(note)) = inline {
                numbers.push(note.number);
            }
        });
        assert_eq!(numbers, [1, 2, 2, 2]);
        Ok(())
    }
}
