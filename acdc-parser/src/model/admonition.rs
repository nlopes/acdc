//! Admonition types for `AsciiDoc` documents.

use std::{fmt::Display, mem, str::FromStr};

use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

use crate::{Error, SourceLocation};

use super::{
    Block, Paragraph,
    location::{Location, Position},
    metadata::BlockMetadata,
    title::Title,
};

/// An `Admonition` represents an admonition in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Admonition<'a> {
    pub(crate) metadata: BlockMetadata<'a>,
    pub(crate) variant: AdmonitionVariant,
    pub(crate) blocks: Vec<Block<'a>>,
    pub(crate) title: Title<'a>,
    pub(crate) location: Location,
    pub(crate) form: AdmonitionForm<'a>,
}

/// The authored form of an admonition, independent of its semantic category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AdmonitionForm<'a> {
    /// A library-built admonition without source-form information.
    Unspecified,
    /// An ordinary or styled paragraph.
    Paragraph,
    /// A styled indented paragraph.
    Indented,
    /// A delimited block with its actual opening token.
    Delimited(&'a str),
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
                    location: crate::Location::point(Position::default()),
                }),
                format!("unknown admonition variant: {variant}"),
            )),
        }
    }
}

impl<'a> Admonition<'a> {
    /// Create a compound admonition without source-form information.
    /// Use [`Self::with_form`] when its source form is known.
    #[must_use]
    pub fn new(variant: AdmonitionVariant, blocks: Vec<Block<'a>>, location: Location) -> Self {
        Self {
            metadata: BlockMetadata::default(),
            variant,
            blocks,
            title: Title::default(),
            location,
            form: AdmonitionForm::Unspecified,
        }
    }

    /// Create a paragraph-form admonition with the paragraph's title and attributes.
    /// Body substitution settings remain on the paragraph as well.
    #[must_use]
    pub fn from_paragraph(
        variant: AdmonitionVariant,
        mut paragraph: Paragraph<'a>,
        location: Location,
    ) -> Self {
        let form = if paragraph
            .source_text()
            .is_some_and(|source| source.starts_with([' ', '\t']))
        {
            AdmonitionForm::Indented
        } else {
            AdmonitionForm::Paragraph
        };
        let metadata = mem::take(&mut paragraph.metadata);
        let title = mem::take(&mut paragraph.title);
        paragraph
            .metadata
            .substitutions
            .clone_from(&metadata.substitutions);
        Self {
            metadata,
            title,
            form,
            ..Self::new(variant, vec![Block::Paragraph(paragraph)], location)
        }
    }

    /// Set source form without rewriting children or locations.
    #[must_use]
    pub fn with_form(mut self, form: AdmonitionForm<'a>) -> Self {
        self.set_form(form);
        self
    }

    /// Replace source-form information without rewriting children or locations.
    pub fn set_form(&mut self, form: AdmonitionForm<'a>) {
        self.form = form;
    }

    /// Replace metadata without shadowing dedicated fields.
    ///
    /// # Errors
    /// Rejects scalar attributes for dedicated metadata or title fields.
    pub fn with_metadata(
        mut self,
        metadata: BlockMetadata<'a>,
    ) -> Result<Self, crate::ReservedAttribute> {
        self.set_metadata(metadata)?;
        Ok(self)
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Title<'a>) -> Self {
        self.set_title(title);
        self
    }

    /// Replace metadata without shadowing dedicated fields.
    ///
    /// # Errors
    /// Rejects conflicting scalar attributes without changing the admonition.
    pub fn set_metadata(
        &mut self,
        metadata: BlockMetadata<'a>,
    ) -> Result<(), crate::ReservedAttribute> {
        metadata.validate_attributes(&["title"])?;
        self.metadata = metadata;
        Ok(())
    }

    /// Copy metadata to prepare a checked replacement without copying children.
    #[must_use]
    pub fn clone_metadata(&self) -> BlockMetadata<'a> {
        self.metadata.clone()
    }

    /// Remove metadata without changing children or their substitution settings.
    #[must_use]
    pub fn take_metadata(&mut self) -> BlockMetadata<'a> {
        mem::take(&mut self.metadata)
    }

    /// Replace the title, including an explicitly empty value.
    pub fn set_title(&mut self, title: Title<'a>) {
        self.title = title;
    }

    /// Remove the title, leaving it absent.
    #[must_use]
    pub fn take_title(&mut self) -> Title<'a> {
        mem::take(&mut self.title)
    }

    /// Replace prepared children without changing source form or registrations.
    pub fn set_blocks(&mut self, blocks: impl IntoIterator<Item = Block<'a>>) {
        self.blocks = blocks.into_iter().collect();
    }

    /// Append a prepared child without changing source form or registrations.
    pub fn push_block(&mut self, block: Block<'a>) {
        self.blocks.push(block);
    }

    /// Remove children, retaining source form and each child's source lifetime.
    pub fn take_blocks(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = Block<'a>> + ExactSizeIterator + use<'a> {
        mem::take(&mut self.blocks).into_iter()
    }

    /// Replace the admonition category without rewriting child metadata.
    pub fn set_variant(&mut self, variant: AdmonitionVariant) {
        self.variant = variant;
    }

    /// Replace the block span without relocating children.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
}

impl Serialize for Admonition<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "admonition")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("variant", &self.variant)?;
        match self.form {
            AdmonitionForm::Unspecified => {}
            AdmonitionForm::Paragraph => state.serialize_entry("form", "paragraph")?,
            AdmonitionForm::Indented => state.serialize_entry("form", "indented")?,
            AdmonitionForm::Delimited(delimiter) => {
                state.serialize_entry("form", "delimited")?;
                state.serialize_entry("delimiter", delimiter)?;
            }
        }
        if !self.metadata.is_empty_for_serialization() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        if self.title.is_present() {
            state.serialize_entry("title", &self.title)?;
        }

        if !self.blocks.is_empty() {
            state.serialize_entry("blocks", &self.blocks)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}
