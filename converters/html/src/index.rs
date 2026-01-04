//! Index catalog rendering for HTML output.
//!
//! Renders a populated index catalog from collected index term entries.
//! Terms are organized alphabetically by first letter, with hierarchical
//! nesting for secondary and tertiary terms.

use std::collections::BTreeMap;

use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::{IndexTermKind, Section};

use crate::{Error, IndexTermEntry, Processor};

/// Represents a single index entry with all its occurrences.
#[derive(Debug, Default)]
struct IndexEntry {
    /// Anchor IDs where this term appears (for linking)
    anchors: Vec<String>,
    /// Nested secondary terms (if any)
    secondary: BTreeMap<String, SecondaryEntry>,
}

/// Represents a secondary-level index entry.
#[derive(Debug, Default)]
struct SecondaryEntry {
    /// Anchor IDs where this secondary term appears
    anchors: Vec<String>,
    /// Nested tertiary terms (if any)
    tertiary: BTreeMap<String, Vec<String>>,
}

/// Build a hierarchical index structure from collected entries.
fn build_index_structure(entries: &[IndexTermEntry]) -> BTreeMap<String, IndexEntry> {
    let mut index: BTreeMap<String, IndexEntry> = BTreeMap::new();

    for entry in entries {
        match &entry.kind {
            IndexTermKind::Flow(term) => {
                // Flow terms: primary only
                let primary_entry = index.entry(term.clone()).or_default();
                primary_entry.anchors.push(entry.anchor_id.clone());
            }
            IndexTermKind::Concealed {
                term,
                secondary,
                tertiary,
            } => {
                let primary_entry = index.entry(term.clone()).or_default();

                match (secondary, tertiary) {
                    (None, None) => {
                        // Primary term only
                        primary_entry.anchors.push(entry.anchor_id.clone());
                    }
                    (Some(sec), None) => {
                        // Primary + secondary
                        let secondary_entry =
                            primary_entry.secondary.entry(sec.clone()).or_default();
                        secondary_entry.anchors.push(entry.anchor_id.clone());
                    }
                    (Some(sec), Some(tert)) => {
                        // Primary + secondary + tertiary
                        let secondary_entry =
                            primary_entry.secondary.entry(sec.clone()).or_default();
                        secondary_entry
                            .tertiary
                            .entry(tert.clone())
                            .or_default()
                            .push(entry.anchor_id.clone());
                    }
                    (None, Some(_)) => {
                        // Invalid: tertiary without secondary - treat as primary only
                        primary_entry.anchors.push(entry.anchor_id.clone());
                    }
                }
            }
            // IndexTermKind is non_exhaustive, handle future variants
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

        // Use '@' for non-alphabetic terms (symbols, numbers)
        let category = if first_char.is_ascii_alphabetic() {
            first_char
        } else {
            '@'
        };

        grouped.entry(category).or_default().insert(term, entry);
    }

    grouped
}

/// Render links for a set of anchor IDs.
fn render_links(anchors: &[String]) -> String {
    anchors
        .iter()
        .enumerate()
        .map(|(i, anchor)| format!("<a href=\"#{anchor}\">[{}]</a>", i + 1))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Render the index catalog for a section with `[index]` style.
///
/// This generates nested definition lists organized alphabetically by first letter.
pub(crate) fn render<V: WritableVisitor<Error = Error>>(
    _section: &Section,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    let entries = processor.index_entries().borrow();

    if entries.is_empty() {
        // No index terms - render empty section like asciidoctor
        return Ok(());
    }

    let index = build_index_structure(&entries);
    let grouped = group_by_letter(index);

    let w = visitor.writer_mut();

    for (letter, terms) in &grouped {
        // Letter heading
        writeln!(w, "<h3 class=\"indexletter\">{letter}</h3>")?;
        writeln!(w, "<dl class=\"indexterms\">")?;

        for (term, entry) in terms {
            writeln!(w, "<dt>{term}")?;
            if !entry.anchors.is_empty() {
                write!(w, " {}", render_links(&entry.anchors))?;
            }
            writeln!(w, "</dt>")?;

            if !entry.secondary.is_empty() {
                writeln!(w, "<dd>")?;
                writeln!(w, "<dl class=\"indexterms-secondary\">")?;

                for (secondary, sec_entry) in &entry.secondary {
                    writeln!(w, "<dt>{secondary}")?;
                    if !sec_entry.anchors.is_empty() {
                        write!(w, " {}", render_links(&sec_entry.anchors))?;
                    }
                    writeln!(w, "</dt>")?;

                    if !sec_entry.tertiary.is_empty() {
                        writeln!(w, "<dd>")?;
                        writeln!(w, "<dl class=\"indexterms-tertiary\">")?;

                        for (tertiary, anchors) in &sec_entry.tertiary {
                            writeln!(w, "<dt>{tertiary}")?;
                            if !anchors.is_empty() {
                                write!(w, " {}", render_links(anchors))?;
                            }
                            writeln!(w, "</dt>")?;
                        }

                        writeln!(w, "</dl>")?;
                        writeln!(w, "</dd>")?;
                    }
                }

                writeln!(w, "</dl>")?;
                writeln!(w, "</dd>")?;
            }
        }

        writeln!(w, "</dl>")?;
    }

    Ok(())
}
