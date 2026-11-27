//! Delimited block kinds.
//!
//! Each public kind wraps a crate-private [`Delimited`] with a typed body, so a
//! listing cannot hold blocks and an example cannot hold inline content. Kinds
//! with the same body type bind to the same `Delimited` type, which lets
//! internal code group them in one or-pattern (see [`compound_block`] and
//! [`verbatim_block`]).

use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

use super::{Block, BlockMetadata, InlineNode, StemContent, StemNotation, TableContent, Title};
use crate::{ColumnFormat, Location, ReservedAttribute, TablePresentation, TableRow};

/// Fields shared by every delimited block, with a typed body.
#[derive(Clone, Debug)]
pub(crate) struct Delimited<'a, B> {
    pub(crate) metadata: BlockMetadata<'a>,
    pub(crate) body: B,
    pub(crate) delimiter: &'a str,
    pub(crate) title: Title<'a>,
    pub(crate) location: Location,
    pub(crate) open_delimiter_location: Option<Location>,
    pub(crate) close_delimiter_location: Option<Location>,
    /// The body after document preprocessing, before inline substitutions.
    /// Not part of serialization or semantic equality.
    pub(crate) source_text: Option<&'a str>,
}

impl<B: PartialEq> PartialEq for Delimited<'_, B> {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            metadata,
            body,
            delimiter,
            title,
            location,
            open_delimiter_location,
            close_delimiter_location,
            source_text: _not_semantic,
        } = self;
        *metadata == other.metadata
            && *body == other.body
            && *delimiter == other.delimiter
            && *title == other.title
            && *location == other.location
            && *open_delimiter_location == other.open_delimiter_location
            && *close_delimiter_location == other.close_delimiter_location
    }
}

impl<'a> Delimited<'a, ()> {
    /// Attach a body to fields assembled before the grammar chose the block's kind.
    pub(crate) fn with_body<B>(self, body: B) -> Delimited<'a, B> {
        let Self {
            metadata,
            body: (),
            delimiter,
            title,
            location,
            open_delimiter_location,
            close_delimiter_location,
            source_text,
        } = self;
        Delimited {
            metadata,
            body,
            delimiter,
            title,
            location,
            open_delimiter_location,
            close_delimiter_location,
            source_text,
        }
    }
}

/// Behavior that depends on a delimited block's body type.
pub(crate) trait DelimitedBody {
    /// React to replaced block metadata. Tables re-resolve local decoration.
    fn metadata_changed(&mut self, _metadata: &BlockMetadata<'_>) {}
}

impl DelimitedBody for Vec<Block<'_>> {}
impl DelimitedBody for Vec<InlineNode<'_>> {}
impl DelimitedBody for StemContent<'_> {}

impl DelimitedBody for TableContent<'_> {
    fn metadata_changed(&mut self, metadata: &BlockMetadata<'_>) {
        self.refresh_presentation(metadata);
    }
}

impl<'a, B> Delimited<'a, B> {
    pub(crate) fn new(body: B, delimiter: &'a str, location: Location) -> Self {
        Self {
            metadata: BlockMetadata::default(),
            body,
            delimiter,
            title: Title::default(),
            location,
            open_delimiter_location: None,
            close_delimiter_location: None,
            source_text: None,
        }
    }
}

impl<'a, B: DelimitedBody> Delimited<'a, B> {
    fn set_metadata(&mut self, metadata: BlockMetadata<'a>) -> Result<(), ReservedAttribute> {
        metadata.validate_attributes(&["title"])?;
        self.body.metadata_changed(&metadata);
        self.metadata = metadata;
        Ok(())
    }

