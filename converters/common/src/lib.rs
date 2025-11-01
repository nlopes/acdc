use acdc_core::{Doctype, SafeMode, Source};
use acdc_parser::DocumentAttributes;

pub mod toc;
pub mod video;

// Visitor pattern infrastructure
pub mod visitor;

#[derive(Debug, Default, Clone)]
pub struct Options {
    pub generator_metadata: GeneratorMetadata,
    pub doctype: Doctype,
    pub safe_mode: SafeMode,
    pub source: Source,
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

    /// Run the processor
    ///
    /// # Errors
    ///
    /// Will typically return parsing or rendering errors. Implementations are free to
    /// return any error type they wish though.
    fn run(&self) -> Result<(), Self::Error>;
}

/// Trait for converting `AsciiDoc` documents to different output formats.
///
/// Converters handle the transformation from parsed AST to output format
/// (HTML, terminal, etc.) using the visitor pattern internally.
pub trait Converter {
    /// Error type for conversion operations
    type Error;

    /// Format-specific options (e.g., `RenderOptions` for HTML)
    type Options: Clone + Default;

    /// Convert a document to the target format, writing to the provided writer.
    ///
    /// # Errors
    ///
    /// Returns an error if document conversion or writing fails.
    fn convert<W: std::io::Write>(
        &self,
        doc: &acdc_parser::Document,
        writer: W,
        options: &Self::Options,
    ) -> Result<(), Self::Error>;
}
