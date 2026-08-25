use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use acdc_pdf_typst::Writer;

#[derive(Debug, Default)]
pub(crate) struct IndexCatalog {
    terms: BTreeMap<TermKey, IndexEntry>,
    next_anchor: usize,
    suspended: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct TermKey {
    folded: String,
    plain: String,
    markup: String,
}

impl TermKey {
    fn new(term: CatalogTerm) -> Self {
        Self {
            folded: term.plain.to_ascii_lowercase(),
            plain: term.plain,
            markup: term.markup,
        }
    }
}

impl Ord for TermKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.folded
            .cmp(&other.folded)
            .then_with(|| self.plain.cmp(&other.plain))
            .then_with(|| self.markup.cmp(&other.markup))
    }
}

impl PartialOrd for TermKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Default)]
struct IndexEntry {
    anchors: Vec<usize>,
    children: BTreeMap<TermKey, IndexEntry>,
    relationship: Option<IndexRelationship>,
}

#[derive(Debug)]
pub(crate) struct CatalogTerm {
    pub(crate) plain: String,
    pub(crate) markup: String,
}

pub(crate) enum CatalogRelationship {
    None,
    See(CatalogTerm),
    SeeAlso(Vec<CatalogTerm>),
}

#[derive(Debug)]
enum IndexRelationship {
    See(TermKey),
    SeeAlso(BTreeSet<TermKey>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageSequenceStyle {
    Term,
    Page,
    Range,
    Print,
}

impl PageSequenceStyle {
    pub(crate) fn from_attributes(value: Option<&str>, media: Option<&str>) -> Self {
        if media.is_some_and(|media| media != "screen") {
            return Self::Print;
        }
        match value {
            Some("page") => Self::Page,
            Some("range") => Self::Range,
            _ => Self::Term,
        }
    }

    const fn typst(self) -> &'static str {
        match self {
            Self::Term => "term",
            Self::Page => "page",
            Self::Range => "range",
            Self::Print => "print",
        }
    }
}

impl IndexCatalog {
    pub(crate) fn set_suspended(&mut self, suspended: bool) -> bool {
        std::mem::replace(&mut self.suspended, suspended)
    }

    pub(crate) const fn is_suspended(&self) -> bool {
        self.suspended
    }

    pub(crate) fn add(
        &mut self,
        primary: CatalogTerm,
        secondary: Option<CatalogTerm>,
        tertiary: Option<CatalogTerm>,
        relationship: CatalogRelationship,
    ) -> Option<usize> {
        if self.suspended {
            return None;
        }
        self.next_anchor += 1;
        let anchor = self.next_anchor;

        let primary = self.terms.entry(TermKey::new(primary)).or_default();
        let target = match (secondary, tertiary) {
            (Some(secondary), Some(tertiary)) => primary
                .children
                .entry(TermKey::new(secondary))
                .or_default()
                .children
                .entry(TermKey::new(tertiary))
                .or_default(),
            (Some(secondary), None) => primary.children.entry(TermKey::new(secondary)).or_default(),
            (None, _) => primary,
        };
        target.anchors.push(anchor);
        target.merge_relationship(relationship);

        Some(anchor)
    }