    fn take_metadata(&mut self) -> BlockMetadata<'a> {
        let metadata = std::mem::take(&mut self.metadata);
        self.body.metadata_changed(&self.metadata);
        metadata
    }

    /// Serialize the shared fields around a body written by `body`.
    fn serialize_with<S: Serializer>(
        &self,
        serializer: S,
        name: &str,
        body: impl FnOnce(&mut S::SerializeMap) -> Result<(), S::Error>,
    ) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", name)?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("form", "delimited")?;
        state.serialize_entry("delimiter", &self.delimiter)?;
        if !self.metadata.is_empty_for_serialization() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        body(&mut state)?;
        if self.title.is_present() {
            state.serialize_entry("title", &self.title)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

/// Implement the methods every delimited kind shares.
///
/// The kinds are declared as ordinary structs so the ASG model audit, which
/// reads declarations with `syn`, sees them.
macro_rules! delimited_kind {
    (
        $kind:ident as $variant:ident, name = $name:literal, delimiter = $delimiter:literal
        $(, serialize = $key:literal)?
    ) => {
        impl<'a> $kind<'a> {
            #[doc = concat!("The delimiter used when none is set: `", $delimiter, "`.")]
            pub const DEFAULT_DELIMITER: &'static str = $delimiter;

            /// The body after document preprocessing, before inline substitutions.
            ///
            /// Delimiters and metadata are excluded; a newline before the closing delimiter
            /// is retained. Absent for programmatically constructed or synthetic blocks.
            /// This text is not part of ASG serialization or semantic equality.
            #[must_use]
            pub fn source_text(&self) -> Option<&'a str> {
                self.0.source_text
            }

            /// Replace metadata without shadowing dedicated fields.
            ///
            /// # Errors
            /// Rejects scalar attributes for dedicated metadata or title fields.
            pub fn with_metadata(mut self, metadata: BlockMetadata<'a>) -> Result<Self, ReservedAttribute> {
                self.set_metadata(metadata)?;
                Ok(self)
            }

            /// Replace metadata without shadowing dedicated fields.
            ///
            /// # Errors
            /// Rejects scalar attributes for dedicated metadata or title fields,
            /// leaving the block unchanged.
            pub fn set_metadata(&mut self, metadata: BlockMetadata<'a>) -> Result<(), ReservedAttribute> {
                self.0.set_metadata(metadata)
            }

            /// Copy metadata to prepare a checked replacement without copying the body.
            #[must_use]
            pub fn clone_metadata(&self) -> BlockMetadata<'a> {
                self.0.metadata.clone()
            }

            /// Remove metadata, leaving it empty.
            #[must_use]
            pub fn take_metadata(&mut self) -> BlockMetadata<'a> {
                self.0.take_metadata()
            }

            /// Set the title.
            #[must_use]
            pub fn with_title(mut self, title: Title<'a>) -> Self {
                self.set_title(title);
                self
            }

            /// Replace the title, including an explicitly empty value.
            pub fn set_title(&mut self, title: Title<'a>) {
                self.0.title = title;
            }

            /// Remove the title, leaving it absent.
            #[must_use]
            pub fn take_title(&mut self) -> Title<'a> {
                std::mem::take(&mut self.0.title)
            }

            /// The delimiter spelling, such as `====` or `!===`.
            #[must_use]
            pub fn delimiter(&self) -> &'a str {
                self.0.delimiter
            }

            /// Replace delimiter spelling without reparsing the body or relocating its spans.
            pub fn set_delimiter(&mut self, delimiter: &'a str) {
                self.0.delimiter = delimiter;
            }

            /// Replace delimiter spans without relocating the body or the block span.
            pub fn set_delimiter_locations(&mut self, open: Option<Location>, close: Option<Location>) {
                self.0.open_delimiter_location = open;
                self.0.close_delimiter_location = close;
            }

            /// Replace the block span without relocating its body or delimiters.
            pub fn set_location(&mut self, location: Location) {
                self.0.location = location;
            }
        }

        impl<'a> From<$kind<'a>> for Block<'a> {
            fn from(block: $kind<'a>) -> Self {
                Self::$variant(block)
            }
        }

        $(
            impl Serialize for $kind<'_> {
                fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                    self.0.serialize_with(serializer, $name, |state| {
                        state.serialize_entry($key, &self.0.body)
                    })
                }
            }
        )?
    };
}

