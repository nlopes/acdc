//! Index catalog rendering for HTML output.
//!
//! Renders a populated index catalog from collected index term entries.
//! Terms are organized alphabetically by first letter, with hierarchical
//! nesting for secondary and tertiary terms. Each occurrence is a back-link
//! whose label is the section it appears in (the HTML analog of a page number);
//! repeats within one section are disambiguated as `Section (2)`, `Section (3)`.
//!
//! NOTE: this is an acdc extension, opt-in via the `:acdc-index:` document
//! attribute. asciidoctor's html5 backend does **not** generate an index — it
//! renders an `[index]` section with an empty body and emits no
//! `<a id="_indexterm_N">` anchors (index generation only happens in `DocBook`
//! output or via extensions such as asciidoctor-pdf). When `:acdc-index:` is
//! unset, acdc matches asciidoctor exactly; when set, acdc emits a back-linked
//! anchor per index-term occurrence (see `inlines::render_indexterm`) and builds
//! the listing below. The `index_catalog*` test fixtures (attribute set)
//! therefore intentionally diverge from asciidoctor; fixtures without the
//! attribute stay byte-identical.

use std::{collections::BTreeMap, io::Write};

use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::{IndexTermKind, Section};

use crate::{Error, HtmlVisitor, IndexTermEntry};

/// A single occurrence of a term: the anchor to jump to, and the title of the
/// section it occurs in (`None` outside any section — falls back to the doc title).
#[derive(Clone, Debug)]
struct Occurrence {
    anchor_id: String,
    section_title: Option<String>,
}

impl Occurrence {
    fn from_entry(entry: &IndexTermEntry) -> Self {
        Self {
            anchor_id: entry.anchor_id.clone(),
            section_title: entry.section_title.clone(),
        }
    }
}

/// Represents a single index entry with all its occurrences.
#[derive(Debug, Default)]
struct IndexEntry {
    /// Occurrences of this term (for linking)
    occurrences: Vec<Occurrence>,
    /// Nested secondary terms (if any)
    secondary: BTreeMap<String, SecondaryEntry>,
}

/// Represents a secondary-level index entry.
#[derive(Debug, Default)]
struct SecondaryEntry {
    /// Occurrences of this secondary term
    occurrences: Vec<Occurrence>,
    /// Nested tertiary terms (if any)
    tertiary: BTreeMap<String, Vec<Occurrence>>,
}

/// Build a hierarchical index structure from collected entries.
fn build_index_structure(entries: &[IndexTermEntry]) -> BTreeMap<String, IndexEntry> {
    let mut index: BTreeMap<String, IndexEntry> = BTreeMap::new();

    for entry in entries {
        let occurrence = Occurrence::from_entry(entry);
        match &entry.kind {
            IndexTermKind::Flow(term) => {
                // Flow terms: primary only
                let primary_entry = index.entry(term.to_string()).or_default();
                primary_entry.occurrences.push(occurrence);
            }
            IndexTermKind::Concealed {
                term,
                secondary,
                tertiary,
            } => {
                let primary_entry = index.entry(term.to_string()).or_default();

                match (secondary, tertiary) {
                    (None, None) => {
                        // Primary term only
                        primary_entry.occurrences.push(occurrence);
                    }
                    (Some(sec), None) => {
                        // Primary + secondary
                        let secondary_entry =
                            primary_entry.secondary.entry(sec.to_string()).or_default();
                        secondary_entry.occurrences.push(occurrence);
                    }
                    (Some(sec), Some(tert)) => {
                        // Primary + secondary + tertiary
                        let secondary_entry =
                            primary_entry.secondary.entry(sec.to_string()).or_default();
                        secondary_entry
                            .tertiary
                            .entry(tert.to_string())
                            .or_default()
                            .push(occurrence);
                    }
                    (None, Some(_)) => {
                        // Invalid: tertiary without secondary - treat as primary only
                        primary_entry.occurrences.push(occurrence);
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

/// Escape text destined for HTML element content / link text.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Render back-links for a term's occurrences, in document order. The link text
/// is the section each occurrence is in (`fallback` when it has none); repeated
/// occurrences within the same section get a `(n)` suffix.
fn render_links(occurrences: &[Occurrence], fallback: &str) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    occurrences
        .iter()
        .map(|occ| {
            let label = occ.section_title.as_deref().unwrap_or(fallback);
            let n = counts.entry(label).or_insert(0);
            *n += 1;
            let text = if *n == 1 {
                escape(label)
            } else {
                format!("{} ({n})", escape(label))
            };
            format!("<a href=\"#{}\">{text}</a>", occ.anchor_id)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Render the index catalog for a section with `[index]` style.
///
/// This generates nested definition lists organized alphabetically by first letter.
pub(crate) fn render<W: Write>(
    _section: &Section,
    visitor: &mut HtmlVisitor<'_, '_, W>,
) -> Result<(), Error> {
    let processor = visitor.processor.clone();
    let entries = processor.index_entries().borrow();

    if entries.is_empty() {
        // No index terms - render empty section like asciidoctor
        return Ok(());
    }

    // Label for occurrences outside any section (e.g. the preamble).
    let fallback = processor
        .document_attributes()
        .get_string("doctitle")
        .map_or_else(|| "top".to_string(), std::borrow::Cow::into_owned);

    let index = build_index_structure(&entries);
    let grouped = group_by_letter(index);

    let w = visitor.writer_mut();

    for (letter, terms) in &grouped {
        // Letter heading
        writeln!(w, "<h3 class=\"indexletter\">{letter}</h3>")?;
        writeln!(w, "<dl class=\"indexterms\">")?;

        for (term, entry) in terms {
            writeln!(w, "<dt>{}", escape(term))?;
            if !entry.occurrences.is_empty() {
                write!(w, " {}", render_links(&entry.occurrences, &fallback))?;
            }
            writeln!(w, "</dt>")?;

            if !entry.secondary.is_empty() {
                writeln!(w, "<dd>")?;
                writeln!(w, "<dl class=\"indexterms-secondary\">")?;

                for (secondary, sec_entry) in &entry.secondary {
                    writeln!(w, "<dt>{}", escape(secondary))?;
                    if !sec_entry.occurrences.is_empty() {
                        write!(w, " {}", render_links(&sec_entry.occurrences, &fallback))?;
                    }
                    writeln!(w, "</dt>")?;

                    if !sec_entry.tertiary.is_empty() {
                        writeln!(w, "<dd>")?;
                        writeln!(w, "<dl class=\"indexterms-tertiary\">")?;

                        for (tertiary, occurrences) in &sec_entry.tertiary {
                            writeln!(w, "<dt>{}", escape(tertiary))?;
                            if !occurrences.is_empty() {
                                write!(w, " {}", render_links(occurrences, &fallback))?;
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
