//! List types for `AsciiDoc` documents.

use serde::{
    Deserialize, Serialize,
    de::{self, Deserializer, MapAccess, Visitor},
    ser::{SerializeMap, Serializer},
};

use super::Block;
use super::anchor::Anchor;
use super::inlines::InlineNode;
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    pub items: Vec<ListItem>,
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

// =============================================================================
// Deserialization
// =============================================================================

impl<'de> Deserialize<'de> for ListItem {
    fn deserialize<D>(deserializer: D) -> Result<ListItem, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ListItemVisitor;

        impl<'de> Visitor<'de> for ListItemVisitor {
            type Value = ListItem;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing ListItem")
            }

            fn visit_map<V>(self, mut map: V) -> Result<ListItem, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut my_principal = None;
                let mut my_blocks = None;
                let mut my_checked = None;
                let mut my_location = None;
                let mut my_marker = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "principal" => {
                            if my_principal.is_some() {
                                return Err(de::Error::duplicate_field("principal"));
                            }
                            my_principal = Some(map.next_value()?);
                        }
                        "blocks" => {
                            if my_blocks.is_some() {
                                return Err(de::Error::duplicate_field("blocks"));
                            }
                            my_blocks = Some(map.next_value()?);
                        }
                        "marker" => {
                            if my_marker.is_some() {
                                return Err(de::Error::duplicate_field("marker"));
                            }
                            my_marker = Some(map.next_value::<String>()?);
                        }
                        "location" => {
                            if my_location.is_some() {
                                return Err(de::Error::duplicate_field("location"));
                            }
                            my_location = Some(map.next_value()?);
                        }
                        "checked" => {
                            if my_checked.is_some() {
                                return Err(de::Error::duplicate_field("checked"));
                            }
                            my_checked = Some(map.next_value::<bool>()?);
                        }
                        _ => {
                            tracing::debug!(?key, "ignoring unexpected field in ListItem");
                            // Ignore any other fields
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                let marker = my_marker.ok_or_else(|| de::Error::missing_field("marker"))?;
                let principal =
                    my_principal.ok_or_else(|| de::Error::missing_field("principal"))?;
                let blocks = my_blocks.unwrap_or_default();
                let level =
                    ListLevel::try_from(ListItem::parse_depth_from_marker(&marker).unwrap_or(1))
                        .map_err(|e| {
                            de::Error::custom(format!("invalid list item level from marker: {e}",))
                        })?;
                let checked = my_checked.map(|c| {
                    if c {
                        ListItemCheckedStatus::Checked
                    } else {
                        ListItemCheckedStatus::Unchecked
                    }
                });
                Ok(ListItem {
                    level,
                    marker,
                    location: my_location.ok_or_else(|| de::Error::missing_field("location"))?,
                    principal,
                    blocks,
                    checked,
                })
            }
        }
        deserializer.deserialize_map(ListItemVisitor)
    }
}

impl<'de> Deserialize<'de> for ListItemCheckedStatus {
    fn deserialize<D>(deserializer: D) -> Result<ListItemCheckedStatus, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ListItemCheckedStatusVisitor;

        impl Visitor<'_> for ListItemCheckedStatusVisitor {
            type Value = ListItemCheckedStatus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a boolean representing checked status")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v {
                    Ok(ListItemCheckedStatus::Checked)
                } else {
                    Ok(ListItemCheckedStatus::Unchecked)
                }
            }
        }

        deserializer.deserialize_bool(ListItemCheckedStatusVisitor)
    }
}
