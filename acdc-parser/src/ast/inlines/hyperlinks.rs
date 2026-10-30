use crate::{
    AttributeValue, Autolink, Link, Mailto, Url,
    ast::{AttributesRef, ContentRef, ElementIdRef, LocationRef, SourceRef, TokensRef},
};

use super::{AutolinkRef, InlineKind, InlineRef, LinkRef, MailtoRef, UrlRef};

/// A hyperlink with shared destination and label reads, retaining its source form.
/// Cross-references use separate reads because their targets need document resolution.
///
/// ```
/// use acdc_parser::{Options, parse_inline, ast::{HyperlinkRef, InlineRef}};
///
/// let parsed = parse_inline("https://example.org mailto:team@example.org[,Hello]", &Options::default())?;
/// for link in parsed.inlines().iter().filter_map(|node| InlineRef::from(node).as_hyperlink()) {
///     assert!(link.text().is_empty());
///     if let HyperlinkRef::Mailto(mail) = link {
///         assert_eq!(mail.subject(), Some("Hello"));
///     }
/// }
/// # Ok::<(), acdc_parser::Error>(())
/// ```
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum HyperlinkRef<'doc> {
    Link(LinkRef<'doc>),
    Url(UrlRef<'doc>),
    Mailto(MailtoRef<'doc>),
    Autolink(AutolinkRef<'doc>),
}

impl<'doc> HyperlinkRef<'doc> {
    /// Destination before converter resolution and mail query encoding.
    #[must_use]
    pub fn target(self) -> SourceRef<'doc> {
        match self {
            Self::Link(link) => link.target(),
            Self::Url(link) => link.target(),
            Self::Mailto(link) => link.target(),
            Self::Autolink(link) => link.target(),
        }
    }

    /// Parsed label content. Empty content uses automatic display text;
    /// autolinks always return empty content. Match the source form for display policy.
    #[must_use]
    pub fn text(self) -> ContentRef<'doc> {
        match self {
            Self::Link(link) => link.text(),
            Self::Url(link) => link.text(),
            Self::Mailto(link) => link.text(),
            Self::Autolink(_) => ContentRef::new(&[]),
        }
    }

    /// The source span of the complete hyperlink syntax.
    #[must_use]
    pub fn location(self) -> LocationRef<'doc> {
        match self {
            Self::Link(link) => link.location(),
            Self::Url(link) => link.location(),
            Self::Mailto(link) => link.location(),
            Self::Autolink(link) => link.location(),
        }
    }
}

impl<'doc> InlineRef<'doc> {
    /// Read a hyperlink through its shared interface. Xrefs remain separate.
    #[must_use]
    pub fn as_hyperlink(self) -> Option<HyperlinkRef<'doc>> {
        if let InlineKind::Link(link) = self.kind() {
            Some(link.into())
        } else if let InlineKind::Url(link) = self.kind() {
            Some(link.into())
        } else if let InlineKind::Mailto(link) = self.kind() {
            Some(link.into())
        } else if let InlineKind::Autolink(link) = self.kind() {
            Some(link.into())
        } else {
            None
        }
    }
}

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

        impl<'doc> From<$read<'doc>> for HyperlinkRef<'doc> {
            fn from(link: $read<'doc>) -> Self {
                Self::$owner(link)
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

impl<'doc> AutolinkRef<'doc> {
    /// The detected destination, including `mailto:` for email addresses.
    #[must_use]
    pub fn target(self) -> SourceRef<'doc> {
        SourceRef::from(&self.node.url)
    }

    /// Whether the source used angle brackets.
    #[must_use]
    pub fn is_bracketed(self) -> bool {
        self.node.bracketed
    }

    /// Whether `hide-uri-scheme` was active at this occurrence.
    /// Email display always omits `mailto:`, independently of this setting.
    #[must_use]
    pub fn hides_uri_scheme(self) -> bool {
        self.node.hides_uri_scheme()
    }
}

impl<'doc> From<&'doc Autolink<'doc>> for AutolinkRef<'doc> {
    fn from(owner: &'doc Autolink<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> From<AutolinkRef<'doc>> for HyperlinkRef<'doc> {
    fn from(link: AutolinkRef<'doc>) -> Self {
        Self::Autolink(link)
    }
}
