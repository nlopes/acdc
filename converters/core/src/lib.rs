//! Core traits and utilities for acdc document converters.
//!
//! This crate provides the shared infrastructure used by all acdc converters
//! (HTML, terminal, manpage, etc.):
//!
//! - [`Converter`] - trait that all converters implement
//! - [`Visitor`](visitor::Visitor) - visitor pattern for AST traversal
//! - [`Options`] - configuration for conversion
//! - [`default_rendering_attributes`] - default document attributes for rendering
//!
//! # Example
//!
//! ```ignore
//! use acdc_converters_core::{Options, Converter, Doctype};
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

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use acdc_parser::{AttributeValue, DocumentAttributes, SafeMode};

/// Source code syntax highlighting and callouts support.
pub mod code;
mod doctype;
pub mod icon;
pub mod section;
pub mod substitutions;
pub mod table;
pub mod toc;
pub mod video;
pub mod visitor;

pub use doctype::Doctype;

/// Decode HTML numeric character references (`&#NNN;` and `&#xHH;`) to Unicode characters.
///
/// Non-HTML converters (terminal, manpage) should call this on `RawText` content
/// so that character replacement attributes like `{apos}` (`&#39;`) render as
/// literal characters instead of HTML entities.
///
/// # Examples
///
/// ```
/// use acdc_converters_core::decode_numeric_char_refs;
///
/// assert_eq!(&*decode_numeric_char_refs("&#39;"), "\'");
/// assert_eq!(&*decode_numeric_char_refs("&#34;"), "\"");
/// assert_eq!(&*decode_numeric_char_refs("&#169;"), "\u{00A9}");
/// assert_eq!(&*decode_numeric_char_refs("&#x27;"), "\'");
/// assert_eq!(&*decode_numeric_char_refs("no refs"), "no refs");
/// ```
#[must_use]
pub fn decode_numeric_char_refs(text: &str) -> Cow<'_, str> {
    if !text.contains("&#") {
        return Cow::Borrowed(text);
    }

    let mut result = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find("&#") {
        result.push_str(&rest[..start]);
        let after_amp = &rest[start + 2..];

        if let Some(end) = after_amp.find(';') {
            let ref_body = &after_amp[..end];
            let decoded = if let Some(hex) = ref_body
                .strip_prefix('x')
                .or_else(|| ref_body.strip_prefix('X'))
            {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else {
                ref_body.parse::<u32>().ok().and_then(char::from_u32)
            };

            if let Some(ch) = decoded {
                result.push(ch);
                rest = &after_amp[end + 1..];
            } else {
                result.push_str("&#");
                rest = after_amp;
            }
        } else {
            result.push_str("&#");
            rest = after_amp;
        }
    }

    result.push_str(rest);
    Cow::Owned(result)
}

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
pub fn default_rendering_attributes() -> DocumentAttributes<'static> {
    let mut attrs = DocumentAttributes::default();

    // HTML lang attribute (default: "en")
    attrs.set(
        std::borrow::Cow::Borrowed("lang"),
        AttributeValue::String(std::borrow::Cow::Borrowed("en")),
    );

    // Admonition captions (capitalized to match asciidoctor)
    attrs.set(
        std::borrow::Cow::Borrowed("note-caption"),
        AttributeValue::String(std::borrow::Cow::Borrowed("Note")),
    );
    attrs.set(
        std::borrow::Cow::Borrowed("tip-caption"),
        AttributeValue::String(std::borrow::Cow::Borrowed("Tip")),
    );
    attrs.set(
        std::borrow::Cow::Borrowed("important-caption"),
        AttributeValue::String(std::borrow::Cow::Borrowed("Important")),
    );
    attrs.set(
        std::borrow::Cow::Borrowed("warning-caption"),
        AttributeValue::String(std::borrow::Cow::Borrowed("Warning")),
    );
    attrs.set(
        std::borrow::Cow::Borrowed("caution-caption"),
        AttributeValue::String(std::borrow::Cow::Borrowed("Caution")),
    );

    // TOC levels (only used when :toc: is set)
    attrs.set(
        std::borrow::Cow::Borrowed("toclevels"),
        AttributeValue::String(std::borrow::Cow::Borrowed("2")),
    );

    // Section numbering levels (for future section numbering feature)
    attrs.set(
        std::borrow::Cow::Borrowed("sectnumlevels"),
        AttributeValue::String(std::borrow::Cow::Borrowed("3")),
    );

    // NOTE: :toc: is intentionally NOT set - TOC should only appear when explicitly requested
    // NOTE: :sectids: is enabled by default in the parser itself, no attribute needed

    attrs
}