/// Define a delimited kind whose body is child blocks.
macro_rules! compound_kind {
    ($kind:ident as $variant:ident, name = $name:literal, delimiter = $delimiter:literal) => {
        delimited_kind! {
            $kind as $variant, name = $name, delimiter = $delimiter, serialize = "blocks"
        }

        impl<'a> $kind<'a> {
            /// Create a block with the default delimiter.
            #[must_use]
            pub fn new(blocks: Vec<Block<'a>>, location: Location) -> Self {
                Self(Delimited::new(blocks, $delimiter, location))
            }

            /// Append a prepared child block without reinterpreting existing children.
            pub fn push_block(&mut self, block: Block<'a>) {
                self.0.body.push(block);
            }

            /// Replace prepared child blocks in content order.
            pub fn set_blocks(&mut self, blocks: impl IntoIterator<Item = Block<'a>>) {
                self.0.body = blocks.into_iter().collect();
            }

            /// Remove child blocks, retaining their source lifetime.
            pub fn take_blocks(
                &mut self,
            ) -> impl DoubleEndedIterator<Item = Block<'a>> + ExactSizeIterator + use<'a> {
                std::mem::take(&mut self.0.body).into_iter()
            }
        }
    };
}

/// Define a delimited kind whose body is inline content.
macro_rules! inline_kind {
    ($kind:ident as $variant:ident, name = $name:literal, delimiter = $delimiter:literal, serialize = $key:literal) => {
        delimited_kind! {
            $kind as $variant, name = $name, delimiter = $delimiter, serialize = $key
        }

        impl<'a> $kind<'a> {
            /// Create a block with the default delimiter.
            #[must_use]
            pub fn new(content: Vec<InlineNode<'a>>, location: Location) -> Self {
                Self(Delimited::new(content, $delimiter, location))
            }

            /// Append prepared inline content without reapplying substitutions.
            pub fn push_inline(&mut self, inline: InlineNode<'a>) {
                self.0.body.push(inline);
            }

            /// Replace prepared inline content without reapplying substitutions.
            pub fn set_content(&mut self, content: impl IntoIterator<Item = InlineNode<'a>>) {
                self.0.body = content.into_iter().collect();
            }

            /// Remove inline content, retaining each inline's source lifetime.
            pub fn take_content(
                &mut self,
            ) -> impl DoubleEndedIterator<Item = InlineNode<'a>> + ExactSizeIterator + use<'a> {
                std::mem::take(&mut self.0.body).into_iter()
            }
        }
    };
}

/// An example block (`====`) containing child blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct Example<'a>(pub(crate) Delimited<'a, Vec<Block<'a>>>);

compound_kind!(
    Example as DelimitedExample,
    name = "example",
    delimiter = "===="
);

/// An open block (`--`) containing child blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct Open<'a>(pub(crate) Delimited<'a, Vec<Block<'a>>>);

compound_kind!(Open as DelimitedOpen, name = "open", delimiter = "--");

/// A quote block (`____`) containing child blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct Quote<'a>(pub(crate) Delimited<'a, Vec<Block<'a>>>);

compound_kind!(Quote as DelimitedQuote, name = "quote", delimiter = "____");

/// A sidebar block (`****`) containing child blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct Sidebar<'a>(pub(crate) Delimited<'a, Vec<Block<'a>>>);

compound_kind!(
    Sidebar as DelimitedSidebar,
    name = "sidebar",
    delimiter = "****"
);

/// A listing block (`----`), typically source code.
#[derive(Clone, Debug, PartialEq)]
pub struct Listing<'a>(pub(crate) Delimited<'a, Vec<InlineNode<'a>>>);

inline_kind!(
    Listing as DelimitedListing,
    name = "listing",
    delimiter = "----",
    serialize = "inlines"
);

/// A literal block (`....`) rendered with its whitespace preserved.
#[derive(Clone, Debug, PartialEq)]
pub struct Literal<'a>(pub(crate) Delimited<'a, Vec<InlineNode<'a>>>);

