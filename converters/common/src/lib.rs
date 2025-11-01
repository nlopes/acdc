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
