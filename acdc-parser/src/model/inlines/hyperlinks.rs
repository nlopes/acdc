use crate::{
    AttributeName, AttributeValue, ContentAttribute, ElementAttributes, InlineMetadata, InlineNode,
    Location, ReservedAttribute, Source,
    model::metadata::{InlineMetadataView, MacroAttributes},
};

/// An explicit link with optional label content.
///
/// ```compile_fail,E0616
/// use acdc_parser::{Link, Location, Source};
/// let mut link = Link::new(Source::Name("target"), Location::default());
/// link.text = Vec::new();
/// ```
///
/// ```compile_fail,E0624
/// use acdc_parser::{Link, Location, Source};
/// let link = Link::new(Source::Name("target"), Location::default());
/// let _ = link.attributes();
/// ```
///
/// ```compile_fail,E0599
/// use acdc_parser::{Link, Location, Source};
/// let mut link = Link::new(Source::Name("target"), Location::default());
/// link.options_mut().push("first");
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Link<'a> {
    /// Hover content supplied by `title=`.
    pub(crate) tooltip: Option<ContentAttribute<'a>>,
    pub(crate) text: Vec<InlineNode<'a>>,
    pub(crate) target: Source<'a>,
    pub(crate) metadata: InlineMetadata<'a>,
    pub(crate) macro_attributes: MacroAttributes<'a>,
    pub(crate) location: Location,
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
        self.replace_text(text);
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
        self.set_attributes(attributes)?;
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
pub struct UrlMacro<'a> {
    /// Hover content supplied by `title=`.
    pub(crate) tooltip: Option<ContentAttribute<'a>>,
    pub(crate) text: Vec<InlineNode<'a>>,
    pub(crate) target: Source<'a>,
    pub(crate) metadata: InlineMetadata<'a>,
    pub(crate) macro_attributes: MacroAttributes<'a>,
    pub(crate) location: Location,
    pub(crate) hide_uri_scheme: bool,
}

impl UrlMacro<'_> {
    /// Whether fallback display text omits the target's URI scheme.
    #[must_use]
    pub fn hides_uri_scheme(&self) -> bool {
        self.hide_uri_scheme
    }
}

/// An email link with optional positional query values.
///
/// ```compile_fail,E0616
/// use acdc_parser::{Mailto, Location, Source};
/// let mut mail = Mailto::new(Source::Name("reader@example.org"), Location::default());
/// mail.subject = Some("Subject");
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Mailto<'a> {
    /// Hover content supplied by `title=`.
    pub(crate) tooltip: Option<ContentAttribute<'a>>,
    pub(crate) text: Vec<InlineNode<'a>>,
    pub(crate) target: Source<'a>,
    /// The second positional argument, before URI encoding.
    pub(crate) subject: Option<&'a str>,
    /// The third positional argument, before URI encoding.
    pub(crate) body: Option<&'a str>,
    pub(crate) metadata: InlineMetadata<'a>,
    pub(crate) macro_attributes: MacroAttributes<'a>,
    pub(crate) location: Location,
}

impl<'a> UrlMacro<'a> {
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
            pub(crate) fn attributes(&self) -> &ElementAttributes<'a> {
                &self.macro_attributes.attributes
            }

            /// Set a scalar control or extension attribute.
            ///
            /// # Errors
            /// Rejects `title`, `id`, `role`/`roles`, and `opts`/`options`.
            /// Use prepared tooltip, metadata, or option edits instead.
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
            pub(crate) fn options(&self) -> &crate::TokenAssignment<'a> {
                &self.macro_attributes.options
            }

            /// Replace prepared label content without parsing or updating registrations.
            /// An empty label selects automatic display text.
            pub fn replace_text(&mut self, text: Vec<InlineNode<'a>>) -> Vec<InlineNode<'a>> {
                std::mem::replace(&mut self.text, text)
            }

            /// Transfer the label, leaving automatic display text selected.
            pub fn take_text(&mut self) -> Vec<InlineNode<'a>> {
                std::mem::take(&mut self.text)
            }

            /// Append a prepared label inline without parsing or registration.
            pub fn push_inline(&mut self, inline: InlineNode<'a>) {
                self.text.push(inline);
            }

            /// Replace scalar controls after validating the complete map.
            ///
            /// # Errors
            /// Rejects dedicated content, identity, role and option names, including aliases.
            /// On error, this node stays unchanged.
            pub fn set_attributes(&mut self, attributes: ElementAttributes<'a>) -> Result<(), ReservedAttribute> {
                self.macro_attributes.replace(attributes, &["title"])
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
            pub fn replace_options(&mut self, options: impl Into<crate::TokenAssignment<'a>>) -> crate::TokenAssignment<'a> {
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

            pub(in crate::model) fn metadata_view(&self) -> InlineMetadataView<'_, 'a> {
                InlineMetadataView {
                    attributes: &self.macro_attributes.attributes,
                    roles: &self.metadata.roles, options: &self.macro_attributes.options, id: &self.metadata.id,
                }
            }
        }
    )+ };
}

hyperlink_controls!(Link, UrlMacro, Mailto);

impl Link<'_> {
    /// Set whether automatic display text omits the URI scheme.
    pub fn set_hide_uri_scheme(&mut self, hide: bool) {
        self.hide_uri_scheme = hide;
    }
}

impl UrlMacro<'_> {
    /// Set whether automatic display text omits the URI scheme.
    pub fn set_hide_uri_scheme(&mut self, hide: bool) {
        self.hide_uri_scheme = hide;
    }
}

impl<'a> Mailto<'a> {
    /// Replace the optional subject without URI encoding or changing named controls.
    pub fn replace_subject(&mut self, subject: Option<&'a str>) -> Option<&'a str> {
        std::mem::replace(&mut self.subject, subject)
    }

    /// Replace the optional body without URI encoding or changing named controls.
    pub fn replace_body(&mut self, body: Option<&'a str>) -> Option<&'a str> {
        std::mem::replace(&mut self.body, body)
    }
}
