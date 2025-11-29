use acdc_core::{Doctype, SafeMode};
use acdc_parser::{AttributeValue, DocumentAttributes};

pub mod code;
pub mod substitutions;
pub mod table;
pub mod toc;
pub mod video;

// Visitor pattern infrastructure
pub mod visitor;

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

#[derive(Debug, Default, Clone)]
pub struct Options {
    pub generator_metadata: GeneratorMetadata,
    pub doctype: Doctype,
    pub safe_mode: SafeMode,
    pub timings: bool,
}

pub trait PrettyDuration {
    /// Returns a human-readable string representation of the duration.
    ///
    /// - Automatically selects appropriate unit (ns, µs, ms, s)
    /// - Rounds to 2 decimal places
    /// - Strips trailing zeros
    fn pretty_print(&self) -> String;

    /// Returns a detailed timing string with specified precision
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

#[derive(Debug, Default, Clone)]
pub struct GeneratorMetadata {
    pub name: String,
    pub version: String,
}

impl GeneratorMetadata {
    #[must_use]
    pub fn new<S: AsRef<str>>(name: S, version: S) -> Self {
        Self {
            name: name.as_ref().to_string(),
            version: version.as_ref().to_string(),
        }
    }
}

impl std::fmt::Display for GeneratorMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} v{}", self.name, self.version)
    }
}

pub trait Processable {
    type Options;
    type Error;

    fn new(options: Self::Options, document_attributes: DocumentAttributes) -> Self;

    /// Convert a pre-parsed document
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
}

/// Walk the error source chain to find a parser error
///
/// This utility function searches through the error chain looking for
/// an `acdc_parser::Error` instance, which allows the CLI to provide
/// rich error displays with source code context.
///
/// # How it works
///
/// Uses the standard `Error::source()` chain walking pattern to traverse
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
