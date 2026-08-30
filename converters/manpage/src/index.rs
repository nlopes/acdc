//! Index catalog rendering for manpages.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Write, sink},
};

use acdc_converters_core::{
    InlineTextTransform,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{IndexTerm, IndexTermRelationship, InlineNode};

use crate::{
    Error, IndexCatalogRelationship, IndexTermEntry, IndexTermLabel, ManpageVisitor, Processor,
    manpage_visitor::IndexCollection,
};

#[derive(Debug, Default)]
struct IndexEntry {
    children: BTreeMap<IndexTermLabel, IndexEntry>,
    relationship: Option<IndexRelationship>,
}

#[derive(Debug)]
enum IndexRelationship {
    See(IndexTermLabel),
    SeeAlso(BTreeSet<IndexTermLabel>),
}

impl IndexEntry {
    fn merge_relationship(&mut self, relationship: &IndexCatalogRelationship) {
        match relationship {
            IndexCatalogRelationship::None => {}
            IndexCatalogRelationship::See(target) => {
                if !matches!(self.relationship, Some(IndexRelationship::See(_))) {
                    self.relationship = Some(IndexRelationship::See(target.clone()));
                }
            }
            IndexCatalogRelationship::SeeAlso(targets) => match &mut self.relationship {
                Some(IndexRelationship::See(_)) => {}
                Some(IndexRelationship::SeeAlso(existing)) => {
                    existing.extend(targets.iter().cloned());
                }
                None => {
                    self.relationship = Some(IndexRelationship::SeeAlso(
                        targets.iter().cloned().collect(),
                    ));
                }
            },
        }
    }
}

fn build_index(entries: &[IndexTermEntry]) -> BTreeMap<IndexTermLabel, IndexEntry> {
    let mut index: BTreeMap<IndexTermLabel, IndexEntry> = BTreeMap::new();
    for entry in entries {
        let mut target = index.entry(entry.primary.clone()).or_default();
        if let Some(secondary) = &entry.secondary {
            target = target.children.entry(secondary.clone()).or_default();
        }
        if let Some(tertiary) = &entry.tertiary {
            target = target.children.entry(tertiary.clone()).or_default();
        }
        target.merge_relationship(&entry.relationship);
    }
    index
}

fn group_by_letter(
    index: BTreeMap<IndexTermLabel, IndexEntry>,
) -> BTreeMap<char, BTreeMap<IndexTermLabel, IndexEntry>> {
    let mut grouped: BTreeMap<char, BTreeMap<IndexTermLabel, IndexEntry>> = BTreeMap::new();
    for (term, entry) in index {
        let first = term
            .plain
            .chars()
            .next()
            .map_or('@', |character| character.to_ascii_uppercase());
        let category = if first.is_ascii_alphabetic() {
            first
        } else {
            '@'
        };
        grouped.entry(category).or_default().insert(term, entry);
    }
    grouped
}

fn render_entries<W: Write + ?Sized>(
    writer: &mut W,
    entries: &BTreeMap<IndexTermLabel, IndexEntry>,
) -> Result<(), Error> {
    for (term, entry) in entries {
        write!(writer, "{}", term.rendered)?;
        if let Some(IndexRelationship::See(target)) = &entry.relationship {
            write!(writer, " (see {})", target.rendered)?;
        }
        writeln!(writer)?;
        writeln!(writer, ".br")?;

        if let Some(IndexRelationship::SeeAlso(targets)) = &entry.relationship {
            writeln!(writer, ".RS 4")?;
            for target in targets {
                writeln!(writer, "(see also {})", target.rendered)?;
                writeln!(writer, ".br")?;
            }
            writeln!(writer, ".RE")?;
        }

        if !entry.children.is_empty() {
            writeln!(writer, ".RS 4")?;
            render_entries(writer, &entry.children)?;
            writeln!(writer, ".RE")?;
        }
    }
    Ok(())
}

impl Processor<'_> {
    fn add_index_entry(&self, entry: IndexTermEntry) {
        self.index_entries.borrow_mut().push(entry);
    }
}

impl<W: Write> ManpageVisitor<'_, '_, W> {
    fn render_index_term_label(
        &mut self,
        inlines: &[InlineNode<'_>],
    ) -> Result<IndexTermLabel, Error> {
        let plain = InlineTextTransform::default()
            .references(&self.processor.references)
            .to_string(inlines);
        let mut output = Vec::new();
        {
            let mut visitor = self.nested_visitor(&mut output);
            visitor.index_collection = IndexCollection::Disabled;
            visitor.visit_inline_nodes(inlines)?;
        }
        Ok(IndexTermLabel {
            plain,
            rendered: String::from_utf8_lossy(&output).into_owned(),
        })
    }

    pub(crate) fn render_index_term(&mut self, term: &IndexTerm<'_>) -> Result<(), Error> {
        if self.index_collection == IndexCollection::Enabled
            && self.processor.has_valid_index_section
        {
            let primary = self.render_index_term_label(term.term())?;
            let secondary = term
                .secondary()
                .map(|inlines| self.render_index_term_label(inlines))
                .transpose()?;
            let tertiary = term
                .tertiary()
                .map(|inlines| self.render_index_term_label(inlines))
                .transpose()?;
            let relationship = match term.relationship.as_ref() {
                Some(IndexTermRelationship::See { target }) => {
                    IndexCatalogRelationship::See(self.render_index_term_label(target)?)
                }
                Some(IndexTermRelationship::SeeAlso { targets }) => {
                    IndexCatalogRelationship::SeeAlso(
                        targets
                            .iter()
                            .map(|target| self.render_index_term_label(target))
                            .collect::<Result<_, _>>()?,
                    )
                }
                None | Some(_) => IndexCatalogRelationship::None,
            };
            self.processor.add_index_entry(IndexTermEntry {
                primary,
                secondary,
                tertiary,
                relationship,
            });
        }

        if term.is_visible() {
            let previous = self.index_collection;
            self.index_collection = IndexCollection::Disabled;
            let result = self.visit_inline_nodes(term.term());
            self.index_collection = previous;
            result?;
        }
        Ok(())
    }

    pub(crate) fn collect_index_terms_from_inlines(
        &mut self,
        inlines: &[InlineNode<'_>],
    ) -> Result<(), Error> {
        if !self.processor.has_valid_index_section {
            return Ok(());
        }
        let mut output = sink();
        let mut visitor = self.nested_visitor(&mut output);
        visitor.visit_inline_nodes(inlines)
    }

    pub(crate) fn render_index_catalog(&mut self) -> Result<(), Error> {
        let entries = self.processor.index_entries.borrow().clone();
        if entries.is_empty() {
            return Ok(());
        }

        let grouped = group_by_letter(build_index(&entries));
        let writer = self.writer_mut();
        for (letter, terms) in grouped {
            writeln!(writer, ".SS \"{letter}\"")?;
            writeln!(writer, ".RS 4")?;
            render_entries(writer, &terms)?;
            writeln!(writer, ".RE")?;
        }
        Ok(())
    }
}
