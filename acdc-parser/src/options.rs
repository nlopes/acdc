use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

pub use crate::safe_mode::SafeMode;

use crate::{
    AttributeValue, DocumentAttributes, Error,
    document_attribute::{InputKind, initialize_configuration, initialize_intrinsics},
    model::RawAttributes,
};

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
// Each flag turns one parse behavior on or off independently of the others,
// so they are not the states of a mode an enum could replace.
#[allow(clippy::struct_excessive_bools)]
pub struct Options<'a> {
    pub safe_mode: SafeMode,
    pub timings: bool,
    pub(crate) document_attributes: DocumentAttributes<'a>,
    /// Directory used to resolve relative includes from the entry input.
    ///
    /// String and reader input default to the current working directory. File
    /// input normally uses the entry file's parent, unless this value overrides
    /// it. In Safe and Server modes this directory is also the local-include
    /// boundary.
    pub base_dir: Option<PathBuf>,
    /// Strict mode - fail on non-conformance instead of warn-and-continue.
    ///
    /// When enabled, issues that would normally result in a warning and fallback
    /// behavior will instead cause parsing to fail. For example:
    /// - Non-conforming manpage titles (not matching `name(volume)` format)
    pub strict: bool,
    /// Resolve an inter-document cross-reference by its anchor alone.
    ///
    /// `<<other.adoc#anchor>>` then resolves as `<<anchor>>`, which is what a
    /// document assembled from includes needs: the anchor is part of this
    /// document even though the reference names the file that defines it.
    /// Custom text is untouched, so `<<other.adoc#anchor,text>>` still shows
    /// `text`. A target with no file part, or none after the `#`, is left as
    /// written.
    pub ignore_filename_in_crossrefs: bool,
    /// Enable Setext-style (underlined) header parsing.
    ///
    /// When enabled, headers can use the legacy two-line syntax:
    /// ```text
    /// Document Title
    /// ==============
    /// ```
    #[cfg(feature = "setext")]
    pub setext: bool,
}

impl<'a> Options<'a> {
    /// Create a new `OptionsBuilder` for fluent configuration.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::{Options, SafeMode};
    ///
    /// let options = Options::builder()
    ///     .with_safe_mode(SafeMode::Safe)
    ///     .with_timings()
    ///     .with_attribute("toc", "left")
    ///     .build()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    #[must_use]
    pub fn builder() -> OptionsBuilder<'a> {
        OptionsBuilder::default()
    }

    /// Create a new `Options` with default settings.
    ///
    /// Equivalent to `Options::default()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create options from application-supplied attributes.
    ///
    /// # Errors
    ///
    /// Returns a structured error for an invalid attribute value.
    pub fn with_attributes<N, V>(
        attributes: impl IntoIterator<Item = (N, V)>,
    ) -> Result<Self, Error>
    where
        N: Into<Cow<'a, str>>,
        V: Into<AttributeValue<'a>>,
    {
        Self::builder().with_attributes(attributes).build()
    }

    /// Return the validated attributes configured for parsing.
    #[must_use]
    pub const fn document_attributes(&self) -> &DocumentAttributes<'a> {
        &self.document_attributes
    }

    /// Consume the options and return their validated attribute collection.
    #[must_use]
    pub fn into_document_attributes(self) -> DocumentAttributes<'a> {
        self.document_attributes
    }

    /// Replace the attribute snapshot with one from an already parsed document.
    ///
    /// This does not reclassify or revalidate assignments. Use the builder when
    /// supplied values should become new application overrides.
    #[must_use]
    pub fn with_document_attributes(mut self, attributes: DocumentAttributes<'a>) -> Self {
        self.document_attributes = attributes;
        self
    }

    /// Reopen configuration while preserving application overrides and defaults.
    #[must_use]
    pub fn into_builder(self) -> OptionsBuilder<'a> {
        let (attributes, defaults) = self.document_attributes.into_configuration();
        OptionsBuilder {
            attributes,
            defaults,
            safe_mode: self.safe_mode,
            timings: self.timings,
            base_dir: self.base_dir,
            strict: self.strict,
            ignore_filename_in_crossrefs: Some(self.ignore_filename_in_crossrefs),
            #[cfg(feature = "setext")]
            setext: self.setext,
        }
    }

    pub(crate) fn prepare_for_parse(mut self, input_kind: InputKind<'_>) -> Self {
        initialize_intrinsics(&mut self.document_attributes, self.safe_mode, input_kind);
        self
    }

    /// Consume the options, producing an independent `'static` copy.
    #[must_use]
    pub fn into_static(self) -> Options<'static> {
        Options {
            safe_mode: self.safe_mode,
            timings: self.timings,
            document_attributes: self.document_attributes.into_static(),
            base_dir: self.base_dir,
            strict: self.strict,
            ignore_filename_in_crossrefs: self.ignore_filename_in_crossrefs,
            #[cfg(feature = "setext")]
            setext: self.setext,
        }
    }
}

