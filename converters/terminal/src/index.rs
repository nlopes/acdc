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

use crate::{Error, IndexTermEntry, IndexTermLabel, Processor};

/// Represents a single primary index entry with nested sub-entries.
#[derive(Debug, Default)]
struct IndexEntry {
    /// Nested secondary terms (if any)
    secondary: BTreeMap<IndexTermLabel, SecondaryEntry>,
}

/// Represents a secondary-level index entry.
#[derive(Debug, Default)]
struct SecondaryEntry {
    /// Nested tertiary terms (if any)
    tertiary: BTreeSet<IndexTermLabel>,
}

/// Build a hierarchical index structure from collected term kinds.
fn build_index_structure(entries: &[IndexTermEntry]) -> BTreeMap<IndexTermLabel, IndexEntry> {
    let mut index: BTreeMap<IndexTermLabel, IndexEntry> = BTreeMap::new();

    for entry in entries {
        let primary_entry = index.entry(entry.primary.clone()).or_default();
        match (&entry.secondary, &entry.tertiary) {
            (Some(secondary), Some(tertiary)) => {
                primary_entry
                    .secondary
                    .entry(secondary.clone())
                    .or_default()
                    .tertiary
                    .insert(tertiary.clone());
            }
            (Some(secondary), None) => {
                primary_entry
                    .secondary
                    .entry(secondary.clone())
                    .or_default();
            }
            (None, _) => {}
        }
    }

    index
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

        for (term, entry) in terms {
            write!(w, "  {}", term.rendered)?;
            writeln!(w)?;

            for (secondary, sec_entry) in &entry.secondary {
                write!(w, "    {}", secondary.rendered)?;
                writeln!(w)?;

                for tertiary in &sec_entry.tertiary {
                    write!(w, "      {}", tertiary.rendered)?;
                    writeln!(w)?;
                }
            }
        }
    }

    Ok(())
}
