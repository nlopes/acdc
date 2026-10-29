use crate::{
    AttributeValue, Icon,
    ast::{AttributesRef, ContentRef, ElementIdRef, SourceRef, TokensRef},
};

use super::IconRef;

impl<'doc> IconRef<'doc> {
    /// The icon target before converter resolution.
    #[must_use]
    pub fn target(self) -> SourceRef<'doc> {
        SourceRef::from(&self.node.target)
    }

    /// Explicit alternative content; absence leaves the fallback to the converter.
    #[must_use]
    pub fn alt(self) -> Option<ContentRef<'doc>> {
        self.node.alt.as_ref().map(ContentRef::from)
    }

    /// Hover content supplied by `title=`, including an explicitly empty value.
    #[must_use]
    pub fn tooltip(self) -> Option<ContentRef<'doc>> {
        self.node.tooltip.as_ref().map(ContentRef::from)
    }

    /// The retained ID and its source location.
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

    /// The declared size, without validating it for a particular icon backend.
    #[must_use]
    pub fn size(self) -> Option<&'doc str> {
        if let Some(AttributeValue::String(size)) = self.node.attributes().get("size") {
            Some(size.as_ref())
        } else {
            None
        }
    }

    /// Read the icon's content, metadata, and scalar controls.
    #[must_use]
    pub fn attributes(self) -> AttributesRef<'doc> {
        AttributesRef::icon(self.node)
    }
}

impl<'doc> From<&'doc Icon<'doc>> for IconRef<'doc> {
    fn from(icon: &'doc Icon<'doc>) -> Self {
        Self::new(icon)
    }
}
