//! Admonition types for `AsciiDoc` documents.

use std::fmt::Display;
use std::str::FromStr;

use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

use crate::{Error, Positioning, SourceLocation};

use super::Block;
use super::location::{Location, Position};
use super::metadata::BlockMetadata;
use super::title::Title;

/// An `Admonition` represents an admonition in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Admonition {
    pub metadata: BlockMetadata,
    pub variant: AdmonitionVariant,
    pub blocks: Vec<Block>,
    pub title: Title,
    pub location: Location,
}

/// The variant/type of an admonition block.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AdmonitionVariant {
    Note,
    Tip,
    Important,
    Caution,
    Warning,
}

impl Display for AdmonitionVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdmonitionVariant::Note => write!(f, "note"),
            AdmonitionVariant::Tip => write!(f, "tip"),
            AdmonitionVariant::Important => write!(f, "important"),
            AdmonitionVariant::Caution => write!(f, "caution"),
            AdmonitionVariant::Warning => write!(f, "warning"),
        }
    }
}

impl FromStr for AdmonitionVariant {
    type Err = Error;

    fn from_str(variant: &str) -> Result<Self, Self::Err> {
        match variant.to_lowercase().as_str() {
            "note" => Ok(AdmonitionVariant::Note),
            "tip" => Ok(AdmonitionVariant::Tip),
            "important" => Ok(AdmonitionVariant::Important),
            "caution" => Ok(AdmonitionVariant::Caution),
            "warning" => Ok(AdmonitionVariant::Warning),
            _ => Err(Error::Parse(
                Box::new(SourceLocation {
                    file: None,
                    positioning: Positioning::Position(Position::default()),
                }),
                format!("unknown admonition variant: {variant}"),
            )),
        }
    }
}

impl Admonition {
    /// Create a new admonition with the given variant, blocks, and location.
    #[must_use]
    pub fn new(variant: AdmonitionVariant, blocks: Vec<Block>, location: Location) -> Self {
        Self {
            metadata: BlockMetadata::default(),
            variant,
            blocks,
            title: Title::default(),
            location,
        }
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Title) -> Self {
        self.title = title;
        self
    }
}

impl Serialize for Admonition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "admonition")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("variant", &self.variant)?;
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }

        if !self.blocks.is_empty() {
            state.serialize_entry("blocks", &self.blocks)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}
