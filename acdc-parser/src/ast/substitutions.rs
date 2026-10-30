use crate::{Substitution, SubstitutionOp, SubstitutionSpec};

/// Retained block substitution settings, before resolution against block defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubstitutionSpecRef<'doc> {
    spec: &'doc SubstitutionSpec,
}

/// How a substitution specification interprets its entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SubstitutionSpecKind {
    /// Authored entries whose first entry selects replacement or modifier behavior.
    Source,
    /// A typed replacement for the block defaults, including an empty replacement.
    Explicit,
    /// Typed operations applied to the block defaults.
    Modifiers,
}

/// One retained substitution entry, without resolving aliases, groups, or modifiers.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SubstitutionEntryRef<'doc> {
    /// Authored text, including unsupported names and retained empty entries.
    Source(&'doc str),
    /// A typed substitution or group in an explicit specification.
    Substitution(Substitution),
    /// A typed operation in a modifier specification.
    Modifier(SubstitutionOp),
}

impl<'doc> SubstitutionSpecRef<'doc> {
    /// The retained form, including for a specification with no entries.
    #[must_use]
    pub fn kind(self) -> SubstitutionSpecKind {
        match self.spec {
            SubstitutionSpec::Source(_) => SubstitutionSpecKind::Source,
            SubstitutionSpec::Explicit(_) => SubstitutionSpecKind::Explicit,
            SubstitutionSpec::Modifiers(_) => SubstitutionSpecKind::Modifiers,
        }
    }

    /// Entries in retained order, including duplicates. Source entries keep their
    /// spelling after attribute expansion and trimming around each entry.
    pub fn entries(self) -> impl Iterator<Item = SubstitutionEntryRef<'doc>> + Send + Sync + 'doc {
        let (source, explicit, modifiers) = match self.spec {
            SubstitutionSpec::Source(entries) => (entries.as_slice(), &[][..], &[][..]),
            SubstitutionSpec::Explicit(entries) => (&[][..], entries.as_slice(), &[][..]),
            SubstitutionSpec::Modifiers(entries) => (&[][..], &[][..], entries.as_slice()),
        };
        source
            .iter()
            .map(|entry| SubstitutionEntryRef::Source(entry.as_str()))
            .chain(
                explicit
                    .iter()
                    .cloned()
                    .map(SubstitutionEntryRef::Substitution),
            )
            .chain(
                modifiers
                    .iter()
                    .cloned()
                    .map(SubstitutionEntryRef::Modifier),
            )
    }

    /// Whether the specification has no retained entries. An empty modifier list
    /// keeps block defaults; an empty explicit list disables all substitutions.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.entries().next().is_none()
    }
}

impl<'doc> From<&'doc SubstitutionSpec> for SubstitutionSpecRef<'doc> {
    fn from(spec: &'doc SubstitutionSpec) -> Self {
        Self { spec }
    }
}
