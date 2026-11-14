use crate::{DocumentAttributes, SourceLocation, Substitution, ast::DocumentAttributesRef};

/// Title input retained by an extension parse before inline substitutions.
///
/// Source is after document preprocessing and excludes the title marker or
/// attribute quotes. The attribute snapshot and substitution order describe the
/// title itself, independently of the block body's `subs=` setting. For attribute
/// titles, attribute-list expansion and dequoting precede these inline stages.
#[derive(Clone, Debug)]
pub struct TitleSource<'doc> {
    pub(crate) source: &'doc str,
    pub(crate) substitutions: Vec<Substitution>,
    pub(crate) attributes: DocumentAttributes<'doc>,
    pub(crate) location: SourceLocation,
}

impl<'doc> TitleSource<'doc> {
    #[must_use]
    pub const fn raw_source(&self) -> &'doc str {
        self.source
    }

    /// The stages applied to the title, in execution order.
    #[must_use]
    pub fn substitutions(&self) -> &[Substitution] {
        &self.substitutions
    }

    /// Document attributes in effect when this title was interpreted.
    #[must_use]
    pub fn attributes(&self) -> DocumentAttributesRef<'_> {
        DocumentAttributesRef::from(&self.attributes)
    }

    #[must_use]
    pub const fn source_location(&self) -> &SourceLocation {
        &self.location
    }
}
