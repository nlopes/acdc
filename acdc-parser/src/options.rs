use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

pub use crate::safe_mode::SafeMode;

use crate::{
    AttributeValue, DocumentAttributes, Error, IncludeLoader,
    document_attribute::{InputKind, initialize_configuration, initialize_intrinsics},
    model::RawAttributes,
};

const DEFAULT_MAX_TOTAL_INCLUDE_BYTES: usize = 64 * 1024 * 1024;

/// Selects the supported Asciidoctor or draft language rules for one parse.
///
/// Both modes use the same AST. Currently they differ in named block-title
/// quoting and repeated block roles and options. Neither mode promises complete
/// conformance, and the choice does not change safe mode or output format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompatibilityMode {
    /// Aim for maximum compatibility with Asciidoctor, with documented exceptions
    /// for bug fixes and deliberate improvements to the user experience.
    #[default]
    Asciidoctor,
    /// Apply supported draft rules while retaining shared acdc behavior.
    Draft,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum FileXrefMode {
    /// Use normal reference rules, including fully included source files.
    #[default]
    SourceAware,
    ForceLocal,
}

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Options<'a> {
    /// Defaults to [`CompatibilityMode::Asciidoctor`]. Independent of ASG export.
    pub compatibility_mode: CompatibilityMode,
    /// Defaults to [`SafeMode::Secure`], which prevents include reads.
    pub safe_mode: SafeMode,
    pub timings: bool,
    pub(crate) document_attributes: DocumentAttributes<'a>,
    /// Selects how the parser loads include sources.
    ///
    /// Defaults to [`IncludeLoader::System`], subject to `safe_mode`. Set
    /// [`IncludeLoader::Disabled`] to preserve directives in modes below Secure.
    pub include_loader: IncludeLoader,
    pub(crate) max_total_include_bytes: Option<usize>,
    /// Directory used to resolve relative includes from the entry input.
    ///
    /// Only consulted when include loading is enabled. String and reader input
    /// default to the current working directory. File input normally uses the
    /// entry file's parent, unless this value overrides it. In Safe and Server
    /// modes this directory is also the local-include boundary.
    pub base_dir: Option<PathBuf>,
    /// Reject a manpage title that does not use the `name(volume)` form.
    ///
    /// This does not promote all warnings to errors. Use
    /// [`crate::ParseResult::source_recovery`] to detect recovered content.
    pub strict: bool,
    pub(crate) file_xref_mode: FileXrefMode,
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

    /// Maximum selected include text per parse, in bytes. Defaults to 64 MiB.
    ///
    /// See [`OptionsBuilder::with_max_total_include_bytes`] for what counts.
    #[must_use]
    pub fn max_total_include_bytes(&self) -> usize {
        self.max_total_include_bytes
            .unwrap_or(DEFAULT_MAX_TOTAL_INCLUDE_BYTES)
    }

    pub(crate) fn force_local_file_xrefs(&self) -> bool {
        self.file_xref_mode == FileXrefMode::ForceLocal
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
            compatibility_mode: self.compatibility_mode,
            attributes,
            defaults,
            safe_mode: self.safe_mode,
            timings: self.timings,
            include_loader: self.include_loader,
            max_total_include_bytes: self.max_total_include_bytes,
            base_dir: self.base_dir,
            strict: self.strict,
            file_xref_mode: self.file_xref_mode,
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
            compatibility_mode: self.compatibility_mode,
            safe_mode: self.safe_mode,
            timings: self.timings,
            document_attributes: self.document_attributes.into_static(),
            include_loader: self.include_loader,
            max_total_include_bytes: self.max_total_include_bytes,
            base_dir: self.base_dir,
            strict: self.strict,
            file_xref_mode: self.file_xref_mode,
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
    compatibility_mode: CompatibilityMode,
    safe_mode: SafeMode,
    timings: bool,
    attributes: RawAttributes<'a>,
    defaults: RawAttributes<'a>,
    include_loader: IncludeLoader,
    max_total_include_bytes: Option<usize>,
    base_dir: Option<PathBuf>,
    strict: bool,
    file_xref_mode: FileXrefMode,
    #[cfg(feature = "setext")]
    setext: bool,
}