    pub(crate) fn write(
        &self,
        writer: &mut Writer,
        sequence_style: PageSequenceStyle,
        columns: usize,
        column_gap_pt: f64,
    ) {
        writer.raw(if sequence_style == PageSequenceStyle::Print {
            INDEX_PRINT_PAGE_HELPER
        } else {
            INDEX_PAGE_HELPER
        });
        if columns > 1 {
            let _ = writeln!(writer, "#columns({columns}, gutter: {column_gap_pt}pt)[");
        }
        let definitions = definition_labels(&self.terms);

        let mut categories: BTreeMap<String, Vec<(&TermKey, &IndexEntry)>> = BTreeMap::new();
        for (term, entry) in &self.terms {
            categories
                .entry(category_for(&term.plain))
                .or_default()
                .push((term, entry));
        }

        for (category_index, (category, terms)) in categories.iter().enumerate() {
            if category_index > 0 {
                writer.raw("#v(0.75em)\n");
            }
            writer.raw("#text(weight: \"bold\")[#text(");
            writer.string_literal(category);
            writer.raw(")]\n#v(0.25em)\n");
            for (term, entry) in terms {
                write_entry(writer, term, entry, 0, sequence_style, &definitions);
            }
        }
        if columns > 1 {
            writer.raw("]\n");
        }
    }
}

impl IndexEntry {
    fn merge_relationship(&mut self, relationship: CatalogRelationship) {
        match relationship {
            CatalogRelationship::None => {}
            CatalogRelationship::See(target) => {
                if !matches!(self.relationship, Some(IndexRelationship::See(_))) {
                    self.relationship = Some(IndexRelationship::See(TermKey::new(target)));
                }
            }
            CatalogRelationship::SeeAlso(targets) => match &mut self.relationship {
                Some(IndexRelationship::See(_)) => {}
                Some(IndexRelationship::SeeAlso(existing)) => {
                    existing.extend(targets.into_iter().map(TermKey::new));
                }
                None => {
                    self.relationship = Some(IndexRelationship::SeeAlso(
                        targets.into_iter().map(TermKey::new).collect(),
                    ));
                }
            },
        }
    }
}

fn category_for(term: &str) -> String {
    term.chars().next().map_or_else(
        || "@".to_owned(),
        |first| {
            if first.is_alphabetic() {
                first.to_uppercase().collect()
            } else {
                "@".to_owned()
            }
        },
    )
}

fn write_entry(
    writer: &mut Writer,
    term: &TermKey,
    entry: &IndexEntry,
    depth: usize,
    sequence_style: PageSequenceStyle,
    definitions: &BTreeMap<String, String>,
) {
    let definition = (depth == 0).then(|| definitions.get(&term.plain)).flatten();
    if let Some(definition) = definition
        && *definition == definition_label(term)
    {
        writer.raw("#metadata(none) <");
        writer.raw(definition);
        writer.raw(">\n");
    }
    if depth > 0 {
        let _ = write!(writer, "#pad(left: {depth} * 1.25em)[");
    }
    writer.raw("#par(hanging-indent: 1em)[");
    writer.raw(&term.markup);
    if !matches!(entry.relationship, Some(IndexRelationship::See(_))) && !entry.anchors.is_empty() {
        writer.raw("#_acdc_index_pages((");
        for anchor in &entry.anchors {
            let _ = write!(writer, "<__indexterm-{anchor}>,");
        }
        let _ = write!(writer, "), \"{}\")", sequence_style.typst());
    }
    if let Some(IndexRelationship::See(target)) = &entry.relationship {
        writer.raw(" (see ");
        write_relationship_target(writer, target, sequence_style, definitions);
        writer.raw(")");
    }
    writer.raw("]");
    if depth > 0 {
        writer.raw("]");
    }
    writer.raw("\n");

    if let Some(IndexRelationship::SeeAlso(targets)) = &entry.relationship {
        for target in targets {
            write_see_also(writer, target, depth + 1, sequence_style, definitions);
        }
    }

    for (child, child_entry) in &entry.children {
        write_entry(
            writer,
            child,
            child_entry,
            depth + 1,
            sequence_style,
            definitions,
        );
    }
}

fn write_see_also(
    writer: &mut Writer,
    target: &TermKey,
    depth: usize,
    sequence_style: PageSequenceStyle,
    definitions: &BTreeMap<String, String>,
) {
    let _ = write!(
        writer,
        "#pad(left: {depth} * 1.25em)[#par(hanging-indent: 1em)[(see also "
    );
    write_relationship_target(writer, target, sequence_style, definitions);
    writer.raw(")]]\n");
}

fn write_relationship_target(
    writer: &mut Writer,
    target: &TermKey,
    sequence_style: PageSequenceStyle,
    definitions: &BTreeMap<String, String>,
) {
    if sequence_style != PageSequenceStyle::Print
        && let Some(definition) = definitions.get(&target.plain)
    {
        writer.raw("#link(<");
        writer.raw(definition);
        writer.raw(">)[");
        writer.raw(&target.markup);
        writer.raw("]");
    } else {
        writer.raw(&target.markup);
    }
}

fn definition_labels(terms: &BTreeMap<TermKey, IndexEntry>) -> BTreeMap<String, String> {
    let mut targets = BTreeSet::new();
    collect_relationship_targets(terms.values(), &mut targets);
    targets
        .into_iter()
        .filter_map(|target| {
            terms
                .keys()
                .find(|term| term.plain == target)
                .map(|term| (target, definition_label(term)))
        })
        .collect()
}

fn collect_relationship_targets<'a>(
    entries: impl Iterator<Item = &'a IndexEntry>,
    targets: &mut BTreeSet<String>,
) {
    for entry in entries {
        match &entry.relationship {
            Some(IndexRelationship::See(target)) => {
                targets.insert(target.plain.clone());
            }
            Some(IndexRelationship::SeeAlso(related)) => {
                targets.extend(related.iter().map(|target| target.plain.clone()));
            }
            None => {}
        }
        collect_relationship_targets(entry.children.values(), targets);
    }
}

