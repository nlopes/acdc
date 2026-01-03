//! Core traits and utilities for acdc document converters.
//!
//! This crate provides the shared infrastructure used by all acdc converters
//! (HTML, terminal, manpage, etc.):
//!
//! - [`Processable`] - trait that all converters implement
//! - [`Visitor`](visitor::Visitor) - visitor pattern for AST traversal
//! - [`Options`] - configuration for conversion
//! - [`default_rendering_attributes`] - default document attributes for rendering
//!
//! # Example
//!
//! ```ignore
//! use acdc_converters_core::{Options, Processable, Doctype};
//!
//! let options = Options::builder()
//!     .doctype(Doctype::Article)
//!     .embedded(true)
//!     .build();
//! ```
//!
//! # Modules
//!
//! - [`code`] - Programming language detection for syntax highlighting
//! - [`icon`] - Icon rendering mode configuration
//! - [`substitutions`] - Text substitution utilities for escape handling
//! - [`table`] - Table column width calculations
//! - [`toc`] - Table of contents configuration
//! - [`video`] - Video URL generation for `YouTube`, `Vimeo`, etc.
//! - [`visitor`] - Visitor pattern infrastructure for AST traversal

use acdc_parser::{AttributeValue, DocumentAttributes, SafeMode};

/// Source code syntax highlighting and callouts support.
pub mod code;
mod doctype;
pub mod icon;
pub mod substitutions;
pub mod table;
pub mod toc;
pub mod video;
pub mod visitor;

pub use doctype::Doctype;

/// Create default document attributes for rendering.
///
/// These defaults match asciidoctor's rendering behavior and are used by converters
/// (HTML, terminal, etc.) to provide consistent output. Document-level attributes
/// from the source always take precedence over these defaults.
///
/// # Default Attributes
///
/// - `lang`: "en" - HTML lang attribute for accessibility
/// - `note-caption`: "Note" - Capitalized admonition label
/// - `tip-caption`: "Tip" - Capitalized admonition label
/// - `important-caption`: "Important" - Capitalized admonition label
/// - `warning-caption`: "Warning" - Capitalized admonition label
/// - `caution-caption`: "Caution" - Capitalized admonition label
/// - `toclevels`: "2" - Table of contents depth (only used when `:toc:` is set)
/// - `sectnumlevels`: "3" - Section numbering depth (when section numbering enabled)
///
/// # Usage
///
/// Converters should merge these defaults with document attributes:
///
/// ```ignore
/// let mut attrs = default_rendering_attributes();
/// attrs.merge(document.attributes.clone()); // Document attributes override defaults
/// ```
///
/// # Note
///
/// The `:toc:` attribute is intentionally NOT set by default - TOC generation
/// must be explicitly requested in the document.
#[must_use]
pub fn default_rendering_attributes() -> DocumentAttributes {
    let mut attrs = DocumentAttributes::default();

    // HTML lang attribute (default: "en")
    attrs.set("lang".to_string(), AttributeValue::String("en".to_string()));

    // Admonition captions (capitalized to match asciidoctor)
    attrs.set(
        "note-caption".to_string(),
        AttributeValue::String("Note".to_string()),
    );
    attrs.set(
        "tip-caption".to_string(),
        AttributeValue::String("Tip".to_string()),
    );
    attrs.set(
        "important-caption".to_string(),
        AttributeValue::String("Important".to_string()),
    );
    attrs.set(
        "warning-caption".to_string(),
        AttributeValue::String("Warning".to_string()),
    );
    attrs.set(
        "caution-caption".to_string(),
        AttributeValue::String("Caution".to_string()),
    );

    // TOC levels (only used when :toc: is set)
    attrs.set(
        "toclevels".to_string(),
        AttributeValue::String("2".to_string()),
    );

    // Section numbering levels (for future section numbering feature)
    attrs.set(
        "sectnumlevels".to_string(),
        AttributeValue::String("3".to_string()),
    );

    // NOTE: :toc: is intentionally NOT set - TOC should only appear when explicitly requested
    // NOTE: :sectids: is enabled by default in the parser itself, no attribute needed

    attrs
}

/// Converter options.
///
/// Use [`Options::builder()`] to construct an instance. This struct is marked
/// `#[non_exhaustive]` to allow adding new fields in future minor versions.
///
/// # Example
///
/// ```
/// use acdc_converters_core::{Options, Doctype, GeneratorMetadata};
///
/// let options = Options::builder()
///     .doctype(Doctype::Article)
///     .embedded(true)
///     .generator_metadata(GeneratorMetadata::new("my-converter", "1.0.0"))
///     .build();
/// ```
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Options {
    generator_metadata: GeneratorMetadata,
    doctype: Doctype,
    safe_mode: SafeMode,
    timings: bool,
    embedded: bool,
}

