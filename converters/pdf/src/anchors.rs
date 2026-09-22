use std::collections::{HashMap, HashSet, VecDeque};

use acdc_parser::{Reference, Section, TocEntry};

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
        entries: &[TocEntry<'a>],
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
            let first = references.get(entry.id).is_none_or(|reference| {
                reference.title.as_ref() == Some(&entry.title)
                    && reference.location.end == entry.location.end
                    && reference.location.absolute_end == entry.location.absolute_end
            });
            let label = if first && seen.insert(entry.id) {
                encode_label(entry.id)
            } else {
                anchors.private_label()
            };
            anchors.toc_labels.push(label.clone());
            anchors
                .sections
                .entry(entry.id)
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

    pub(crate) fn section(&mut self, section: &Section<'_>, in_table_cell: bool) -> String {
        let id = section.id();
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
