//! Construction and detached edits for structural boundary blocks.

use super::{
    Anchor, BlockMetadata, Comment, CommentKind, DiscreteHeader, DocumentAttribute, Location,
    PageBreak, TableOfContents, ThematicBreak, Title,
};

macro_rules! metadata_edits {
    ($($name:ident),+ $(,)?) => {
        $(
            impl<'a> $name<'a> {
                /// Replace metadata without shadowing dedicated fields.
                ///
                /// # Errors
                /// Rejects conflicting scalar attributes without changing the block.
                pub fn set_metadata(
                    &mut self,
                    metadata: BlockMetadata<'a>,
                ) -> Result<(), crate::ReservedAttribute> {
                    metadata.validate_attributes(&[])?;
                    self.metadata = metadata;
                    Ok(())
                }

                /// Attach checked metadata without interpreting its content.
                ///
                /// # Errors
                /// Rejects scalar attributes for dedicated fields.
                pub fn with_metadata(
                    mut self,
                    metadata: BlockMetadata<'a>,
                ) -> Result<Self, crate::ReservedAttribute> {
                    self.set_metadata(metadata)?;
                    Ok(self)
                }

                /// Copy metadata to prepare a checked replacement.
                #[must_use]
                pub fn clone_metadata(&self) -> BlockMetadata<'a> {
                    self.metadata.clone()
                }

                /// Remove metadata without rebuilding document registrations.
                #[must_use]
                pub fn take_metadata(&mut self) -> BlockMetadata<'a> {
                    std::mem::take(&mut self.metadata)
                }
            }
        )+
    };
}

metadata_edits!(DiscreteHeader, PageBreak, TableOfContents);

macro_rules! title_edits {
    ($($name:ident),+ $(,)?) => {
        $(
            impl<'a> $name<'a> {
                /// Replace the prepared title, including an explicitly empty value.
                pub fn set_title(&mut self, title: Title<'a>) {
                    self.title = title;
                }

                /// Attach a prepared title without parsing or registering content.
                #[must_use]
                pub fn with_title(mut self, title: Title<'a>) -> Self {
                    self.set_title(title);
                    self
                }

                /// Remove the title, leaving it absent.
                #[must_use]
                pub fn take_title(&mut self) -> Title<'a> {
                    std::mem::take(&mut self.title)
                }
            }
        )+
    };
}

title_edits!(DiscreteHeader, PageBreak, ThematicBreak);

macro_rules! location_edits {
    ($($name:ident),+ $(,)?) => {
        $(
            impl $name<'_> {
                /// Replace the block span without relocating content or rebuilding registrations.
                pub fn set_location(&mut self, location: Location) {
                    self.location = location;
                }
            }
        )+
    };
}

location_edits!(
    DiscreteHeader,
    PageBreak,
    TableOfContents,
    ThematicBreak,
    Comment,
    DocumentAttribute
);

impl<'a> DiscreteHeader<'a> {
    /// Construct a discrete heading without assigning an ID or registering a target.
    #[must_use]
    pub fn new(title: Title<'a>, level: u8, location: Location) -> Self {
        Self {
            metadata: BlockMetadata::default(),
            title,
            level,
            location,
        }
    }

    /// Replace the heading level without changing document structure or catalogs.
    pub fn set_level(&mut self, level: u8) {
        self.level = level;
    }
}

impl PageBreak<'_> {
    /// Construct a page break with no title or metadata.
    #[must_use]
    pub fn new(location: Location) -> Self {
        Self {
            title: Title::default(),
            metadata: BlockMetadata::default(),
            location,
        }
    }
}

impl TableOfContents<'_> {
    /// Construct a TOC placement without rebuilding its document's registered entries.
    #[must_use]
    pub fn new(location: Location) -> Self {
        Self {
            metadata: BlockMetadata::default(),
            location,
        }
    }
}

impl<'a> ThematicBreak<'a> {
    /// Construct a thematic break with no title or anchors.
    #[must_use]
    pub fn new(location: Location) -> Self {
        Self {
            location,
            ..Self::default()
        }
    }

    /// Replace anchors in their supplied order without rebuilding reference catalogs.
    pub fn set_anchors(&mut self, anchors: impl IntoIterator<Item = Anchor<'a>>) {
        self.anchors = anchors.into_iter().collect();
    }

    /// Append an anchor without rebuilding reference catalogs.
    pub fn push_anchor(&mut self, anchor: Anchor<'a>) {
        self.anchors.push(anchor);
    }

    /// Remove anchors, retaining their content and source lifetimes.
    pub fn take_anchors(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = Anchor<'a>> + ExactSizeIterator + use<'a> {
        std::mem::take(&mut self.anchors).into_iter()
    }
}

impl<'a> Comment<'a> {
    /// Construct a comment without interpreting or normalizing its text.
    #[must_use]
    pub fn new(kind: CommentKind, text: &'a str, location: Location) -> Self {
        Self {
            kind,
            content: text,
            location,
        }
    }

    /// Replace the retained syntactic category without changing text.
    pub fn set_kind(&mut self, kind: CommentKind) {
        self.kind = kind;
    }

    /// Replace comment text and transfer the previous text's source borrow.
    pub fn replace_text(&mut self, text: &'a str) -> &'a str {
        std::mem::replace(&mut self.content, text)
    }
}
