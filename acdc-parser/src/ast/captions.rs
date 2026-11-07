use std::num::NonZeroU32;

use crate::{Caption, CaptionKind};

use super::{AttributesRef, ContentRef, DocumentAttributesRef};

/// A caption, without choosing how to display its prefix or title.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum CaptionRef<'doc> {
    /// A category-specific label and its assigned ordinal, if any.
    Numbered {
        /// The category whose counter supplied the ordinal.
        kind: CaptionKind,
        /// The resolved label, including an explicitly empty label.
        label: &'doc str,
        /// The stored ordinal. An untitled block can have no ordinal.
        number: Option<NonZeroU32>,
    },
    /// A literal custom prefix, including an empty prefix.
    Custom(&'doc str),
    /// A custom prefix with parsed inline content, including empty content.
    CustomContent(ContentRef<'doc>),
    /// The target can have a caption, but no prefix applies.
    Unnumbered,
}

impl<'doc> From<&'doc Caption<'doc>> for CaptionRef<'doc> {
    fn from(caption: &'doc Caption<'doc>) -> Self {
        match caption {
            Caption::Numbered {
                kind,
                label,
                number,
            } => Self::Numbered {
                kind: *kind,
                label: label.as_ref(),
                number: *number,
            },
            Caption::Custom(prefix) => Self::Custom(prefix.as_ref()),
            Caption::CustomContent(content) => Self::CustomContent(ContentRef::from(content)),
            Caption::Unnumbered => Self::Unnumbered,
        }
    }
}

impl<'doc> CaptionRef<'doc> {
    /// Read the stored caption, or resolve a fallback for an element without one.
    ///
    /// Stored labels and ordinals stay unchanged. Fallbacks follow
    /// [`Caption::resolve_owned`] with the supplied document snapshot and no
    /// ordinal. Elements that cannot retain captions, and elements without a
    /// stored caption or fallback kind, return `None`.
    #[must_use]
    pub fn resolve(
        attributes: AttributesRef<'doc>,
        document_attributes: DocumentAttributesRef<'doc>,
        fallback: Option<CaptionKind>,
    ) -> Option<Self> {
        Caption::resolve_ref(
            attributes.metadata()?,
            document_attributes.attributes,
            fallback,
        )
    }
}