impl Options {
    /// Create a new builder with default values.
    #[must_use]
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder::default()
    }

    /// Get the generator metadata.
    #[must_use]
    pub fn generator_metadata(&self) -> &GeneratorMetadata {
        &self.generator_metadata
    }

    /// Get the document type.
    #[must_use]
    pub fn doctype(&self) -> Doctype {
        self.doctype
    }

    /// Get the safe mode.
    #[must_use]
    pub fn safe_mode(&self) -> SafeMode {
        self.safe_mode
    }

    /// Get whether timing information should be output.
    #[must_use]
    pub fn timings(&self) -> bool {
        self.timings
    }

    /// Get whether to output an embeddable document.
    ///
    /// When true, converters should output content without document wrappers
    /// (e.g., no DOCTYPE, html, head, body tags for HTML).
    #[must_use]
    pub fn embedded(&self) -> bool {
        self.embedded
    }
}

/// Builder for [`Options`].
///
/// Use [`Options::builder()`] to create a new builder.
#[derive(Debug, Default, Clone)]
pub struct OptionsBuilder {
    generator_metadata: GeneratorMetadata,
    doctype: Doctype,
    safe_mode: SafeMode,
    timings: bool,
    embedded: bool,
}

impl OptionsBuilder {
    /// Set the generator metadata (name and version).
    #[must_use]
    pub fn generator_metadata(mut self, meta: GeneratorMetadata) -> Self {
        self.generator_metadata = meta;
        self
    }

    /// Set the document type (Article, Book, Manpage, Inline).
    #[must_use]
    pub fn doctype(mut self, doctype: Doctype) -> Self {
        self.doctype = doctype;
        self
    }

    /// Set the safe mode for processing.
    #[must_use]
    pub fn safe_mode(mut self, mode: SafeMode) -> Self {
        self.safe_mode = mode;
        self
    }

    /// Enable or disable timing output.
    #[must_use]
    pub fn timings(mut self, timings: bool) -> Self {
        self.timings = timings;
        self
    }

    /// Enable or disable embedded output mode.
    ///
    /// When true, converters should output content without document wrappers.
    #[must_use]
    pub fn embedded(mut self, embedded: bool) -> Self {
        self.embedded = embedded;
        self
    }

    /// Build the [`Options`] instance.
    #[must_use]
    pub fn build(self) -> Options {
        Options {
            generator_metadata: self.generator_metadata,
            doctype: self.doctype,
            safe_mode: self.safe_mode,
            timings: self.timings,
            embedded: self.embedded,
        }
    }
}

/// Extension trait for formatting [`Duration`](std::time::Duration) in human-readable form.
pub trait PrettyDuration {
    /// Returns a human-readable string representation of the duration.
    ///
    /// - Automatically selects appropriate unit (ns, µs, ms, s)
    /// - Rounds to 2 decimal places
    /// - Strips trailing zeros
    fn pretty_print(&self) -> String;

    /// Returns a detailed timing string with specified precision.
    ///
    /// # Arguments
    /// * `precision` - Number of decimal places (0-9)
    fn pretty_print_precise(&self, precision: u8) -> String;
}

impl PrettyDuration for std::time::Duration {
    fn pretty_print(&self) -> String {
        let nanos = self.as_nanos();

        // This is actually fine. f64 can represent all integers up to u128::MAX: 2^128-1
        // (roughly 3.8x10^38).
        #[allow(clippy::cast_precision_loss)]
        let f_nanos = nanos as f64;
        match nanos {
            0..=999 => format!("{nanos}ns"),
            1_000..=999_999 => format!("{:.2}µs", f_nanos / 1_000.0),
            1_000_000..=999_999_999 => format!("{:.2}ms", f_nanos / 1_000_000.0),
            _ => format!("{:.2}s", f_nanos / 1_000_000_000.0),
        }
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
    }

    fn pretty_print_precise(&self, precision: u8) -> String {
        let precision = precision.min(9);
        let nanos = self.as_nanos();
        // This is actually fine. f64 can represent all integers up to u128::MAX: 2^128-1
        // (roughly 3.8x10^38).
        #[allow(clippy::cast_precision_loss)]
        let f_nanos = nanos as f64;
        match nanos {
            0..=999 => format!("{nanos}ns"),
            1_000..=999_999 => format!("{:.1$}µs", nanos / 1_000, precision as usize),
            1_000_000..=999_999_999 => {
                format!("{:.1$}ms", nanos / 1_000_000, precision as usize)
            }
            _ => format!("{:.1$}s", f_nanos / 1_000_000_000.0, precision as usize),
        }
    }
}

