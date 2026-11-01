use std::borrow::Cow;

use crate::{
    CommentKind,
    ast::{AnchorRef, SourceRef},
};

use super::{
    AttributesRef, AudioRef, CommentRef, ContentRef, DiscreteHeaderRef, ElementIdRef, PageBreakRef,
    TableOfContentsRef, ThematicBreakRef, TokensRef, VideoRef,
};

macro_rules! metadata_reads {
    ($($name:ident, $owner:ident, $attributes:ident);+ $(;)?) => {
        $(
            impl<'doc> $name<'doc> {
                /// Read this element's retained attributes.
                #[must_use]
                pub fn attributes(self) -> AttributesRef<'doc> {
                    AttributesRef::$attributes(self.node)
                }

                /// The primary ID, if present.
                #[must_use]
                pub fn id(self) -> Option<ElementIdRef<'doc>> {
                    self.attributes().id()
                }

                /// The dedicated ID, when present, followed by the retained anchor list.
                pub fn anchors(self) -> impl Iterator<Item = AnchorRef<'doc>> + Send + Sync + 'doc {
                    self.attributes().anchors()
                }

                /// The retained block style.
                #[must_use]
                pub fn style(self) -> Option<&'doc str> {
                    self.attributes().style()
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
            }

            impl<'doc> From<&'doc crate::$owner<'doc>> for $name<'doc> {
                fn from(owner: &'doc crate::$owner<'doc>) -> Self {
                    Self::new(owner)
                }
            }
        )+
    };
}

metadata_reads!(
    AudioRef, Audio, audio;
    VideoRef, Video, video;
    TableOfContentsRef, TableOfContents, table_of_contents;
    DiscreteHeaderRef, DiscreteHeader, discrete_header;
    PageBreakRef, PageBreak, page_break;
);

macro_rules! title_reads {
    ($($name:ident),+ $(,)?) => {
        $(
            impl<'doc> $name<'doc> {
                /// The title, including an explicitly empty title.
                #[must_use]
                pub fn title(self) -> Option<ContentRef<'doc>> {
                    self.node.title.content()
                }
            }
        )+
    };
}

title_reads!(
    AudioRef,
    VideoRef,
    DiscreteHeaderRef,
    PageBreakRef,
    ThematicBreakRef
);

macro_rules! media_reads {
    ($($name:ident),+ $(,)?) => {
        $(
            impl<'doc> $name<'doc> {
                /// The complete target before converter resolution, without splitting commas.
                #[must_use]
                pub fn source(self) -> SourceRef<'doc> {
                    SourceRef::from(&self.node.source)
                }

                /// Authored caption content, separate from the title and generated counters.
                #[must_use]
                pub fn caption_content(self) -> Option<ContentRef<'doc>> {
                    self.node.metadata.caption_content().map(ContentRef::from)
                }
            }
        )+
    };
}

media_reads!(AudioRef, VideoRef);

/// Copies the video's current values and source locations. The copy can still
/// borrow source text; changing it does not update the original.
impl<'doc> From<VideoRef<'doc>> for crate::Video<'doc> {
    fn from(video: VideoRef<'doc>) -> Self {
        video.node.clone()
    }
}

impl<'doc> DiscreteHeaderRef<'doc> {
    /// Resolve the heading ID from the explicit ID, last anchor, or title.
    ///
    /// Title-derived fallbacks are not assigned or checked for collisions.
    #[must_use]
    pub fn resolve_id(self) -> Cow<'doc, str> {
        crate::Section::explicit_id(&self.node.metadata).map_or_else(
            || {
                Cow::Owned(crate::Section::generate_id_string(
                    &self.node.metadata,
                    &self.node.title,
                ))
            },
            Cow::Borrowed,
        )
    }

    /// The heading's parsed level; discrete headings do not create sections.
    #[must_use]
    pub fn level(self) -> u8 {
        self.node.level
    }
}

impl<'doc> ThematicBreakRef<'doc> {
    /// The retained title and primary ID. Thematic breaks have no scalar attribute map.
    #[must_use]
    pub fn attributes(self) -> AttributesRef<'doc> {
        AttributesRef::thematic_break(self.node)
    }

    /// The first retained anchor's ID, if present.
    #[must_use]
    pub fn id(self) -> Option<ElementIdRef<'doc>> {
        self.attributes().id()
    }

    /// Anchors in their retained order.
    pub fn anchors(self) -> impl Iterator<Item = AnchorRef<'doc>> + Send + Sync + 'doc {
        self.attributes().anchors()
    }
}

impl<'doc> From<&'doc crate::ThematicBreak<'doc>> for ThematicBreakRef<'doc> {
    fn from(owner: &'doc crate::ThematicBreak<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> CommentRef<'doc> {
    /// Whether this retained comment came from a line or a styled paragraph.
    #[must_use]
    pub fn kind(self) -> CommentKind {
        self.node.kind
    }

    /// Retained comment text, without inline parsing or whitespace normalization.
    #[must_use]
    pub fn text(self) -> &'doc str {
        self.node.content
    }
}

impl<'doc> From<&'doc crate::Comment<'doc>> for CommentRef<'doc> {
    fn from(owner: &'doc crate::Comment<'doc>) -> Self {
        Self::new(owner)
    }
}
