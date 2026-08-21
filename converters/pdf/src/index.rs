use std::{cmp::Ordering, collections::BTreeMap, fmt::Write as _};

use acdc_parser::IndexTermKind;
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
    original: String,
}

impl TermKey {
    fn new(value: &str) -> Self {
        Self {
            folded: value.to_ascii_lowercase(),
            original: value.to_owned(),
        }
    }
}

impl Ord for TermKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.folded
            .cmp(&other.folded)
            .then_with(|| self.original.cmp(&other.original))
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageSequenceStyle {
    Term,
    Page,
    Range,
}

impl PageSequenceStyle {
    pub(crate) fn from_attribute(value: Option<&str>) -> Self {
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
        }
    }
}

impl IndexCatalog {
    pub(crate) fn set_suspended(&mut self, suspended: bool) -> bool {
        std::mem::replace(&mut self.suspended, suspended)
    }

    pub(crate) fn add(&mut self, kind: &IndexTermKind<'_>) -> Option<usize> {
        if self.suspended {
            return None;
        }
        self.next_anchor += 1;
        let anchor = self.next_anchor;

        match kind {
            IndexTermKind::Flow(primary) => {
                self.terms
                    .entry(TermKey::new(primary))
                    .or_default()
                    .anchors
                    .push(anchor);
            }
            IndexTermKind::Concealed {
                term,
                secondary,
                tertiary,
            } => {
                let primary = self.terms.entry(TermKey::new(term)).or_default();
                let target = match (secondary, tertiary) {
                    (Some(secondary), Some(tertiary)) => primary
                        .children
                        .entry(TermKey::new(secondary))
                        .or_default()
                        .children
                        .entry(TermKey::new(tertiary))
                        .or_default(),
                    (Some(secondary), None) => {
                        primary.children.entry(TermKey::new(secondary)).or_default()
                    }
                    (None, _) => primary,
                };
                target.anchors.push(anchor);
            }
            _ => {}
        }

        Some(anchor)
    }

    pub(crate) fn write(&self, writer: &mut Writer, sequence_style: PageSequenceStyle) {
        writer.raw(INDEX_PAGE_HELPER);

        let mut categories: BTreeMap<String, Vec<(&TermKey, &IndexEntry)>> = BTreeMap::new();
        for (term, entry) in &self.terms {
            categories
                .entry(category_for(&term.original))
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
                write_entry(writer, term, entry, 0, sequence_style);
            }
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
) {
    if depth > 0 {
        let _ = write!(writer, "#pad(left: {depth} * 1.25em)[");
    }
    writer.raw("#par(hanging-indent: 1em)[");
    writer.raw("#text(");
    writer.string_literal(&term.original);
    writer.raw(")");
    if !entry.anchors.is_empty() {
        writer.raw("#_acdc_index_pages((");
        for anchor in &entry.anchors {
            let _ = write!(writer, "<__indexterm-{anchor}>,");
        }
        let _ = write!(writer, "), \"{}\")", sequence_style.typst());
    }
    writer.raw("]");
    if depth > 0 {
        writer.raw("]");
    }
    writer.raw("\n");

    for (child, child_entry) in &entry.children {
        write_entry(writer, child, child_entry, depth + 1, sequence_style);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_style_defaults_to_each_term_occurrence() {
        assert_eq!(
            PageSequenceStyle::from_attribute(None),
            PageSequenceStyle::Term
        );
        assert_eq!(
            PageSequenceStyle::from_attribute(Some("unknown")),
            PageSequenceStyle::Term
        );
        assert_eq!(
            PageSequenceStyle::from_attribute(Some("page")),
            PageSequenceStyle::Page
        );
        assert_eq!(
            PageSequenceStyle::from_attribute(Some("range")),
            PageSequenceStyle::Range
        );
    }

    #[test]
    fn term_keys_sort_case_insensitively_then_by_original_text() {
        let mut terms = [
            TermKey::new("apple"),
            TermKey::new("Zebra"),
            TermKey::new("Apple"),
            TermKey::new("animal"),
            TermKey::new("Animal"),
        ];
        terms.sort();

        assert_eq!(
            terms.map(|term| term.original),
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
