//! Index catalog rendering for terminal output.
//!
//! Renders an alphabetized index of collected index terms, organized
//! by first letter with hierarchical nesting for secondary and tertiary terms.

use std::collections::{BTreeMap, BTreeSet};

use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::IndexTermKind;
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Error, Processor};

/// Represents a single primary index entry with nested sub-entries.
#[derive(Debug, Default)]
struct IndexEntry {
    /// Nested secondary terms (if any)
    secondary: BTreeMap<String, SecondaryEntry>,
}

/// Represents a secondary-level index entry.
#[derive(Debug, Default)]
struct SecondaryEntry {
    /// Nested tertiary terms (if any)
    tertiary: BTreeSet<String>,
}

/// Build a hierarchical index structure from collected term kinds.
fn build_index_structure(entries: &[IndexTermKind]) -> BTreeMap<String, IndexEntry> {
    let mut index: BTreeMap<String, IndexEntry> = BTreeMap::new();

    for kind in entries {
        match kind {
            IndexTermKind::Flow(term) => {
                index.entry(term.clone()).or_default();
            }
            IndexTermKind::Concealed {
                term,
                secondary,
                tertiary,
            } => {
                let primary_entry = index.entry(term.clone()).or_default();

                match (secondary, tertiary) {
                    (Some(sec), None) => {
                        primary_entry.secondary.entry(sec.clone()).or_default();
                    }
                    (Some(sec), Some(tert)) => {
                        let secondary_entry =
                            primary_entry.secondary.entry(sec.clone()).or_default();
                        secondary_entry.tertiary.insert(tert.clone());
                    }
                    // Primary-only or invalid (tertiary without secondary)
                    (None, _) => {}
                }
            }
            _ => {}
        }
    }

    index
}

/// Group index entries by their first letter (case-insensitive).
fn group_by_letter(
    index: BTreeMap<String, IndexEntry>,
) -> BTreeMap<char, BTreeMap<String, IndexEntry>> {
    let mut grouped: BTreeMap<char, BTreeMap<String, IndexEntry>> = BTreeMap::new();

    for (term, entry) in index {
        let first_char = term.chars().next().map_or('@', |c| c.to_ascii_uppercase());
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
    processor: &Processor,
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
            write!(w, "  {term}")?;
            writeln!(w)?;

            for (secondary, sec_entry) in &entry.secondary {
                write!(w, "    {secondary}")?;
                writeln!(w)?;

                for tertiary in &sec_entry.tertiary {
                    write!(w, "      {tertiary}")?;
                    writeln!(w)?;
                }
            }
        }
    }

    Ok(())
}
