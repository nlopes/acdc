use crate::{
    CrossReference, XrefCaptionLabel, XrefSignifier, XrefStyle,
    ast::{AttributesRef, ContentRef, ElementIdRef, TokensRef},
};

use super::CrossReferenceRef;

impl<'doc> CrossReferenceRef<'doc> {
    /// Effective target after local reference resolution.
    #[must_use]
    pub fn target(self) -> &'doc str {
        self.node.target
    }

    /// Whether the target addresses this document, even if no matching ID exists.
    #[must_use]
    pub fn target_is_local(self) -> bool {
        self.node.target_is_local
    }

    /// Explicit label content. Empty content uses the target's reference text or title.
    #[must_use]
    pub fn text(self) -> ContentRef<'doc> {
        ContentRef::new(&self.node.text)
    }

    /// Style used when there is no explicit label or target reference text.
    #[must_use]
    pub fn xrefstyle(self) -> XrefStyle {
        self.node.xrefstyle
    }

    /// Policy for choosing the label before an automatic caption number.
    #[must_use]
    pub fn caption_label(self) -> XrefCaptionLabel<'doc> {
        self.node.caption_label
    }

    /// Word before a section number, including an explicit empty or omitted value.
    #[must_use]
    pub fn signifier(self) -> XrefSignifier<'doc> {
        self.node.signifier()
    }

    /// Retained occurrence ID and its source location. This does not register an anchor.
    #[must_use]
    pub fn id(self) -> Option<ElementIdRef<'doc>> {
        self.attributes().id()
    }

    /// Role tokens in their retained order.
    #[must_use]
    pub fn roles(self) -> TokensRef<'doc> {
        self.attributes().roles()
    }

    /// Option tokens in their retained order.
    #[must_use]
    pub fn options(self) -> TokensRef<'doc> {
        self.attributes().options()
    }

    /// Identity, roles, options, effective xref style, and scalar controls.
    /// Named `title` and `reftext` are extensions; neither supplies the display label.
    #[must_use]
    pub fn attributes(self) -> AttributesRef<'doc> {
        AttributesRef::cross_reference(self.node)
    }
}

impl<'doc> From<&'doc CrossReference<'doc>> for CrossReferenceRef<'doc> {
    fn from(owner: &'doc CrossReference<'doc>) -> Self {
        Self::new(owner)
    }
}