/// Output destination for conversion.
///
/// This enum explicitly represents the three possible output modes:
/// - `Derived`: No explicit output specified, derive from input filename
/// - `Stdout`: Explicitly output to stdout (via `-o -`)
/// - `File`: Output to a specific file (via `-o path`)
#[derive(Debug, Clone, Default)]
pub enum OutputDestination {
    /// Derive output path from input file (default behavior).
    /// For HTML: `input.adoc` → `input.html`
    /// For manpage: `cmd.adoc` → `cmd.1`
    #[default]
    Derived,
    /// Write to stdout (equivalent to `-o -`).
    Stdout,
    /// Write to a specific file.
    File(PathBuf),
}

/// Result metadata for a completed conversion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConversionResult {
    output_path: Option<PathBuf>,
}

impl ConversionResult {
    /// Conversion wrote to stdout or another non-file destination.
    #[must_use]
    pub fn stdout() -> Self {
        Self { output_path: None }
    }

    /// Conversion wrote to a file at `path`.
    #[must_use]
    pub fn file(path: PathBuf) -> Self {
        Self {
            output_path: Some(path),
        }
    }

    /// Get the file path written by this conversion, if any.
    #[must_use]
    pub fn output_path(&self) -> Option<&Path> {
        self.output_path.as_deref()
    }

    /// Consume the result and return the file path written by this conversion, if any.
    #[must_use]
    pub fn into_output_path(self) -> Option<PathBuf> {
        self.output_path
    }
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
    /// Output destination for conversion.
    output_destination: OutputDestination,
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

