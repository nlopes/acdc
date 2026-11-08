//! Ordered diagnostics with collision-checked, scoped duplicate lookup.

use std::{collections::HashMap, hash::BuildHasher, ops::Deref};

use super::{Warning, WarningKind};

/// The vector owns each warning once; the index holds only its fingerprint and
/// position. Inline owners search their own suffix because earlier diagnostics
/// can still be in another substring's coordinates.
#[derive(Debug, Default)]
pub(crate) struct WarningCollection {
    entries: Vec<Warning>,
    index: HashMap<u64, Vec<usize>>,
    deprecated_toc2: usize,
}

impl WarningCollection {
    pub(crate) fn push(&mut self, warning: Warning) {
        let hash = self.index.hasher().hash_one(&warning);
        self.push_hashed(warning, hash);
    }

    pub(crate) fn push_unique_from(&mut self, warning: Warning, start: usize) {
        let hash = self.index.hasher().hash_one(&warning);
        if !self.contains_hashed(&warning, start, hash) {
            self.push_hashed(warning, hash);
        }
    }

    fn contains_hashed(&self, warning: &Warning, start: usize, hash: u64) -> bool {
        self.index.get(&hash).is_some_and(|positions| {
            positions
                .iter()
                .rev()
                .take_while(|&&position| position >= start)
                .any(|&position| self.entries.get(position) == Some(warning))
        })
    }

    fn push_hashed(&mut self, warning: Warning, hash: u64) {
        self.deprecated_toc2 +=
            usize::from(matches!(warning.kind, WarningKind::DeprecatedToc2Attribute));
        self.index.entry(hash).or_default().push(self.entries.len());
        self.entries.push(warning);
    }

    pub(crate) const fn contains_deprecated_toc2(&self) -> bool {
        self.deprecated_toc2 != 0
    }

    pub(crate) fn split_off(&mut self, start: usize) -> Vec<Warning> {
        let removed = self.entries.split_off(start);
        for warning in removed.iter().rev() {
            let hash = self.index.hasher().hash_one(warning);
            if let Some(positions) = self.index.get_mut(&hash) {
                positions.pop();
                if positions.is_empty() {
                    self.index.remove(&hash);
                }
            }
            self.deprecated_toc2 -=
                usize::from(matches!(warning.kind, WarningKind::DeprecatedToc2Attribute));
        }
        removed
    }

    pub(crate) fn into_vec(self) -> Vec<Warning> {
        self.entries
    }
}

impl Deref for WarningCollection {
    type Target = [Warning];

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Position, SourceLocation};

    fn warning(message: &'static str) -> Warning {
        Warning::new(WarningKind::Other(message.into()), None)
    }

    #[test]
    fn duplicates_are_scoped_and_draining_restores_parent_lookup() {
        let mut warnings = WarningCollection::default();
        warnings.push_unique_from(warning("same"), 0);
        let child_start = warnings.len();
        warnings.push_unique_from(warning("same"), child_start);
        warnings.push_unique_from(warning("same"), child_start);
        assert_eq!(warnings.len(), 2);
        let child = warnings.split_off(child_start);
        assert_eq!(child.len(), 1);
        warnings.push_unique_from(warning("same"), 0);
        assert_eq!(warnings.len(), 1);
        warnings.push_unique_from(warning("same"), child_start);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn hash_collisions_still_compare_complete_warnings() {
        let mut warnings = WarningCollection::default();
        warnings.push_hashed(warning("first"), 0);
        assert!(!warnings.contains_hashed(&warning("second"), 0, 0));
        assert!(warnings.contains_hashed(&warning("first"), 0, 0));
        warnings.push_hashed(warning("second"), 0);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn source_files_and_positions_distinguish_warnings() {
        let mut warnings = WarningCollection::default();
        for (file, line) in [("first.adoc", 1), ("second.adoc", 1), ("first.adoc", 2)] {
            for _ in 0..2 {
                warnings.push_unique_from(
                    Warning::new(
                        WarningKind::Other("same".into()),
                        Some(SourceLocation::at_position(
                            Some(file.into()),
                            Position::new(line, 1),
                        )),
                    ),
                    0,
                );
            }
        }
        assert_eq!(warnings.len(), 3);
    }

    #[test]
    fn preprocessor_entries_and_toc2_survive_nested_drains() {
        let mut warnings = WarningCollection::default();
        warnings.push(Warning::new(WarningKind::DeprecatedToc2Attribute, None));
        warnings.push(warning("preprocessor"));
        warnings.push_unique_from(warning("preprocessor"), 0);
        warnings.push(warning("child"));
        warnings.split_off(2);
        assert_eq!(warnings.len(), 2);
        assert!(warnings.contains_deprecated_toc2());
        warnings.split_off(0);
        assert!(!warnings.contains_deprecated_toc2());
        warnings.push_unique_from(warning("preprocessor"), 0);
        assert_eq!(warnings.len(), 1);
    }
}
