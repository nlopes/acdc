use crate::{
    AttributeName, AttributeValue, ContentAttribute, ElementAttributes, InlineMetadata, InlineNode,
    Location, ReservedAttribute, Source,
    model::metadata::{InlineMetadataView, MacroAttributes},
};

/// An explicit link with optional label content.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Link<'a> {
    /// Hover content supplied by `title=`.
    pub tooltip: Option<ContentAttribute<'a>>,
    pub text: Vec<InlineNode<'a>>,
    pub target: Source<'a>,
    pub metadata: InlineMetadata<'a>,
    pub(crate) macro_attributes: MacroAttributes<'a>,
    pub location: Location,
    pub(crate) hide_uri_scheme: bool,
}

impl<'a> Link<'a> {
    /// Construct an explicit link with automatic display text.
    #[must_use]
    pub fn new(target: Source<'a>, location: Location) -> Self {
        Self {
            text: Vec::new(),
            tooltip: None,
            target,
            metadata: InlineMetadata::default(),
            macro_attributes: MacroAttributes::default(),
            location,
            hide_uri_scheme: false,
        }
    }

    /// Sets the link text as inline nodes.
    #[must_use]
    pub fn with_text(mut self, text: Vec<InlineNode<'a>>) -> Self {
        self.text = text;
        self
    }

    /// Set scalar attributes without shadowing dedicated metadata.
    ///
    /// # Errors
    /// Rejects the dedicated names listed by [`Self::set_attribute`].
    pub fn with_attributes(
        mut self,
        attributes: ElementAttributes<'a>,
    ) -> Result<Self, ReservedAttribute> {
        self.macro_attributes.replace(attributes, &["title"])?;
        Ok(self)
    }

    /// Whether fallback display text omits the target's URI scheme.
    #[must_use]
    pub fn hides_uri_scheme(&self) -> bool {
        self.hide_uri_scheme
    }
}

/// A URL macro with optional label content.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Url<'a> {
    /// Hover content supplied by `title=`.
    pub tooltip: Option<ContentAttribute<'a>>,
    pub text: Vec<InlineNode<'a>>,
    pub target: Source<'a>,
    pub metadata: InlineMetadata<'a>,
    pub(crate) macro_attributes: MacroAttributes<'a>,
    pub location: Location,
    pub(crate) hide_uri_scheme: bool,
}

impl Url<'_> {
    /// Whether fallback display text omits the target's URI scheme.
    #[must_use]
    pub fn hides_uri_scheme(&self) -> bool {
        self.hide_uri_scheme
    }
}

/// An email link with optional positional query values.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Mailto<'a> {
    /// Hover content supplied by `title=`.
    pub tooltip: Option<ContentAttribute<'a>>,
    pub text: Vec<InlineNode<'a>>,
    pub target: Source<'a>,
    /// The second positional argument, before URI encoding.
    pub subject: Option<&'a str>,
    /// The third positional argument, before URI encoding.
    pub body: Option<&'a str>,
    pub metadata: InlineMetadata<'a>,
    pub(crate) macro_attributes: MacroAttributes<'a>,
    pub location: Location,
}

impl<'a> Url<'a> {
    /// Construct a URL macro with automatic display text.
    #[must_use]
    pub fn new(target: Source<'a>, location: Location) -> Self {
        Self {
            text: Vec::new(),
            tooltip: None,
            target,
            location,
            metadata: InlineMetadata::default(),
            macro_attributes: MacroAttributes::default(),
            hide_uri_scheme: false,
        }
    }
}

impl<'a> Mailto<'a> {
    /// Construct an email link without a subject or body.
    #[must_use]
    pub fn new(target: Source<'a>, location: Location) -> Self {
        Self {
            text: Vec::new(),
            tooltip: None,
            target,
            location,
            subject: None,
            body: None,
            metadata: InlineMetadata::default(),
            macro_attributes: MacroAttributes::default(),
        }
    }
}

macro_rules! hyperlink_controls {
    ($($owner:ident),+ $(,)?) => { $(
        impl<'a> $owner<'a> {
            /// Scalar controls and accepted extensions, excluding identity, roles, options, and tooltip.
            #[must_use]
            pub fn attributes(&self) -> &ElementAttributes<'a> {
                &self.macro_attributes.attributes
            }

            /// Set a scalar control or extension attribute.
            ///
            /// # Errors
            /// Rejects `title`, `id`, `role`/`roles`, and `opts`/`options`.
            /// Update the tooltip, metadata, or options directly instead.
            pub fn set_attribute(&mut self, name: AttributeName<'a>, value: AttributeValue<'a>) -> Result<(), ReservedAttribute> {
                self.macro_attributes.set(name, value, &["title"])
            }

            /// Remove a scalar control or extension attribute.
            ///
            /// # Errors
            /// Rejects the dedicated names listed by [`Self::set_attribute`].
            pub fn remove_attribute(&mut self, name: &str) -> Result<Option<AttributeValue<'a>>, ReservedAttribute> {
                self.macro_attributes.remove(name, &["title"])
            }

            /// Option tokens in their retained order.
            #[must_use]
            pub fn options(&self) -> &[&'a str] { &self.macro_attributes.options }

            /// Edit option tokens without creating attribute-map aliases.
            pub fn options_mut(&mut self) -> &mut Vec<&'a str> { &mut self.macro_attributes.options }

            pub(crate) fn metadata_view(&self) -> InlineMetadataView<'_, 'a> {
                InlineMetadataView {
                    attributes: &self.macro_attributes.attributes,
                    roles: &self.metadata.roles, options: &self.macro_attributes.options, id: &self.metadata.id,
                }
            }
        }
    )+ };
}

hyperlink_controls!(Link, Url, Mailto);
