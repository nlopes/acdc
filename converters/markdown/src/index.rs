//! Linked index catalog rendering for Markdown output.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
};

use acdc_converters_core::{Converter, visitor::WritableVisitor};

use crate::{Error, IndexCatalogRelationship, IndexTermEntry, IndexTermLabel, Processor};

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

fn escape_link_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn render_relationship_target(
    target: &IndexTermLabel,
    definitions: &BTreeMap<String, IndexTermLabel>,
) -> String {
    if definitions.contains_key(&target.plain) {
        format!(
            "[{}](#{})",
            escape_link_text(&target.plain),
            definition_id(&target.plain)
        )
    } else {
        target.rendered.clone()
    }
}

fn render_occurrence_links(occurrences: &[Occurrence], fallback: &str) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    occurrences
        .iter()
        .map(|occurrence| {
            let label = occurrence.section_title.as_deref().unwrap_or(fallback);
            let count = counts.entry(label).or_insert(0);
            *count += 1;
            let text = if *count == 1 {
                escape_link_text(label)
            } else {
                format!("{} ({count})", escape_link_text(label))
            };
            format!("[{text}](#{})", occurrence.anchor_id)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_entries<W: Write + ?Sized>(
    writer: &mut W,
    entries: &BTreeMap<IndexTermLabel, IndexEntry>,
    depth: usize,
    fallback: &str,
    definitions: &BTreeMap<String, IndexTermLabel>,
) -> Result<(), Error> {
    for (term, entry) in entries {
        write!(writer, "{}- ", "  ".repeat(depth))?;
        if depth == 0 && definitions.get(&term.plain) == Some(term) {
            write!(writer, "<a id=\"{}\"></a>", definition_id(&term.plain))?;
        }
        write!(writer, "{}", term.rendered)?;

        match &entry.relationship {
            Some(IndexRelationship::See(target)) => {
                write!(
                    writer,
                    " — see {}",
                    render_relationship_target(target, definitions)
                )?;
            }
            Some(IndexRelationship::SeeAlso(_)) | None if !entry.occurrences.is_empty() => {
                write!(
                    writer,
                    " — {}",
                    render_occurrence_links(&entry.occurrences, fallback)
                )?;
            }
            Some(IndexRelationship::SeeAlso(_)) | None => {}
        }
        writeln!(writer)?;

        if let Some(IndexRelationship::SeeAlso(targets)) = &entry.relationship {
            for target in targets {
                writeln!(
                    writer,
                    "{}- see also {}",
                    "  ".repeat(depth + 1),
                    render_relationship_target(target, definitions)
                )?;
            }
        }
        render_entries(writer, &entry.children, depth + 1, fallback, definitions)?;
    }
    Ok(())
}

pub(crate) fn render<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    processor: &Processor<'_>,
    heading_level: usize,
) -> Result<(), Error> {
    let entries = processor.index_entries().borrow();
    if entries.is_empty() {
        return Ok(());
    }

    let fallback = processor
        .document_attributes()
        .get_string("doctitle")
        .map_or_else(|| "top".to_owned(), std::borrow::Cow::into_owned);
    let index = build_index_structure(&entries);
    let definitions = definition_terms(&index);
    let grouped = group_by_letter(index);
    let hashes = "#".repeat(heading_level.min(6));
    let writer = visitor.writer_mut();

    for (letter, terms) in grouped {
        writeln!(writer)?;
        writeln!(writer, "{hashes} {letter}")?;
        writeln!(writer)?;
        render_entries(writer, &terms, 0, &fallback, &definitions)?;
    }
    Ok(())
}