    /// Get the output destination.
    ///
    /// See [`OutputDestination`] for the possible values.
    #[must_use]
    pub fn output_destination(&self) -> &OutputDestination {
        &self.output_destination
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
    output_destination: OutputDestination,
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

    /// Set the output destination.
    ///
    /// See [`OutputDestination`] for the possible values.
    /// If this is not called, converters will derive output path from input file.
    #[must_use]
    pub fn output_destination(mut self, destination: OutputDestination) -> Self {
        self.output_destination = destination;
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
            output_destination: self.output_destination,
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
/// 2. **Converter-specific defaults** - from [`Converter::document_attributes_defaults()`] (e.g., `man-linkstyle` for manpage)
/// 3. **CLI attributes** - user-provided via `-a name=value`
/// 4. **Document attributes** - `:name: value` in document header
///
/// ## Implementation
///
/// Converters must implement these required methods:
/// - [`write_to`](Converter::write_to) - Core conversion logic
/// - [`derive_output_path`](Converter::derive_output_path) - Output path derivation
/// - [`name`](Converter::name) - Converter name for logging/messages
/// - [`options`](Converter::options) - Access to converter options
/// - [`document_attributes`](Converter::document_attributes) - Access to document attributes
///
/// Converters get these methods for free:
/// - [`convert`](Converter::convert) - Main entry point with routing
/// - [`convert_to_stdout`](Converter::convert_to_stdout) - Output to stdout
/// - [`convert_to_file`](Converter::convert_to_file) - Output to file with timing
pub trait Converter<'a>: Sized {
    /// The error type for this converter.
    ///
    /// Must implement `From<std::io::Error>` for the provided methods to work.
    type Error: std::error::Error + From<std::io::Error>;

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
    fn document_attributes_defaults() -> DocumentAttributes<'static> {
        DocumentAttributes::default()
    }

    /// Create a new converter instance.
    fn new(options: Options, document_attributes: DocumentAttributes<'a>) -> Self;

    /// Get a reference to the converter options.
    fn options(&self) -> &Options;

    /// Get a reference to the document attributes.
    #[must_use]
    fn document_attributes(&self) -> &DocumentAttributes<'a>;

    /// Derive output path from input path (e.g., "doc.adoc" → "doc.html").
    ///
    /// Returns `Ok(None)` if this converter doesn't support derived output paths
    /// (e.g., terminal converter always outputs to stdout by default).
    ///
    /// Returns `Err` if derivation fails (e.g., output path would overwrite input).
    ///
    /// # Arguments
    ///
    /// * `input` - The input file path
    /// * `doc` - The parsed document (for attributes like `manvolnum`)
    ///
    /// # Errors
    ///
    /// Returns an error if the derived output path is invalid (e.g., same as input).
    fn derive_output_path(
        &self,
        input: &std::path::Path,
        doc: &acdc_parser::Document<'a>,
    ) -> Result<Option<std::path::PathBuf>, Self::Error>;

    /// Core conversion: write the document to any writer.
    ///
    /// This is the only method converters MUST implement with real conversion logic.
    /// All other output methods delegate to this.
    ///
    /// # Arguments
    ///
    /// * `doc` - The parsed document
    /// * `writer` - Any type implementing `Write`
    /// * `source_file` - Optional source file path (for metadata, last modified, etc.)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion or writing fails.
    fn write_to<W: std::io::Write>(
        &self,
        doc: &acdc_parser::Document<'a>,
        writer: W,
        source_file: Option<&std::path::Path>,
    ) -> Result<(), Self::Error>;

    /// Post-processing after successful file write.
    ///
    /// Override for converter-specific cleanup (e.g., CSS copying for HTML).
    /// Default implementation does nothing.
    fn after_write(&self, _doc: &acdc_parser::Document<'a>, _output_path: &std::path::Path) {}

    /// Returns the converter's display name (e.g. `"html"`, `"html5s"`,
    /// `"manpage"`, `"markdown"`, `"terminal"`).
    ///
    /// Used by the provided methods for logging and the "Generated X file"
    /// message. Implementations may return different names for different
    /// internal states (e.g. the html converter returns `"html5s"` when in
    /// semantic mode).
    #[must_use]
    fn name(&self) -> &'static str;

    /// Convert to stdout.
    ///
    /// # Errors
    ///
    /// Returns an error if conversion or writing fails.
    fn convert_to_stdout(
        &self,
        doc: &acdc_parser::Document<'a>,
        source_file: Option<&std::path::Path>,
    ) -> Result<ConversionResult, Self::Error> {
        let stdout = std::io::stdout();
        self.write_to(doc, std::io::BufWriter::new(stdout.lock()), source_file)?;
        Ok(ConversionResult::stdout())
    }

    /// Convert to a specific file path.
    ///
    /// Handles timing output and success messages automatically.
    ///
    /// # Errors
    ///
    /// Returns an error if file creation, conversion, or writing fails.
    fn convert_to_file(
        &self,
        doc: &acdc_parser::Document<'a>,
        source_file: Option<&std::path::Path>,
        output_path: &std::path::Path,
    ) -> Result<ConversionResult, Self::Error> {
        let start = self.options().timings().then(std::time::Instant::now);

        if let Some(f) = source_file.filter(|_| self.options().timings()) {
            println!("Input file: {}", f.display());
        }

        tracing::debug!(
            source = ?source_file,
            destination = ?output_path,
            "converting document to {}",
            self.name()
        );

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(output_path)?;
        self.write_to(doc, std::io::BufWriter::new(file), source_file)?;

        if let Some(start) = start {
            let elapsed = start.elapsed();
            tracing::debug!(
                time = elapsed.pretty_print_precise(3),
                source = ?source_file,
                destination = ?output_path,
                "time to convert document"
            );
            println!("  Time to convert document: {}", elapsed.pretty_print());
        }

        println!("Generated {} file: {}", self.name(), output_path.display());

        self.after_write(doc, output_path);
        Ok(ConversionResult::file(output_path.to_path_buf()))
    }

    /// Main entry point: route based on [`OutputDestination`].
    ///
    /// This method handles all output routing logic:
    /// - `Stdout`: Write to stdout
    /// - `File(path)`: Write to specific file
    /// - `Derived`: Derive path from input or fall back to stdout
    ///
    /// # Errors
    ///
    /// Returns an error if conversion or writing fails.
    fn convert(
        &self,
        doc: &acdc_parser::Document<'a>,
        source_file: Option<&std::path::Path>,
    ) -> Result<ConversionResult, Self::Error> {
        match self.options().output_destination() {
            OutputDestination::Stdout => self.convert_to_stdout(doc, source_file),
            OutputDestination::File(path) => self.convert_to_file(doc, source_file, path),
            OutputDestination::Derived => {
                if let Some(input) = source_file
                    && let Some(output) = self.derive_output_path(input, doc)?
                {
                    return self.convert_to_file(doc, source_file, &output);
                }
                self.convert_to_stdout(doc, source_file)
            }
        }
    }
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
