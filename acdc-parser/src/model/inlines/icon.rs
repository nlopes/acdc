use crate::{
    AttributeName, AttributeValue, ContentAttribute, ElementAttributes, InlineMetadata, Location,
    ReservedAttribute, Source,
    model::metadata::{InlineMetadataView, MacroAttributes},
};

/// An inline icon with independent content, identity, and macro controls.
///
/// ```compile_fail,E0616
/// use acdc_parser::{Icon, Location, Source};
/// let mut node = Icon::new(Source::Name("target"), Location::default());
/// node.alt = None;
/// ```
///
/// ```compile_fail,E0624
/// use acdc_parser::{Icon, Location, Source};
/// let node = Icon::new(Source::Name("target"), Location::default());
/// let _ = node.attributes();
/// ```
///
/// ```compile_fail,E0599
/// use acdc_parser::{Icon, Location, Source};
/// let mut node = Icon::new(Source::Name("target"), Location::default());
/// node.options_mut().push("first");
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Icon<'a> {
    /// Explicit alternative content; the target supplies the default.
    pub(crate) alt: Option<ContentAttribute<'a>>,
    /// Hover content supplied by `title=`.
    pub(crate) tooltip: Option<ContentAttribute<'a>>,
    pub(crate) target: Source<'a>,
    pub(crate) metadata: InlineMetadata<'a>,
    pub(crate) macro_attributes: MacroAttributes<'a>,
    pub(crate) location: Location,
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
    pub(crate) fn attributes(&self) -> &ElementAttributes<'a> {
        &self.macro_attributes.attributes
    }

    /// Set an attribute without a dedicated field.
    ///
    /// # Errors
    /// Rejects `alt`, `title`, `id`, `role`/`roles`, and `opts`/`options`.
    /// Use prepared content, metadata, or option edits instead.
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
    pub(crate) fn options(&self) -> &crate::TokenAssignment<'a> {
        &self.macro_attributes.options
    }

    /// Replace scalar controls after validating the complete map.
    ///
    /// # Errors
    /// Rejects dedicated content, identity, role and option names, including aliases.
    /// On error, this node stays unchanged.
    pub fn set_attributes(
        &mut self,
        attributes: ElementAttributes<'a>,
    ) -> Result<(), ReservedAttribute> {
        self.macro_attributes.replace(attributes, &["alt", "title"])
    }

    /// Copy scalar controls without copying content, identity, roles or options.
    #[must_use]
    pub fn clone_attributes(&self) -> ElementAttributes<'a> {
        self.macro_attributes.attributes.clone()
    }

    /// Transfer scalar controls, leaving none.
    #[must_use]
    pub fn take_attributes(&mut self) -> ElementAttributes<'a> {
        std::mem::take(&mut self.macro_attributes.attributes)
    }

    /// Replace prepared options, retaining presence, order and source lifetimes.
    /// An empty vector is an explicit assignment; the default value is absent.
    pub fn replace_options(
        &mut self,
        options: impl Into<crate::TokenAssignment<'a>>,
    ) -> crate::TokenAssignment<'a> {
        std::mem::replace(&mut self.macro_attributes.options, options.into())
    }

    /// Transfer the option assignment, leaving it absent.
    pub fn take_options(&mut self) -> crate::TokenAssignment<'a> {
        std::mem::take(&mut self.macro_attributes.options)
    }

    /// Append a prepared option name without interpreting or splitting it.
    pub fn push_option(&mut self, option: &'a str) {
        self.macro_attributes.options.push(option);
    }

    /// Set prepared identity and roles without registering them in a document.
    pub fn set_metadata(&mut self, metadata: InlineMetadata<'a>) {
        self.metadata = metadata;
    }

    /// Copy identity and roles, keeping scalar controls and content separate.
    #[must_use]
    pub fn clone_metadata(&self) -> InlineMetadata<'a> {
        self.metadata.clone()
    }

    /// Transfer identity and roles, leaving them absent.
    #[must_use]
    pub fn take_metadata(&mut self) -> InlineMetadata<'a> {
        std::mem::take(&mut self.metadata)
    }

    /// Set prepared alternative content; absence selects the converter's fallback.
    pub fn set_alt(&mut self, alt: Option<ContentAttribute<'a>>) {
        self.alt = alt;
    }

    /// Transfer alternative content, leaving it absent.
    #[must_use]
    pub fn take_alt(&mut self) -> Option<ContentAttribute<'a>> {
        self.alt.take()
    }

    /// Set prepared hover content, retaining absent versus explicitly empty values.
    pub fn set_tooltip(&mut self, tooltip: Option<ContentAttribute<'a>>) {
        self.tooltip = tooltip;
    }

    /// Transfer hover content, leaving it absent.
    #[must_use]
    pub fn take_tooltip(&mut self) -> Option<ContentAttribute<'a>> {
        self.tooltip.take()
    }

    /// Replace the prepared target without resolving paths, fetching or registration.
    pub fn replace_target(&mut self, source: Source<'a>) -> Source<'a> {
        std::mem::replace(&mut self.target, source)
    }

    /// Replace the node span, keeping the separate ID and content locations.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }

    pub(in crate::model) fn has_metadata(&self) -> bool {
        !self.metadata.is_empty() || !self.macro_attributes.is_empty()
    }

    pub(in crate::model) fn metadata_view(&self) -> InlineMetadataView<'_, 'a> {
        InlineMetadataView {
            attributes: &self.macro_attributes.attributes,
            roles: &self.metadata.roles,
            options: &self.macro_attributes.options,
            id: &self.metadata.id,
        }
    }
}
