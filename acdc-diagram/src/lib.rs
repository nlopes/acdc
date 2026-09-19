//! Diagram generation for acdc — a Rust port of asciidoctor-diagram.
//!
//! Blocks and block macros written in a plain-text diagram language are
//! rendered to images by the corresponding tool and replaced by image blocks,
//! so the rest of acdc never has to know a diagram was involved.
//!
//! ```asciidoc
//! [graphviz, ethane, svg]
//! ----
//! graph ethane {
//!   C_0 -- H_0;
//! }
//! ----
//!
//! plantuml::activity.puml[format=svg, align=center]
//! ```
//!
//! # Where it runs
//!
//! asciidoctor-diagram hooks into Asciidoctor's extension API and runs while
//! the document is parsed. acdc's parser has no extension registry, so this
//! crate runs as a pass over the finished AST, between parsing and
//! conversion:
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let parser_options = acdc_parser::Options::default();
//! let mut parsed = acdc_parser::parse_file("guide.adoc", &parser_options)?;
//!
//! let processor = acdc_diagram::Processor::new(
//!     acdc_diagram::Options::builder()
//!         .base_dir(".")
//!         .output_dir(".")
//!         .backend("html5")
//!         .build(),
//! );
//!
//! let mut warnings = Vec::new();
//! parsed.with_document_mut(|document, arena| {
//!     processor.process(document, arena, &mut warnings)
//! })?;
//! # Ok(())
//! # }
//! ```
//!
//! # Caching
//!
//! Generating a diagram means running an external program, which dominates
//! the cost of a document that contains many of them. Every generated image
//! is therefore paired with a JSON sidecar under `.asciidoctor/diagram`
//! recording the digest of the diagram code and attributes, the tool options,
//! and the measured image size. A later run that finds a matching sidecar and
//! an intact image runs nothing at all.
//!
//! The digest covers the block's attributes as well as its code, so switching
//! `layout=neato` regenerates the image; for a block macro the source file's
//! timestamp is checked too. `[graphviz%nocache]` opts a block out, and
//! `%cache-images` keeps the image itself in the cache directory and links it
//! into the output tree.
//!
//! # Differences from asciidoctor-diagram
//!
//! - Image file names use a truncated SHA-256 digest where the gem uses MD5,
//!   so the two do not share cache entries.
//! - The inline-macro form (`graphviz:chart.dot[]` inside a sentence) is not
//!   supported: by the time this pass runs the parser has already turned it
//!   into text.
//! - Rendering is always local. The gem can delegate to a remote server with
//!   `:diagram-server-url:`; acdc does not.
//! - `barcode` and `structurizr` are not ported — one is a pure-Ruby library
//!   and the other needs the rendering server bundled inside the gem.
//! - Diagram tools are located on `PATH` or through document attributes; no
//!   tool is bundled. Java-based tools (`plantuml`, `ditaa`, `umlet`,
//!   `syntrax`) prefer a native launcher and otherwise run `java -jar`
//!   against an archive named by an attribute or a `DIAGRAM_*_CLASSPATH`
//!   environment variable.

mod attrlist;
mod cache;
mod cli;
mod converters;
mod error;
mod format;
mod generate;
mod image;
mod options;
mod paths;
mod platform;
mod processor;
mod render;
mod source;
mod which;

pub use error::{Error, Result};
pub use format::Format;
pub use options::{Options, OptionsBuilder};
pub use processor::Processor;

/// Every diagram block style and block macro name this crate recognises.
///
/// Useful to callers that want to report which diagram types are available,
/// or to decide whether running the pass at all is worthwhile.
#[must_use]
pub fn diagram_names() -> &'static [&'static str] {
    converters::NAMES
}
