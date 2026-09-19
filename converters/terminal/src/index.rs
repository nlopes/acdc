//! Index catalog rendering for terminal output.
//!
//! Renders an alphabetized index of collected index terms, organized
//! by first letter with hierarchical nesting for secondary and tertiary terms.

use std::collections::{BTreeMap, BTreeSet};

use acdc_converters_core::visitor::WritableVisitor;
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Error, IndexCatalogRelationship, IndexTermEntry, IndexTermLabel, Processor};

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

/// Build a hierarchical index structure from collected entries.
fn build_index_structure(entries: &[IndexTermEntry]) -> BTreeMap<IndexTermLabel, IndexEntry> {
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

/// Group index entries by their first letter (case-insensitive).
fn group_by_letter(
    index: BTreeMap<IndexTermLabel, IndexEntry>,
) -> BTreeMap<char, BTreeMap<IndexTermLabel, IndexEntry>> {
    let mut grouped: BTreeMap<char, BTreeMap<IndexTermLabel, IndexEntry>> = BTreeMap::new();

    for (term, entry) in index {
        let first_char = term
            .plain
            .chars()
            .next()
            .map_or('@', |c| c.to_ascii_uppercase());
        let category = if first_char.is_ascii_alphabetic() {
            first_char
        } else {
            '@'
        };
        grouped.entry(category).or_default().insert(term, entry);
    }

    grouped
}

/// Render the index catalog for an `[index]` section.
pub(crate) fn render<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    processor: &Processor<'_>,
) -> Result<(), Error> {
    let entries = processor.index_entries.borrow();

    if entries.is_empty() {
        return Ok(());
    }

    let index = build_index_structure(&entries);
    let grouped = group_by_letter(index);

    let w = visitor.writer_mut();

    for (letter, terms) in &grouped {
        // Letter heading (bold + colored)
        writeln!(w)?;
        w.queue(PrintStyledContent(
            letter
                .to_string()
                .bold()
                .with(processor.appearance.colors.section_h3),
        ))?;
        writeln!(w)?;

        render_entries(w, terms, 1)?;
    }

    Ok(())
}

fn render_entries<W: std::io::Write + ?Sized>(
    w: &mut W,
    entries: &BTreeMap<IndexTermLabel, IndexEntry>,
    depth: usize,
) -> Result<(), Error> {
    for (term, entry) in entries {
        write!(w, "{}{}", "  ".repeat(depth), term.rendered)?;
        if let Some(IndexRelationship::See(target)) = &entry.relationship {
            write!(w, " (see {})", target.rendered)?;
        }
        writeln!(w)?;

        if let Some(IndexRelationship::SeeAlso(targets)) = &entry.relationship {
            for target in targets {
                writeln!(
                    w,
                    "{}(see also {})",
                    "  ".repeat(depth + 1),
                    target.rendered
                )?;
            }
        }
        render_entries(w, &entry.children, depth + 1)?;
    }
    Ok(())
}
