use crate::{Admonition, AdmonitionVariant, StemNotation};

use super::{
    AdmonitionRef, AttributesRef, BlockRef, ContentRef, DelimitedCommentRef, ElementIdRef,
    ExampleRef, ListingRef, LiteralRef, LocationRef, OpenRef, PassthroughRef, QuoteRef, SidebarRef,
    StemBlockRef, TokensRef, VerseRef,
};

macro_rules! block_metadata {
    ($($name:ident => $attributes:ident),+ $(,)?) => {
        $(
            impl<'doc> $name<'doc> {
                /// Read this element's attributes.
                #[must_use]
                pub fn attributes(self) -> AttributesRef<'doc> {
                    AttributesRef::$attributes(self.node)
                }

                /// The title, including an explicitly empty title.
                #[must_use]
                pub fn title(self) -> Option<ContentRef<'doc>> {
                    self.node.title.content()
                }

                /// The primary ID, if present.
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

                /// The retained block style, including an explicitly empty style.
                #[must_use]
                pub fn style(self) -> Option<&'doc str> {
                    self.attributes().style()
                }

                /// The attribution, including an explicitly empty value.
                #[must_use]
                pub fn attribution(self) -> Option<ContentRef<'doc>> {
                    self.node.metadata.attribution.as_ref().map(ContentRef::from)
                }

                /// The citation title, including an explicitly empty value.
                #[must_use]
                pub fn citetitle(self) -> Option<ContentRef<'doc>> {
                    self.node.metadata.citetitle.as_ref().map(ContentRef::from)
                }

                /// Authored caption content, excluding generated labels and counters.
                #[must_use]
                pub fn caption_content(self) -> Option<ContentRef<'doc>> {
                    self.node.metadata.caption_content().map(ContentRef::from)
                }
            }
        )+
    };
}

block_metadata!(
    AdmonitionRef => admonition,
    ExampleRef => delimited,
    OpenRef => delimited,
    SidebarRef => delimited,
    QuoteRef => delimited,
    VerseRef => delimited,
    ListingRef => delimited,
    LiteralRef => delimited,
    PassthroughRef => delimited,
    DelimitedCommentRef => delimited,
    StemBlockRef => delimited,
);

macro_rules! delimited_source {
    ($($name:ident),+ $(,)?) => {
        $(
            impl<'doc> $name<'doc> {
                /// The retained delimiter; empty for a form without one.
                #[must_use]
                pub fn delimiter(self) -> &'doc str {
                    self.node.delimiter
                }

                /// The opening delimiter's source span, if retained.
                #[must_use]
                pub fn open_delimiter_location(self) -> Option<LocationRef<'doc>> {
                    self.node.open_delimiter_location.as_ref().map(LocationRef::from)
                }

                /// The closing delimiter's source span, if retained.
                #[must_use]
                pub fn close_delimiter_location(self) -> Option<LocationRef<'doc>> {
                    self.node.close_delimiter_location.as_ref().map(LocationRef::from)
                }

                /// Body text after document preprocessing, before inline substitutions.
                ///
                /// Excludes metadata and delimiters. Absent when no source text was retained,
                /// including constructed blocks.
                #[must_use]
                pub fn source_text(self) -> Option<&'doc str> {
                    self.node.source_text()
                }
            }
        )+
    };
}

delimited_source!(
    ExampleRef,
    OpenRef,
    SidebarRef,
    QuoteRef,
    VerseRef,
    ListingRef,
    LiteralRef,
    PassthroughRef,
    DelimitedCommentRef,
    StemBlockRef,
);

macro_rules! compound_body {
    ($($name:ident),+ $(,)?) => {
        $(
            impl<'doc> $name<'doc> {
                /// Direct child blocks in content order.
                pub fn blocks(self) -> impl Iterator<Item = BlockRef<'doc>> + Send + Sync + 'doc {
                    self.body.iter().map(BlockRef::from)
                }
            }
        )+
    };
}

compound_body!(ExampleRef, OpenRef, SidebarRef, QuoteRef);

macro_rules! inline_body {
    ($($name:ident),+ $(,)?) => {
        $(
            impl<'doc> $name<'doc> {
                /// Parsed body content, preserving inline kinds and whitespace.
                #[must_use]
                pub fn content(self) -> ContentRef<'doc> {
                    ContentRef::new(self.body)
                }
            }
        )+
    };
}

inline_body!(
    ListingRef,
    LiteralRef,
    VerseRef,
    PassthroughRef,
    DelimitedCommentRef
);

impl<'doc> StemBlockRef<'doc> {
    /// The formula text, without rendering.
    #[must_use]
    pub fn text(self) -> &'doc str {
        self.body.content
    }

    /// The formula's resolved notation.
    #[must_use]
    pub fn notation(self) -> StemNotation {
        self.body.notation.clone()
    }
}

impl<'doc> AdmonitionRef<'doc> {
    /// Direct child blocks in content order.
    pub fn blocks(self) -> impl Iterator<Item = BlockRef<'doc>> + Send + Sync + 'doc {
        self.node.blocks.iter().map(BlockRef::from)
    }

    /// The admonition's category.
    #[must_use]
    pub fn variant(self) -> AdmonitionVariant {
        self.node.variant.clone()
    }
}

impl<'doc> From<&'doc Admonition<'doc>> for AdmonitionRef<'doc> {
    fn from(admonition: &'doc Admonition<'doc>) -> Self {
        Self::new(admonition)
    }
}
