//! Lists of figures, tables, and other captioned blocks — a Rust port of
//! [asciidoctor-lists].
//!
//! A `list-of::` call is replaced by cross-references to every block of the
//! named kind that carries a title or a caption:
//!
//! ```asciidoc
//! == List of figures
//! list-of::image[]
//!
//! == List of tables
//! list-of::table[hide_empty_section=true]
//! ```
//!
//! The element name is Asciidoctor's block context — `image`, `table`,
//! `listing`, `olist` — because that is what the original extension passes to
//! `find_by(context:)`. [`Processor::process`] reports a name it does not know
//! and leaves the call alone.
//!
//! [asciidoctor-lists]: https://github.com/Alwinator/asciidoctor-lists
//!
//! # Where it runs
//!
//! asciidoctor-lists is a block macro plus a treeprocessor, both driven by
//! Asciidoctor's extension API. acdc's parser has no extension registry, so
//! this crate runs as a pass over the finished AST, between parsing and
//! conversion — which means every backend renders the list without knowing it
//! was generated.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let parser_options = acdc_parser::Options::default();
//! let mut parsed = acdc_parser::parse_file("guide.adoc", &parser_options)?;
//!
//! let processor = acdc_lists::Processor::new();
//! let mut warnings = Vec::new();
//! parsed.with_document_mut(|document, arena| {
//!     processor.process(document, arena, &mut warnings);
//! });
//! # Ok(())
//! # }
//! ```
//!
//! # How an entry is rendered
//!
//! An element whose caption supplies a prefix contributes that prefix as the
//! link and its title as the text after it, so a list of figures reads
//! `Figure 1 The wonderful linux logo` with `Figure 1` linked. An element with
//! only a title puts the whole title in the link. Entries are separated by
//! hard line breaks, so one call produces one block.
//!
//! Because the prefix comes from the target's own caption, it follows
//! `figure-caption`, `table-caption` and the rest, and each backend renders it
//! the way it renders any other cross-reference.
//!
//! # Differences from asciidoctor-lists
//!
//! - An element that has no id is given one so the list can link to it. The
//!   original uses a UUID; this uses the element name and the entry's position
//!   — `image-1`, `table-2` — which is readable in a URL and stable between
//!   runs, so `:reproducible:` output stays reproducible.
//! - `enhanced_rendering` is accepted and ignored. It exists in the original
//!   to render a title's inline markup, which acdc does unconditionally: a
//!   title is inline nodes in the AST, not pre-rendered text.
//! - `hide_empty_section` removes the section holding the call, as documented.
//!   A call outside a section, or in a container that is not a section, simply
//!   disappears when its list is empty.
//! - The caption in a list entry carries no trailing period. acdc renders a
//!   caption-only cross-reference as `Figure 1`, matching `xrefstyle=short`,
//!   rather than repeating the separator that follows a block title.

mod element;
mod entry;
mod macro_call;
mod processor;
mod walk;

pub use element::Element;
pub use processor::Processor;

/// Every element name a `list-of::` call may use.
///
/// Useful to a caller that wants to report what is available, or to decide
/// whether running the pass is worthwhile at all.
#[must_use]
pub fn element_names() -> &'static [&'static str] {
    element::NAMES
}
