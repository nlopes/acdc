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

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
};

use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::Section;

use crate::{Error, HtmlVisitor, IndexCatalogRelationship, IndexTermEntry, IndexTermLabel};

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
    occurrences: Vec<Occurrence>,
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
        let occurrence = Occurrence::from_entry(entry);
        let mut target = index.entry(entry.primary.clone()).or_default();
        if let Some(secondary) = &entry.secondary {
            target = target.children.entry(secondary.clone()).or_default();
        }
        if let Some(tertiary) = &entry.tertiary {
            target = target.children.entry(tertiary.clone()).or_default();
        }
        target.occurrences.push(occurrence);
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

fn collect_relationship_targets(
    entries: &BTreeMap<IndexTermLabel, IndexEntry>,
) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    for entry in entries.values() {
        match &entry.relationship {
            Some(IndexRelationship::See(target)) => {
                targets.insert(target.plain.clone());
            }
            Some(IndexRelationship::SeeAlso(related)) => {
                targets.extend(related.iter().map(|target| target.plain.clone()));
            }
            None => {}
        }
        targets.extend(collect_relationship_targets(&entry.children));
    }
    targets
}

fn definition_terms(
    index: &BTreeMap<IndexTermLabel, IndexEntry>,
) -> BTreeMap<String, IndexTermLabel> {
    collect_relationship_targets(index)
        .into_iter()
        .filter_map(|target| {
            index
                .keys()
                .find(|term| term.plain == target)
                .cloned()
                .map(|term| (target, term))
        })
        .collect()
}

fn definition_id(term: &str) -> String {
    let mut id = String::from("_indextermdef_");
    for byte in term.bytes() {
        id.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
        id.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap_or('0'));
    }
    id
}

fn render_relationship_target(
    target: &IndexTermLabel,
    definitions: &BTreeMap<String, IndexTermLabel>,
) -> String {
    if definitions.contains_key(&target.plain) {
        format!(
            "<a href=\"#{}\">{}</a>",
            definition_id(&target.plain),
            target.html
        )
    } else {
        target.html.clone()
    }
}

fn render_entries<W: Write + ?Sized>(
    w: &mut W,
    entries: &BTreeMap<IndexTermLabel, IndexEntry>,
    depth: usize,
    fallback: &str,
    definitions: &BTreeMap<String, IndexTermLabel>,
) -> Result<(), Error> {
    for (term, entry) in entries {
        if depth == 0 && definitions.get(&term.plain) == Some(term) {
            writeln!(w, "<dt id=\"{}\">{}", definition_id(&term.plain), term.html)?;
        } else {
            writeln!(w, "<dt>{}", term.html)?;
        }

        match &entry.relationship {
            Some(IndexRelationship::See(target)) => {
                write!(
                    w,
                    " <span class=\"index-see\">(see {})</span>",
                    render_relationship_target(target, definitions)
                )?;
            }
            Some(IndexRelationship::SeeAlso(_)) | None => {
                if !entry.occurrences.is_empty() {
                    write!(w, " {}", render_links(&entry.occurrences, fallback))?;
                }
            }
        }
        writeln!(w, "</dt>")?;

        let see_also = match &entry.relationship {
            Some(IndexRelationship::SeeAlso(targets)) => Some(targets),
            Some(IndexRelationship::See(_)) | None => None,
        };
        if see_also.is_some() || !entry.children.is_empty() {
            let class = match depth {
                0 => "indexterms-secondary",
                1 => "indexterms-tertiary",
                _ => "indexterms-related",
            };
            writeln!(w, "<dd>")?;
            writeln!(w, "<dl class=\"{class}\">")?;
            if let Some(targets) = see_also {
                for target in targets {
                    writeln!(
                        w,
                        "<dt><span class=\"index-see-also\">(see also {})</span></dt>",
                        render_relationship_target(target, definitions)
                    )?;
                }
            }
            render_entries(w, &entry.children, depth + 1, fallback, definitions)?;
            writeln!(w, "</dl>")?;
            writeln!(w, "</dd>")?;
        }
    }
    Ok(())
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
    let definitions = definition_terms(&index);
    let grouped = group_by_letter(index);

    let w = visitor.writer_mut();

    for (letter, terms) in &grouped {
        // Letter heading
        writeln!(w, "<h3 class=\"indexletter\">{letter}</h3>")?;
        writeln!(w, "<dl class=\"indexterms\">")?;

        render_entries(w, terms, 0, &fallback, &definitions)?;

        writeln!(w, "</dl>")?;
    }

    Ok(())
}
