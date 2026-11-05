use std::collections::{HashMap, HashSet, VecDeque};

use acdc_parser::{
    Reference,
    ast::{ReferenceRef, SectionRef, TocEntryRef},
};

use crate::encode_label;

// Keep public IDs unique while giving each section its own TOC destination.
pub(crate) struct Anchors<'a> {
    emitted: HashSet<String>,
    sections: HashMap<&'a str, VecDeque<String>>,
    pub(crate) toc_labels: Vec<String>,
    next_private: usize,
    pub(crate) suspended: bool,
}

impl<'a> Anchors<'a> {
    pub(crate) fn new(
        entries: impl ExactSizeIterator<Item = TocEntryRef<'a>>,
        references: &HashMap<&'a str, Reference<'a>>,
    ) -> Self {
        let mut anchors = Self {
            emitted: HashSet::new(),
            sections: HashMap::new(),
            toc_labels: Vec::with_capacity(entries.len()),
            next_private: 0,
            suspended: false,
        };
        let mut seen = HashSet::new();
        for entry in entries {
            let first = references.get(entry.id()).is_none_or(|reference| {
                let reference = ReferenceRef::from(reference);
                let reference_location = reference.location();
                let entry_location = entry.location();
                let title = reference.title();
                title.is_some()
                    && title.filter(|_| reference.title_is_present()) == entry.title()
                    && reference_location.end() == entry_location.end()
                    && reference_location.absolute_end() == entry_location.absolute_end()
            });
            let label = if first && seen.insert(entry.id()) {
                encode_label(entry.id())
            } else {
                anchors.private_label()
            };
            anchors.toc_labels.push(label.clone());
            anchors
                .sections
                .entry(entry.id())
                .or_default()
                .push_back(label);
        }
        anchors
    }

    pub(crate) fn claim(&mut self, id: &str) -> Option<String> {
        if self.suspended || !self.emitted.insert(id.to_owned()) {
            return None;
        }
        Some(encode_label(id))
    }

    pub(crate) fn section(&mut self, section: SectionRef<'_>, in_table_cell: bool) -> String {
        let id = section.resolve_id();
        if !in_table_cell
            && let Some(label) = self
                .sections
                .get_mut(id.as_ref())
                .and_then(VecDeque::pop_front)
            && label != encode_label(&id)
        {
            return label;
        }
        self.claim(&id).unwrap_or_else(|| self.private_label())
    }

    fn private_label(&mut self) -> String {
        let label = format!("acdc-section-{}", self.next_private);
        self.next_private += 1;
        label
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use acdc_parser::{Location, Reference, Title, TocEntry};

    use super::Anchors;

    #[test]
    fn constructed_title_presence_keeps_public_destination_selection() {
        for entry_title in [Title::default(), Title::new(Vec::new())] {
            for reference_title in [None, Some(Title::default()), Some(Title::new(Vec::new()))] {
                let matches = reference_title.as_ref() == Some(&entry_title);
                let mut reference = Reference::new(Location::default());
                reference.set_title(reference_title);
                let entry = TocEntry::new("target", entry_title.clone(), 1, Location::default());
                let entries = [entry];
                let references = HashMap::from([("target", reference)]);
                let anchors = Anchors::new(
                    entries.iter().map(acdc_parser::ast::TocEntryRef::from),
                    &references,
                );
                let expected = if matches {
                    crate::encode_label("target")
                } else {
                    String::from("acdc-section-0")
                };
                assert_eq!(anchors.toc_labels, [expected]);
            }
        }
    }
}