impl<'a> OptionsBuilder<'a> {
    /// Select the language rules for this parse. Defaults to Asciidoctor.
    #[must_use]
    pub fn with_compatibility_mode(mut self, mode: CompatibilityMode) -> Self {
        self.compatibility_mode = mode;
        self
    }

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

    /// Select how the parser loads include sources.
    ///
    /// Defaults to [`IncludeLoader::System`]. The default Secure mode prevents
    /// include reads even when a provider is supplied. Use [`IncludeLoader::Disabled`]
    /// to disable loading in lower modes, or [`IncludeLoader::custom`] to supply
    /// sources from an editor overlay, virtual filesystem, or custom transport.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::{IncludeLoader, Options, SafeMode};
    ///
    /// let options = Options::builder()
    ///     .with_safe_mode(SafeMode::Safe)
    ///     .with_include_loader(IncludeLoader::System)
    ///     .build()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    #[must_use]
    pub fn with_include_loader(mut self, include_loader: IncludeLoader) -> Self {
        self.include_loader = include_loader;
        self
    }

    /// Set the total selected include text allowed per parse, in bytes.
    ///
    /// Defaults to 64 MiB. Each selection counts after `indent=`, plus one byte
    /// for a separating newline if the text is not empty. This count precedes
    /// nested processing and removal of comments and inactive conditionals.
    /// Nested and repeated includes share the limit. The entry text does not count.
    ///
    /// Zero permits only empty selections. Document attributes cannot change this
    /// limit. The 10 MiB per-include limit and safe-mode permissions still apply.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::{Options, SafeMode};
    ///
    /// let options = Options::builder()
    ///     .with_safe_mode(SafeMode::Safe)
    ///     .with_max_total_include_bytes(256 * 1024 * 1024)
    ///     .build()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    #[must_use]
    pub fn with_max_total_include_bytes(mut self, bytes: usize) -> Self {
        self.max_total_include_bytes = Some(bytes);
        self
    }

    /// Set the directory used to resolve relative includes from the entry input.
    ///
    /// Only takes effect when include loading is enabled.
    /// For file input this overrides the entry file's parent directory.
    /// For string or reader input it overrides the current working directory.
    /// Nested includes remain relative to the file that contains them.
    #[must_use]
    pub fn with_base_dir(mut self, base_dir: impl AsRef<Path>) -> Self {
        self.base_dir = Some(base_dir.as_ref().to_path_buf());
        self
    }

    /// Reject manpage titles that do not use the `name(volume)` form.
    ///
    /// Other recoverable problems still produce warnings.
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

    /// Force file-qualified cross-references to resolve within the parsed document.
    ///
    /// Use for combined or partially included sources whose references still
    /// name the original files. `<<file.adoc#anchor>>` becomes `<<anchor>>`;
    /// `xref:file.adoc#anchor[text]` keeps its text. A nonempty fragment is required.
    /// Multi-letter URI schemes and `//` prefixes retain normal resolution.
    /// A single letter before `:` is treated as a Windows drive.
    ///
    /// The fragment must identify the intended local anchor. This option also
    /// redirects genuine external file references and does not scope duplicate
    /// IDs. Missing anchors produce unresolved-reference warnings; no target
    /// files are loaded. Antora resource IDs and page-only references are not
    /// resolved by this option. Disabled by default.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::Options;
    ///
    /// let options = Options::builder()
    ///     .with_force_local_file_xrefs()
    ///     .build()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    #[must_use]
    pub fn with_force_local_file_xrefs(mut self) -> Self {
        self.file_xref_mode = FileXrefMode::ForceLocal;
        self
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
    /// ```
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
            compatibility_mode: self.compatibility_mode,
            safe_mode: self.safe_mode,
            timings: self.timings,
            document_attributes,
            include_loader: self.include_loader,
            max_total_include_bytes: self.max_total_include_bytes,
            base_dir: self.base_dir,
            strict: self.strict,
            file_xref_mode: self.file_xref_mode,
            #[cfg(feature = "setext")]
            setext: self.setext,
        })
    }
}