/// Builder for `Options` that provides an API for configuration.
///
/// Create an `OptionsBuilder` using `Options::builder()`.
///
/// # Example
///
/// ```
/// use acdc_parser::{Options, SafeMode};
///
/// let options = Options::builder()
///     .with_safe_mode(SafeMode::Safe)
///     .with_timings()
///     .with_attribute("toc", "left")
///     .with_attribute("sectnums", true)
///     .build()?;
/// # Ok::<(), acdc_parser::Error>(())
/// ```
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct OptionsBuilder<'a> {
    safe_mode: SafeMode,
    timings: bool,
    attributes: RawAttributes<'a>,
    defaults: RawAttributes<'a>,
    base_dir: Option<PathBuf>,
    strict: bool,
    /// Left unset so a converter can supply its own default before `build`.
    ignore_filename_in_crossrefs: Option<bool>,
    #[cfg(feature = "setext")]
    setext: bool,
}

impl<'a> OptionsBuilder<'a> {
    /// Set the safe mode for parsing.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::{Options, SafeMode};
    ///
    /// let options = Options::builder()
    ///     .with_safe_mode(SafeMode::Safe)
    ///     .build()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    #[must_use]
    pub fn with_safe_mode(mut self, safe_mode: SafeMode) -> Self {
        self.safe_mode = safe_mode;
        self
    }

    /// Enable timing information during parsing.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::Options;
    ///
    /// let options = Options::builder()
    ///     .with_timings()
    ///     .build()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    #[must_use]
    pub fn with_timings(mut self) -> Self {
        self.timings = true;
        self
    }

    /// Set the directory used to resolve relative includes from the entry input.
    ///
    /// For file input this overrides the entry file's parent directory. Nested
    /// includes remain relative to the file that contains them.
    #[must_use]
    pub fn with_base_dir(mut self, base_dir: impl AsRef<Path>) -> Self {
        self.base_dir = Some(base_dir.as_ref().to_path_buf());
        self
    }

    /// Enable strict mode.
    ///
    /// When enabled, issues that would normally result in a warning and fallback
    /// behavior will instead cause parsing to fail.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::Options;
    ///
    /// let options = Options::builder()
    ///     .with_strict()
    ///     .build()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    #[must_use]
    pub fn with_strict(mut self) -> Self {
        self.strict = true;
        self
    }

    /// Resolve an inter-document cross-reference by its anchor alone, so that
    /// `<<other.adoc#anchor>>` resolves as `<<anchor>>`.
    ///
    /// See [`Options::ignore_filename_in_crossrefs`].
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::Options;
    ///
    /// let options = Options::builder()
    ///     .with_ignore_filename_in_crossrefs(true)
    ///     .build()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    #[must_use]
    pub const fn with_ignore_filename_in_crossrefs(mut self, ignore: bool) -> Self {
        self.ignore_filename_in_crossrefs = Some(ignore);
        self
    }

