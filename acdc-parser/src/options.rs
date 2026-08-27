use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

pub use crate::safe_mode::SafeMode;

use crate::{AttributeValue, DocumentAttributes};

/// Whether the current explicit attributes have been classified as caller input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CallerAttributeLockState {
    /// Explicit attributes must be marked when parsing starts.
    #[default]
    Pending,
    /// Existing explicit attributes are locked; later merges remain defaults.
    Initialized,
}

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Options<'a> {
    pub safe_mode: SafeMode,
    pub timings: bool,
    pub document_attributes: DocumentAttributes<'a>,
    pub(crate) caller_attribute_lock_state: CallerAttributeLockState,
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
    ///     .build();
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

    /// Create a new `Options` with locked document attributes.
    ///
    /// Explicit attributes in the map cannot be replaced or unset by document
    /// entries.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::{Options, DocumentAttributes, AttributeValue};
    ///
    /// let mut attrs = DocumentAttributes::default();
    /// attrs.insert("toc".into(), AttributeValue::String("left".into()));
    ///
    /// let options = Options::with_attributes(attrs);
    /// ```
    #[must_use]
    pub fn with_attributes(mut document_attributes: DocumentAttributes<'a>) -> Self {
        document_attributes.mark_explicit_as_caller_locked();
        Self {
            document_attributes,
            caller_attribute_lock_state: CallerAttributeLockState::Initialized,
            ..Default::default()
        }
    }

    /// Lock attributes from directly constructed options before parsing starts.
    ///
    /// Builder-created options are already initialized. Skipping a second pass
    /// keeps attributes merged later, such as converter defaults, unlocked.
    pub(crate) fn prepare_for_parse(mut self) -> Self {
        if self.caller_attribute_lock_state == CallerAttributeLockState::Pending {
            self.document_attributes.mark_explicit_as_caller_locked();
            self.caller_attribute_lock_state = CallerAttributeLockState::Initialized;
        }
        self
    }

    /// Whether document content must ignore an attribute entry.
    ///
    /// This combines the name-based protection for read-only and API-only
    /// built-ins with locks created from caller-supplied attributes.
    pub(crate) fn is_document_attribute_locked(&self, name: &str, in_header: bool) -> bool {
        crate::constants::is_builtin_attribute_protected(name)
            || self.is_caller_attribute_locked(name, in_header)
    }

    /// Whether a caller-supplied attribute is locked against document entries.
    ///
    /// Builder attributes are marked as caller values when options are built.
    /// Directly constructed options are marked when parsing starts. Attributes
    /// merged into built options, such as converter defaults, remain
    /// document-overridable. A caller-set `sectnums` is locked in the header but
    /// becomes modifiable in the body; a caller-requested unset remains locked.
    fn is_caller_attribute_locked(&self, name: &str, in_header: bool) -> bool {
        let locked = self.document_attributes.is_caller_locked(name);

        if locked && name == "sectnums" && !in_header {
            return !self.document_attributes.is_set(name);
        }

        locked
    }

    /// Consume the options, producing an independent `'static` copy.
    #[must_use]
    pub fn into_static(self) -> Options<'static> {
        Options {
            safe_mode: self.safe_mode,
            timings: self.timings,
            document_attributes: self.document_attributes.into_static(),
            caller_attribute_lock_state: self.caller_attribute_lock_state,
            base_dir: self.base_dir,
            strict: self.strict,
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
///     .build();
/// ```
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct OptionsBuilder<'a> {
    safe_mode: SafeMode,
    timings: bool,
    document_attributes: DocumentAttributes<'a>,
    base_dir: Option<PathBuf>,
    strict: bool,
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
    ///     .build();
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
    ///     .build();
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
    ///     .build();
    /// ```
    #[must_use]
    pub fn with_strict(mut self) -> Self {
        self.strict = true;
        self
    }

    /// Add a locked document attribute.
    ///
    /// Document entries cannot replace or unset this value.
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
    ///     .build();
    /// ```
    #[must_use]
    pub fn with_attribute(
        mut self,
        name: impl Into<Cow<'a, str>>,
        value: impl Into<AttributeValue<'a>>,
    ) -> Self {
        self.document_attributes.set(name.into(), value.into());
        self
    }

    /// Set all locked document attributes at once.
    ///
    /// Explicit attributes in the map cannot be replaced or unset by document
    /// entries.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::{Options, DocumentAttributes, AttributeValue};
    ///
    /// let mut attrs = DocumentAttributes::default();
    /// attrs.insert("toc".into(), AttributeValue::String("left".into()));
    ///
    /// let options = Options::builder()
    ///     .with_attributes(attrs)
    ///     .build();
    /// ```
    #[must_use]
    pub fn with_attributes(mut self, document_attributes: DocumentAttributes<'a>) -> Self {
        self.document_attributes = document_attributes;
        self
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
    ///     .build();
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
    ///     .build();
    /// ```
    #[must_use]
    pub fn build(mut self) -> Options<'a> {
        self.document_attributes.mark_explicit_as_caller_locked();
        Options {
            safe_mode: self.safe_mode,
            timings: self.timings,
            document_attributes: self.document_attributes,
            caller_attribute_lock_state: CallerAttributeLockState::Initialized,
            base_dir: self.base_dir,
            strict: self.strict,
            #[cfg(feature = "setext")]
            setext: self.setext,
        }
    }
}
