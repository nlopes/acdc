//! List types for `AsciiDoc` documents.

use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

use super::Block;
use super::anchor::Anchor;
use super::inlines::{CalloutRef, InlineNode};
use super::location::Location;
use super::metadata::BlockMetadata;
use super::title::Title;

pub type ListLevel = u8;

/// A `ListItemCheckedStatus` represents the checked status of a list item.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ListItemCheckedStatus {
    Checked,
    Unchecked,
}

/// A `ListItem` represents a list item in a document.
///
/// List items have principal text (inline content immediately after the marker) and
/// optionally attached blocks (via continuation or nesting). This matches Asciidoctor's
/// AST structure where principal text renders as bare `<p>` and attached blocks render
/// with their full wrapper divs.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ListItem {
    pub level: ListLevel,
    pub marker: String,
    pub checked: Option<ListItemCheckedStatus>,
    /// Principal text - inline content that appears immediately after the list marker
    pub principal: Vec<InlineNode>,
    /// Attached blocks - blocks attached via continuation (+) or nesting
    pub blocks: Vec<Block>,
    pub location: Location,
}

/// A `DescriptionList` represents a description list in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DescriptionList {
    pub title: Title,
    pub metadata: BlockMetadata,
    pub items: Vec<DescriptionListItem>,
    pub location: Location,
}

/// An item in a description list (term + description).
///
/// # Structure
///
/// ```text
/// term:: principal text    <- term, delimiter, principal_text
///        description       <- description (blocks)
/// ```
///
/// # Note on Field Names
///
/// - `description` is **singular** (not `descriptions`) - it holds the block content
///   following the term
/// - `principal_text` is inline content immediately after the delimiter on the same line
///
/// ```
/// # use acdc_parser::DescriptionListItem;
/// fn has_description(item: &DescriptionListItem) -> bool {
///     !item.description.is_empty()  // Note: singular 'description'
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct DescriptionListItem {
    /// Optional anchors (IDs) attached to this item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor>,
    /// The term being defined (inline content before the delimiter).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub term: Vec<InlineNode>,
    /// The delimiter used (`::`, `:::`, `::::`, or `;;`).
    pub delimiter: String,
    /// Inline content immediately after the delimiter on the same line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub principal_text: Vec<InlineNode>,
    /// Block content providing the description (singular, not plural).
    pub description: Vec<Block>,
    pub location: Location,
}

/// A `UnorderedList` represents an unordered list in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct UnorderedList {
    pub title: Title,
    pub metadata: BlockMetadata,
    pub items: Vec<ListItem>,
    pub marker: String,
    pub location: Location,
}

/// An `OrderedList` represents an ordered list in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct OrderedList {
    pub title: Title,
    pub metadata: BlockMetadata,
    pub items: Vec<ListItem>,
    pub marker: String,
    pub location: Location,
}

/// A `CalloutList` represents a callout list in a document.
///
/// Callout lists are used to annotate code blocks with numbered references.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CalloutList {
    pub title: Title,
    pub metadata: BlockMetadata,
    pub items: Vec<CalloutListItem>,
    pub location: Location,
}

/// A `CalloutListItem` represents an item in a callout list.
///
/// Unlike [`ListItem`], callout list items have a structured [`CalloutRef`] that
/// preserves whether the original marker was explicit (`<1>`) or auto-numbered (`<.>`).
///
/// # Example
///
/// ```asciidoc
/// <1> First explanation
/// <.> Auto-numbered explanation
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CalloutListItem {
    /// The callout reference (explicit or auto-numbered).
    pub callout: CalloutRef,
    /// Principal text - inline content that appears after the callout marker.
    pub principal: Vec<InlineNode>,
    /// Attached blocks - blocks attached via continuation (though rarely used for callouts).
    pub blocks: Vec<Block>,
    /// Source location of this item.
    pub location: Location,
}

// =============================================================================
// Serialization
// =============================================================================

macro_rules! impl_list_serialize {
    ($type:ty, $variant:literal, with_marker) => {
        impl Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let mut state = serializer.serialize_map(None)?;
                state.serialize_entry("name", "list")?;
                state.serialize_entry("type", "block")?;
                state.serialize_entry("variant", $variant)?;
                state.serialize_entry("marker", &self.marker)?;
                if !self.title.is_empty() {
                    state.serialize_entry("title", &self.title)?;
                }
                if !self.metadata.is_default() {
                    state.serialize_entry("metadata", &self.metadata)?;
                }
                state.serialize_entry("items", &self.items)?;
                state.serialize_entry("location", &self.location)?;
                state.end()
            }
        }
    };
    ($type:ty, $variant:literal) => {
        impl Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let mut state = serializer.serialize_map(None)?;
                state.serialize_entry("name", "list")?;
                state.serialize_entry("type", "block")?;
                state.serialize_entry("variant", $variant)?;
                if !self.title.is_empty() {
                    state.serialize_entry("title", &self.title)?;
                }
                if !self.metadata.is_default() {
                    state.serialize_entry("metadata", &self.metadata)?;
                }
                state.serialize_entry("items", &self.items)?;
                state.serialize_entry("location", &self.location)?;
                state.end()
            }
        }
    };
}

impl_list_serialize!(UnorderedList, "unordered", with_marker);
impl_list_serialize!(OrderedList, "ordered", with_marker);
impl_list_serialize!(CalloutList, "callout");

impl Serialize for DescriptionList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "dlist")?;
        state.serialize_entry("type", "block")?;
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        state.serialize_entry("items", &self.items)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for ListItem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "listItem")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("marker", &self.marker)?;
        if let Some(checked) = &self.checked {
            state.serialize_entry("checked", checked)?;
        }
        // The TCK doesn't contain level information for list items, so we don't serialize
        // it.
        //
        // Uncomment the line below if level information is added in the future.
        //
        // state.serialize_entry("level", &self.level)?;
        state.serialize_entry("principal", &self.principal)?;
        if !self.blocks.is_empty() {
            state.serialize_entry("blocks", &self.blocks)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for ListItemCheckedStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            ListItemCheckedStatus::Checked => serializer.serialize_bool(true),
            ListItemCheckedStatus::Unchecked => serializer.serialize_bool(false),
        }
    }
}

impl Serialize for CalloutListItem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "listItem")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("callout", &self.callout)?;
        state.serialize_entry("principal", &self.principal)?;
        if !self.blocks.is_empty() {
            state.serialize_entry("blocks", &self.blocks)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}
