use crate::{Footnote, ast::ContentRef};

use super::FootnoteRef;

impl<'doc> FootnoteRef<'doc> {
    /// The named footnote key; `None` for an anonymous note.
    #[must_use]
    pub fn id(self) -> Option<&'doc str> {
        self.node.id
    }

    /// The assigned footnote number, shared by references to the same definition.
    #[must_use]
    pub fn number(self) -> u32 {
        self.node.number
    }

    /// The body retained on this occurrence. References, including repeated
    /// definitions, have empty content and do not repeat the first definition's body.
    #[must_use]
    pub fn content(self) -> ContentRef<'doc> {
        ContentRef::new(&self.node.content)
    }
}

impl<'doc> From<&'doc Footnote<'doc>> for FootnoteRef<'doc> {
    fn from(owner: &'doc Footnote<'doc>) -> Self {
        Self::new(owner)
    }
}