    /// Whether the file part of a cross-reference target has been decided yet.
    ///
    /// A converter reads this to apply its own default only when the caller
    /// expressed no preference, the way the PDF backend follows Antora and
    /// drops the file part unless asked not to.
    #[must_use]
    pub const fn ignore_filename_in_crossrefs(&self) -> Option<bool> {
        self.ignore_filename_in_crossrefs
    }

    /// Supply an application attribute.
    ///
    /// Application values take precedence over document entries, subject to the
    /// attribute's standard assignment rules.
    ///
    /// This is a convenience method that accepts various types for the value:
    /// - `&str` becomes `AttributeValue::String`
    /// - `bool` becomes `AttributeValue::Bool`
    /// - `()` becomes `AttributeValue::None`
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::Options;
    ///
    /// let options = Options::builder()
    ///     .with_attribute("toc", "left")
    ///     .with_attribute("sectnums", true)
    ///     .build()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    #[must_use]
    pub fn with_attribute(
        mut self,
        name: impl Into<Cow<'a, str>>,
        value: impl Into<AttributeValue<'a>>,
    ) -> Self {
        self.attributes.insert(name.into(), value.into());
        self
    }

    /// Replace the application-supplied attributes.
    #[must_use]
    pub fn with_attributes<N, V>(mut self, attributes: impl IntoIterator<Item = (N, V)>) -> Self
    where
        N: Into<Cow<'a, str>>,
        V: Into<AttributeValue<'a>>,
    {
        self.attributes = attributes
            .into_iter()
            .map(|(name, value)| (name.into(), value.into()))
            .collect();
        self
    }

    /// Replace defaults for attributes not supplied by the application.
    ///
    /// Document entries can replace these defaults when the attribute permits it.
    /// Intrinsic backend values retain their normal precedence.
    /// Backend, base backend, file type, and document type defaults also determine
    /// their convenience attributes when the options are built.
    #[must_use]
    pub fn with_defaults<N, V>(mut self, defaults: impl IntoIterator<Item = (N, V)>) -> Self
    where
        N: Into<Cow<'a, str>>,
        V: Into<AttributeValue<'a>>,
    {
        self.defaults = defaults
            .into_iter()
            .map(|(name, value)| (name.into(), value.into()))
            .collect();
        self
    }

    /// Supply a default without changing an application override.
    #[must_use]
    pub fn with_default_attribute(
        mut self,
        name: impl Into<Cow<'a, str>>,
        value: impl Into<AttributeValue<'a>>,
    ) -> Self {
        self.defaults.insert(name.into(), value.into());
        self
    }

    /// Inspect a supplied override or default before validation.
    #[must_use]
    pub fn attribute(&self, name: &str) -> Option<&AttributeValue<'a>> {
        self.attributes
            .get(name)
            .or_else(|| self.defaults.get(name))
    }

    /// Enable Setext-style (underlined) header parsing.
    ///
    /// When enabled, headers can use the legacy two-line syntax where
    /// the title is underlined with `=`, `-`, `~`, `^`, or `+` characters.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use acdc_parser::Options;
    ///
    /// let options = Options::builder()
    ///     .with_setext()
    ///     .build()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    #[cfg(feature = "setext")]
    #[must_use]
    pub fn with_setext(mut self) -> Self {
        self.setext = true;
        self
    }

    /// Build the `Options` from this builder.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::{Options, SafeMode};
    ///
    /// let options = Options::builder()
    ///     .with_safe_mode(SafeMode::Safe)
    ///     .build()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a structured error for an invalid attribute value.
    pub fn build(self) -> Result<Options<'a>, Error> {
        let document_attributes = initialize_configuration(self.attributes, self.defaults)?;
        Ok(Options {
            safe_mode: self.safe_mode,
            timings: self.timings,
            document_attributes,
            base_dir: self.base_dir,
            strict: self.strict,
            ignore_filename_in_crossrefs: self.ignore_filename_in_crossrefs.unwrap_or(false),
            #[cfg(feature = "setext")]
            setext: self.setext,
        })
    }
}