inline_kind!(
    Literal as DelimitedLiteral,
    name = "literal",
    delimiter = "....",
    serialize = "inlines"
);

/// A passthrough block (`++++`) passed to the output without substitutions.
#[derive(Clone, Debug, PartialEq)]
pub struct Passthrough<'a>(pub(crate) Delimited<'a, Vec<InlineNode<'a>>>);

inline_kind!(
    Passthrough as DelimitedPassthrough,
    name = "pass",
    delimiter = "++++",
    serialize = "inlines"
);

/// A verse block (`____` with the `verse` style) that preserves line breaks.
#[derive(Clone, Debug, PartialEq)]
pub struct Verse<'a>(pub(crate) Delimited<'a, Vec<InlineNode<'a>>>);

inline_kind!(
    Verse as DelimitedVerse,
    name = "verse",
    delimiter = "____",
    serialize = "inlines"
);

// The retained comment body keeps its historical `blocks` key in serialized output.
/// A delimited comment block (`////`), which is not rendered.
#[derive(Clone, Debug, PartialEq)]
pub struct DelimitedComment<'a>(pub(crate) Delimited<'a, Vec<InlineNode<'a>>>);

inline_kind!(
    DelimitedComment as DelimitedComment,
    name = "comment",
    delimiter = "////",
    serialize = "blocks"
);

/// A STEM block containing a formula in LaTeX or `AsciiMath` notation.
#[derive(Clone, Debug, PartialEq)]
pub struct StemBlock<'a>(pub(crate) Delimited<'a, StemContent<'a>>);

delimited_kind!(
    StemBlock as DelimitedStem,
    name = "stem",
    delimiter = "++++"
);

impl<'a> StemBlock<'a> {
    /// Create a STEM block with the default delimiter.
    #[must_use]
    pub fn new(text: &'a str, notation: StemNotation, location: Location) -> Self {
        Self(Delimited::new(
            StemContent::new(text, notation),
            "++++",
            location,
        ))
    }

    /// The prepared formula text.
    #[must_use]
    pub fn text(&self) -> &'a str {
        self.0.body.content
    }

    /// The stored formula notation.
    #[must_use]
    pub fn notation(&self) -> StemNotation {
        self.0.body.notation.clone()
    }

    /// Replace protected formula text and return its previous value.
    pub fn replace_text(&mut self, text: &'a str) -> &'a str {
        std::mem::replace(&mut self.0.body.content, text)
    }

    /// Replace the notation without translating the formula text.
    pub fn replace_notation(&mut self, notation: StemNotation) -> StemNotation {
        std::mem::replace(&mut self.0.body.notation, notation)
    }
}

impl Serialize for StemBlock<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize_with(serializer, "stem", |state| {
            state.serialize_entry("content", &self.0.body.content)?;
            state.serialize_entry("notation", &self.0.body.notation)
        })
    }
}

/// A table block (`|===`), with its rows and column formats.
#[derive(Clone, Debug, PartialEq)]
pub struct Table<'a>(pub(crate) Delimited<'a, TableContent<'a>>);

delimited_kind!(
    Table as DelimitedTable,
    name = "table",
    delimiter = "|===",
    serialize = "content"
);

impl<'a> Table<'a> {
    /// Create a table with the default delimiter.
    ///
    /// `location` is used for both the block span and the delimited span.
    #[must_use]
    pub fn new(rows: Vec<TableRow<'a>>, location: Location) -> Self {
        Self(Delimited::new(
            TableContent::new(rows, location.clone()),
            "|===",
            location,
        ))
    }

    /// Set the header row.
    #[must_use]
    pub fn with_header(mut self, header: Option<TableRow<'a>>) -> Self {
        self.0.body.header = header;
        self
    }

    /// Set the footer row.
    #[must_use]
    pub fn with_footer(mut self, footer: Option<TableRow<'a>>) -> Self {
        self.0.body.footer = footer;
        self
    }

    /// Set the column format specifications.
    #[must_use]
    pub fn with_columns(mut self, columns: Vec<ColumnFormat>) -> Self {
        self.0.body.columns = columns;
        self
    }

