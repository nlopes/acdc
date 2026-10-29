use crate::{
    Form,
    ast::{AttributesRef, ContentRef, ElementIdRef, TokensRef},
};

use super::{
    BoldRef, CurvedApostropheRef, CurvedQuotationRef, HighlightRef, ItalicRef, MonospaceRef,
    SubscriptRef, SuperscriptRef,
};

macro_rules! span_reads {
    ($name:ident, $owner:ident, $attributes:ident) => {
        impl<'doc> $name<'doc> {
            /// Read this span's direct inline content.
            #[must_use]
            pub fn content(self) -> ContentRef<'doc> {
                ContentRef::new(&self.node.content)
            }

            /// Whether the source used constrained or unconstrained formatting.
            #[must_use]
            pub fn form(self) -> Form {
                self.node.form.clone()
            }

            /// The retained ID, using the span's location rather than an exact ID range.
            #[must_use]
            pub fn id(self) -> Option<ElementIdRef<'doc>> {
                self.attributes().id()
            }

            /// Role tokens in retained order, including duplicates.
            #[must_use]
            pub fn roles(self) -> TokensRef<'doc> {
                self.attributes().roles()
            }

            /// The span's retained ID and role tokens.
            #[must_use]
            pub fn attributes(self) -> AttributesRef<'doc> {
                AttributesRef::$attributes(self.node)
            }
        }

        impl<'doc> From<&'doc crate::$owner<'doc>> for $name<'doc> {
            fn from(owner: &'doc crate::$owner<'doc>) -> Self {
                Self::new(owner)
            }
        }
    };
}

span_reads!(BoldRef, Bold, bold);
span_reads!(ItalicRef, Italic, italic);
span_reads!(MonospaceRef, Monospace, monospace);
span_reads!(HighlightRef, Highlight, highlight);
span_reads!(SubscriptRef, Subscript, subscript);
span_reads!(SuperscriptRef, Superscript, superscript);
span_reads!(CurvedQuotationRef, CurvedQuotation, curved_quotation);
span_reads!(CurvedApostropheRef, CurvedApostrophe, curved_apostrophe);

impl HighlightRef<'_> {
    /// Whether this is an unadorned mark rather than an attributed generic span.
    ///
    /// A retained ID or role makes it generic, even when the value is empty.
    #[must_use]
    pub fn is_marked(self) -> bool {
        self.node.id.is_none() && self.node.role.is_none()
    }
}