/// Generator metadata for tracking which tool produced the output.
///
/// This is embedded in generated output (e.g., HTML meta tags, man page comments)
/// for debugging and identification purposes.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct GeneratorMetadata {
    name: String,
    version: String,
}

impl GeneratorMetadata {
    /// Create new generator metadata.
    #[must_use]
    pub fn new<S: AsRef<str>>(name: S, version: S) -> Self {
        Self {
            name: name.as_ref().to_string(),
            version: version.as_ref().to_string(),
        }
    }

    /// Get the generator name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the generator version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

impl std::fmt::Display for GeneratorMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} v{}", self.name, self.version)
    }
}

/// Trait for document converters (HTML, Terminal, Manpage, etc.)
///
/// ## Attribute layering
///
/// Document attributes follow a layered precedence system (lowest to highest priority):
///
/// 1. **Base rendering defaults** - from [`default_rendering_attributes()`] (admonition captions, toclevels, etc.)
/// 2. **Converter-specific defaults** - from [`Processable::document_attributes_defaults()`] (e.g., `man-linkstyle` for manpage)
/// 3. **CLI attributes** - user-provided via `-a name=value`
/// 4. **Document attributes** - `:name: value` in document header
///
/// Converters should use `document_attributes_defaults()` to provide backend-specific attribute defaults.
pub trait Processable {
    /// The options type for this converter.
    type Options;
    /// The error type for this converter.
    type Error;

    /// Returns converter-specific default attributes.
    ///
    /// Override this in converters that need backend-specific defaults.
    /// These defaults are merged into the attribute map in `new()`, but won't
    /// overwrite user-provided values (CLI or document attributes).
    ///
    /// # Examples
    ///
    /// - HTML: `stylesdir`, `toc-class`, `webfonts`
    /// - Manpage: `man-linkstyle`, `manname-title`
    /// - Terminal: (none - uses environment detection)
    #[must_use]
    fn document_attributes_defaults() -> DocumentAttributes {
        DocumentAttributes::default()
    }

    /// Create a new converter instance.
    fn new(options: Self::Options, document_attributes: DocumentAttributes) -> Self;

    /// Convert a pre-parsed document.
    ///
    /// The CLI handles all parsing (stdin or files), and converters just focus on conversion.
    ///
    /// # Arguments
    ///
    /// * `doc` - The pre-parsed document
    /// * `file` - Optional source file path (used for output path, metadata, etc.)
    ///   - `Some(path)` for file-based conversion
    ///   - `None` for stdin-based conversion
    ///
    /// # Errors
    ///
    /// Returns an error if conversion or writing fails.
    fn convert(
        &self,
        doc: &acdc_parser::Document,
        file: Option<&std::path::Path>,
    ) -> Result<(), Self::Error>;

    /// Get a reference to the document attributes.
    #[must_use]
    fn document_attributes(&self) -> DocumentAttributes;
}

/// Walk the error source chain to find a parser error.
///
/// This utility function searches through the error chain looking for
/// an [`acdc_parser::Error`] instance, which allows the CLI to provide
/// rich error displays with source code context.
///
/// # How it works
///
/// Uses the standard [`Error::source()`](std::error::Error::source) chain walking pattern to traverse
/// the error hierarchy. At each level, attempts to downcast to
/// `acdc_parser::Error`. Returns the first match found, or None if no
/// parser error exists in the chain.
///
/// This approach leverages Rust's built-in error handling mechanisms and
/// works automatically with any error type that uses `#[error(transparent)]`
/// or implements `source()` correctly.
///
/// # Example
///
/// ```ignore
/// if let Some(parser_error) = find_parser_error(&converter_error) {
///     // Display rich error with source location
/// }
/// ```
pub fn find_parser_error<'e>(
    e: &'e (dyn std::error::Error + 'static),
) -> Option<&'e acdc_parser::Error> {
    // Try to downcast the error directly first
    if let Some(parser_error) = e.downcast_ref::<acdc_parser::Error>() {
        return Some(parser_error);
    }

    // Walk the source chain
    let mut current = e.source();
    while let Some(err) = current {
        if let Some(parser_error) = err.downcast_ref::<acdc_parser::Error>() {
            return Some(parser_error);
        }
        current = err.source();
    }
    None
}