fn definition_label(term: &TermKey) -> String {
    let mut label = String::from("__indextermdef-");
    for byte in term.plain.bytes().chain([0]).chain(term.markup.bytes()) {
        let _ = write!(label, "{byte:02x}");
    }
    label
}

const INDEX_PAGE_HELPER: &str = r#"#let _acdc_index_pages(targets, sequence) = context {
  let occurrences = targets.map(target => (target, counter(page).at(target).first()))
  if sequence == "page" or sequence == "range" {
    occurrences = occurrences.dedup(key: occurrence => occurrence.last())
  }
  let linked = occurrence => link(
    occurrence.first(),
    counter(page).display(at: occurrence.first()),
  )
  let pages = if sequence == "range" {
    let ranges = ()
    for occurrence in occurrences {
      if ranges.len() > 0 and occurrence.last() == ranges.last().last().last() + 1 {
        let previous = ranges.pop()
        ranges.push((previous.first(), occurrence))
      } else {
        ranges.push((occurrence, occurrence))
      }
    }
    ranges.map(range => if range.first().last() == range.last().last() {
      linked(range.first())
    } else {
      linked(range.first()) + [-] + linked(range.last())
    })
  } else {
    occurrences.map(linked)
  }
  if pages.len() > 0 {
    [, ] + pages.join[, ]
  }
}
"#;

const INDEX_PRINT_PAGE_HELPER: &str = r"#let _acdc_index_pages(targets, sequence) = context {
  let occurrences = targets
    .map(target => (target, counter(page).at(target).first()))
    .dedup(key: occurrence => occurrence.last())
  let ranges = ()
  for occurrence in occurrences {
    if ranges.len() > 0 and occurrence.last() == ranges.last().last().last() + 1 {
      let previous = ranges.pop()
      ranges.push((previous.first(), occurrence))
    } else {
      ranges.push((occurrence, occurrence))
    }
  }
  let displayed = occurrence => counter(page).display(at: occurrence.first())
  let pages = ranges.map(range => if range.first().last() == range.last().last() {
    displayed(range.first())
  } else {
    displayed(range.first()) + [-] + displayed(range.last())
  })
  if pages.len() > 0 {
    [, ] + pages.join[, ]
  }
}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_style_defaults_to_each_term_occurrence() {
        assert_eq!(
            PageSequenceStyle::from_attributes(None, None),
            PageSequenceStyle::Term
        );
        assert_eq!(
            PageSequenceStyle::from_attributes(Some("unknown"), Some("screen")),
            PageSequenceStyle::Term
        );
        assert_eq!(
            PageSequenceStyle::from_attributes(Some("page"), None),
            PageSequenceStyle::Page
        );
        assert_eq!(
            PageSequenceStyle::from_attributes(Some("range"), Some("screen")),
            PageSequenceStyle::Range
        );
        assert_eq!(
            PageSequenceStyle::from_attributes(Some("page"), Some("print")),
            PageSequenceStyle::Print
        );
        assert_eq!(
            PageSequenceStyle::from_attributes(Some("term"), Some("prepress")),
            PageSequenceStyle::Print
        );
    }

    #[test]
    fn term_keys_sort_case_insensitively_then_by_original_text() {
        let term = |plain: &str| {
            TermKey::new(CatalogTerm {
                plain: plain.to_owned(),
                markup: plain.to_owned(),
            })
        };
        let mut terms = [
            term("apple"),
            term("Zebra"),
            term("Apple"),
            term("animal"),
            term("Animal"),
        ];
        terms.sort();

        assert_eq!(
            terms.map(|term| term.plain),
            ["Animal", "animal", "Apple", "apple", "Zebra"]
        );
    }

    #[test]
    fn categories_use_unicode_letters_and_group_other_terms_under_at() {
        assert_eq!(category_for("éclair"), "É");
        assert_eq!(category_for("βeta"), "Β");
        assert_eq!(category_for("42 tools"), "@");
        assert_eq!(category_for(""), "@");
    }
}
