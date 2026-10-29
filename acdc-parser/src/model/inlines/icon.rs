use crate::{
    AttributeName, AttributeValue, ContentAttribute, ElementAttributes, InlineMetadata, Location,
    ReservedAttribute, Source,
    model::metadata::{InlineMetadataView, MacroAttributes},
};

/// An inline icon with independent content, identity, and macro controls.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Icon<'a> {
    /// Explicit alternative content; the target supplies the default.
    pub alt: Option<ContentAttribute<'a>>,
    /// Hover content supplied by `title=`.
    pub tooltip: Option<ContentAttribute<'a>>,
    pub target: Source<'a>,
    pub metadata: InlineMetadata<'a>,
    pub(crate) macro_attributes: MacroAttributes<'a>,
    pub location: Location,
}

impl<'a> Icon<'a> {
    /// Construct an icon with no explicit alternative or hover text.
    #[must_use]
    pub fn new(target: Source<'a>, location: Location) -> Self {
        Self {
            alt: None,
            tooltip: None,
            target,
            metadata: InlineMetadata::default(),
            macro_attributes: MacroAttributes::default(),
            location,
        }
    }

    /// Read scalar macro controls, such as size and link.
    ///
    /// This map excludes alt, tooltip, identity, roles, and options. Read those
    /// values through their dedicated fields or methods.
    #[must_use]
    pub fn attributes(&self) -> &ElementAttributes<'a> {
        &self.macro_attributes.attributes
    }

    /// Set an attribute without a dedicated field.
    ///
    /// # Errors
    /// Rejects `alt`, `title`, `id`, `role`/`roles`, and `opts`/`options`.
    /// Update the content, metadata, or options directly instead.
    pub fn set_attribute(
        &mut self,
        name: AttributeName<'a>,
        value: AttributeValue<'a>,
    ) -> Result<(), ReservedAttribute> {
        self.macro_attributes.set(name, value, &["alt", "title"])
    }

    /// Remove an attribute without a dedicated field and return its previous value.
    ///
    /// # Errors
    /// Rejects the dedicated names listed by [`Self::set_attribute`].
    pub fn remove_attribute(
        &mut self,
        name: &str,
    ) -> Result<Option<AttributeValue<'a>>, ReservedAttribute> {
        self.macro_attributes.remove(name, &["alt", "title"])
    }

    /// Read the macro's option names in their stored order.
    #[must_use]
    pub fn options(&self) -> &[&'a str] {
        &self.macro_attributes.options
    }

    /// Edit the macro's option names without creating attribute-map aliases.
    pub fn options_mut(&mut self) -> &mut Vec<&'a str> {
        &mut self.macro_attributes.options
    }

    pub(crate) fn has_metadata(&self) -> bool {
        !self.metadata.is_empty() || !self.macro_attributes.is_empty()
    }

    pub(crate) fn metadata_view(&self) -> InlineMetadataView<'_, 'a> {
        InlineMetadataView {
            attributes: &self.macro_attributes.attributes,
            roles: &self.metadata.roles,
            options: &self.macro_attributes.options,
            id: &self.metadata.id,
        }
    }
}
