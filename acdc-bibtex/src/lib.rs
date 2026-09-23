//! BibTeX citations and bibliographies — a Rust port of [asciidoctor-bibtex].
//!
//! Three macros are resolved against a `.bib` database:
//!
//! ```asciidoc
//! Author-year references: cite:[Lane12] or citenp:[Lane12].
//! With a locator: cite:[Lane12(59)].
//! A rendered entry, inline: bibitem:[Lane12].
//!
//! bibliography::[]
//! ```
//!
//! `cite:` reads as a parenthetical — `(Lane 2012)` — and `citenp:` as part of
//! the sentence — `Lane (2012)`. Each citation links to its entry in the
//! reference list that replaces `bibliography::[]`.
//!
//! [asciidoctor-bibtex]: https://github.com/asciidoctor/asciidoctor-bibtex
//!
//! # Where it runs
//!
//! asciidoctor-bibtex is a block macro plus a treeprocessor. acdc's parser has
//! no extension registry, so this runs as a pass over the finished AST,
//! between parsing and conversion — which means every backend renders the
//! citations without knowing they were generated.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let parser_options = acdc_parser::Options::default();
//! let mut parsed = acdc_parser::parse_file("paper.adoc", &parser_options)?;
//!
//! let processor = acdc_bibtex::Processor::new(
//!     acdc_bibtex::Options::builder().base_dir(".").build(),
//! );
//! let mut warnings = Vec::new();
//! parsed.with_document_mut(|document, arena| {
//!     processor.process(document, arena, &mut warnings)
//! })?;
//! # Ok(())
//! # }
//! ```
//!
//! # Configuration
//!
//! Every setting is a document attribute, with the same name and default as
//! the original extension:
//!
//! | Attribute | Values | Default |
//! |---|---|---|
//! | `bibtex-file` | a path to a `.bib` file | the one beside the document |
//! | `bibtex-style` | `ieee`, `apa`, `chicago-author-date` | `ieee` |
//! | `bibtex-order` | `appearance`, `alphabetical` | `appearance` |
//! | `bibtex-format` | `asciidoc`, `bibtex`, `biblatex` | `asciidoc` |
//! | `bibtex-throw` | `true`, `false` | `false` |
//! | `bibtex-citation-template` | any text containing `$id` | `[$id]` |
//!
//! A relative `bibtex-file` is read the way the original extension reads it:
//! as written, from the directory the command was run in. A path that is not
//! there is then looked for beside the document, so the same document also
//! builds from elsewhere.
//!
//! `bibliography::refs.bib[apa]` names the database and the style too, for a
//! document that sets neither attribute; an attribute wins where both are
//! given.
//!
//! Appearance order applies to a numeric style; an author-date bibliography is
//! sorted by author and year whatever the attribute says, as it is in the
//! original.
//!
//! # Differences from asciidoctor-bibtex
//!
//! - **Styles.** The original renders through citeproc and the CSL style
//!   files, which cover thousands of styles. acdc implements `ieee`, `apa` and
//!   `chicago-author-date` — the three the original's own documentation and
//!   tests are written against — directly. Another style name is reported and
//!   falls back to `ieee`.
//! - `bibtex-locale` is accepted and ignored: the built-in styles render in
//!   English.
//! - A directory holding several `.bib` files is left for the document to
//!   resolve with `bibtex-file`, rather than one being picked arbitrarily.
//! - Numeric citations are not merged into ranges. The original merges them
//!   only when it is not linking entries, which is not how it runs as an
//!   extension, so the behaviour was never reachable.
//! - citeproc drops the `booktitle` from an `@conference` entry, and from an
//!   `@inproceedings` entry that also carries a publisher. acdc keeps it.

mod database;
mod error;
mod latex;
mod macros;
mod names;
mod processor;
mod settings;
mod style;
mod walk;

pub use error::Error;
pub use processor::{Options, OptionsBuilder, Processor};

/// Every `bibtex-style` value this crate renders.
#[must_use]
pub fn style_names() -> &'static [&'static str] {
    style::NAMES
}
