use crate::{Stem, StemNotation};

use super::StemRef;

impl<'doc> StemRef<'doc> {
    /// The protected formula text, without rendering or further substitutions.
    #[must_use]
    pub fn text(self) -> &'doc str {
        self.node.content
    }

    /// The formula's resolved notation, as for a STEM block.
    #[must_use]
    pub fn notation(self) -> StemNotation {
        self.node.notation.clone()
    }
}

impl<'doc> From<&'doc Stem<'doc>> for StemRef<'doc> {
    fn from(owner: &'doc Stem<'doc>) -> Self {
        Self::new(owner)
    }
}