    /// Table decoration that overrides converter defaults, if present.
    #[must_use]
    pub const fn presentation(&self) -> Option<TablePresentation> {
        self.0.body.presentation()
    }

    /// Replace the header, preserving a present empty row.
    pub fn set_header(&mut self, header: Option<TableRow<'a>>) {
        self.0.body.header = header;
    }

    /// Remove the header without moving a body row into its place.
    #[must_use]
    pub fn take_header(&mut self) -> Option<TableRow<'a>> {
        self.0.body.header.take()
    }

    /// Replace the footer, preserving a present empty row.
    pub fn set_footer(&mut self, footer: Option<TableRow<'a>>) {
        self.0.body.footer = footer;
    }

    /// Remove the footer without moving a body row into its place.
    #[must_use]
    pub fn take_footer(&mut self) -> Option<TableRow<'a>> {
        self.0.body.footer.take()
    }

    /// Replace prepared body rows without changing header, footer or presentation.
    pub fn set_rows(&mut self, rows: impl IntoIterator<Item = TableRow<'a>>) {
        self.0.body.rows = rows.into_iter().collect();
    }

    /// Append a body row without recalculating cell positions or spans.
    pub fn push_row(&mut self, row: TableRow<'a>) {
        self.0.body.rows.push(row);
    }

    /// Remove body rows, retaining their content and original source lifetimes.
    pub fn take_rows(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = TableRow<'a>> + ExactSizeIterator + use<'a> {
        std::mem::take(&mut self.0.body.rows).into_iter()
    }

    /// Replace column formats without reinterpreting existing cell content.
    pub fn set_columns(&mut self, columns: impl IntoIterator<Item = ColumnFormat>) {
        self.0.body.columns = columns.into_iter().collect();
    }

    /// Remove column formats without changing cell overrides or resolved content.
    pub fn take_columns(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = ColumnFormat> + ExactSizeIterator + use<> {
        std::mem::take(&mut self.0.body.columns).into_iter()
    }

    /// Replace the span from the opening delimiter through the end, without
    /// relocating cells or the enclosing block.
    pub fn set_delimited_location(&mut self, location: Location) {
        self.0.body.location = location;
    }
}

/// Match the delimited kinds whose body is child blocks, binding their shared
/// `Delimited<'_, Vec<Block>>`.
macro_rules! compound_block {
    ($binding:pat) => {
        $crate::Block::DelimitedExample($crate::model::Example($binding))
            | $crate::Block::DelimitedOpen($crate::model::Open($binding))
            | $crate::Block::DelimitedQuote($crate::model::Quote($binding))
            | $crate::Block::DelimitedSidebar($crate::model::Sidebar($binding))
    };
}

/// Match the delimited kinds whose body is rendered inline content, binding
/// their shared `Delimited<'_, Vec<InlineNode>>`. Delimited comments are
/// matched separately because they are not rendered.
macro_rules! verbatim_block {
    ($binding:pat) => {
        $crate::Block::DelimitedListing($crate::model::Listing($binding))
            | $crate::Block::DelimitedLiteral($crate::model::Literal($binding))
            | $crate::Block::DelimitedPassthrough($crate::model::Passthrough($binding))
            | $crate::Block::DelimitedVerse($crate::model::Verse($binding))
    };
}

/// Match every delimited kind whose body is inline content, including
/// delimited comments, binding their shared `Delimited<'_, Vec<InlineNode>>`.
macro_rules! inline_body_block {
    ($binding:pat) => {
        $crate::Block::DelimitedComment($crate::model::DelimitedComment($binding))
            | $crate::Block::DelimitedListing($crate::model::Listing($binding))
            | $crate::Block::DelimitedLiteral($crate::model::Literal($binding))
            | $crate::Block::DelimitedPassthrough($crate::model::Passthrough($binding))
            | $crate::Block::DelimitedVerse($crate::model::Verse($binding))
    };
}

pub(crate) use {compound_block, inline_body_block, verbatim_block};
