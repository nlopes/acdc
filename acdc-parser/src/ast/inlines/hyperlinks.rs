use crate::{
    AttributeValue, Link, Mailto, Url,
    ast::{AttributesRef, ContentRef, ElementIdRef, SourceRef, TokensRef},
};

use super::{LinkRef, MailtoRef, UrlRef};

macro_rules! hyperlink_reads {
    ($owner:ident, $read:ident, $attributes:ident) => {
        impl<'doc> $read<'doc> {
            /// Destination before converter resolution and mail query encoding.
            #[must_use]
            pub fn target(self) -> SourceRef<'doc> {
                SourceRef::from(&self.node.target)
            }

            /// Parsed label content. Empty content uses automatic display text.
            #[must_use]
            pub fn text(self) -> ContentRef<'doc> {
                ContentRef::new(&self.node.text)
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

            /// The declared window, without applying an output backend's fallback.
            #[must_use]
            pub fn window(self) -> Option<&'doc str> {
                match self.node.attributes().get("window") {
                    Some(AttributeValue::String(value)) => Some(value.as_ref()),
                    _ => None,
                }
            }

            /// Tooltip, identity, role/option tokens, scalar controls, and extensions.
            #[must_use]
            pub fn attributes(self) -> AttributesRef<'doc> {
                AttributesRef::$attributes(self.node)
            }
        }

        impl<'doc> From<&'doc $owner<'doc>> for $read<'doc> {
            fn from(owner: &'doc $owner<'doc>) -> Self {
                Self::new(owner)
            }
        }
    };
}

hyperlink_reads!(Link, LinkRef, link);
hyperlink_reads!(Url, UrlRef, url);
hyperlink_reads!(Mailto, MailtoRef, mailto);

impl LinkRef<'_> {
    /// Whether automatic display text omits the URI scheme.
    #[must_use]
    pub fn hides_uri_scheme(self) -> bool {
        self.node.hides_uri_scheme()
    }
}

impl UrlRef<'_> {
    /// Whether automatic display text omits the URI scheme.
    #[must_use]
    pub fn hides_uri_scheme(self) -> bool {
        self.node.hides_uri_scheme()
    }
}

impl<'doc> MailtoRef<'doc> {
    /// The second positional value, before URI encoding. An empty value stays present.
    #[must_use]
    pub fn subject(self) -> Option<&'doc str> {
        self.node.subject
    }

    /// The third positional value, before URI encoding. An empty value stays present.
    #[must_use]
    pub fn body(self) -> Option<&'doc str> {
        self.node.body
    }
}
