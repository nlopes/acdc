//! Manpage converter for `AsciiDoc` documents.
//!
//! This converter outputs native roff/troff format suitable for the `man` command.
//! It targets modern GNU groff and produces semantically equivalent output to
//! Asciidoctor's manpage backend.
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//! use acdc_converters_manpage::Processor;
//! use acdc_converters_core::{Converter, Options};
//!
//! let options = Options::default();
//! let processor = Processor::new(options, acdc_parser::Options::builder())?;
//! let parsed = acdc_parser::parse(
//!     "= cmd(1)\n\n== Name\n\ncmd - run a command\n",
//!     processor.parser_options(),
//! )?;
//! processor.convert(parsed.document(), Some(Path::new("cmd.adoc")))?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Output Format
//!
//! The converter generates roff output with:
//! - `.TH` header with program name, volume, date, source, and manual
//! - `.SH` and `.SS` macros for section headings
//! - `.PP`, `.IP`, `.TP` for paragraphs and list items
//! - `.EX`/`.EE` for code examples
//! - `.TS`/`.TE` for tables (tbl preprocessor format)
//! - `\fB`, `\fI`, `\fP` for inline formatting

use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::SubsFlags;
use acdc_converters_core::{
    BackendProfile, Converter, Diagnostics, Doctype, Options, TraversalContext,
    section::has_index_section, visitor::Visitor, xref::XrefGuard,
};

use acdc_parser::{
    AttributeValue, Document, DocumentAttributes, Options as ParserOptions, Reference,
};

mod admonition;
mod delimited;
mod document;
mod error;
mod escape;
mod index;
mod inlines;
mod list;
mod manpage_visitor;
mod media;
mod paragraph;
mod section;
mod table;

pub use error::Error;
pub use escape::{EscapeMode, manify};
pub use manpage_visitor::ManpageVisitor;

const MANPAGE_BACKEND: BackendProfile = BackendProfile::new("manpage", "manpage", "man", ".man");

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct IndexTermLabel {
    pub(crate) plain: String,
    pub(crate) rendered: String,
}

#[derive(Clone, Debug)]
pub(crate) struct IndexTermEntry {
    pub(crate) primary: IndexTermLabel,
    pub(crate) secondary: Option<IndexTermLabel>,
    pub(crate) tertiary: Option<IndexTermLabel>,
    pub(crate) relationship: IndexCatalogRelationship,
}

#[derive(Clone, Debug)]
pub(crate) enum IndexCatalogRelationship {
    None,
    See(IndexTermLabel),
    SeeAlso(Vec<IndexTermLabel>),
}

/// Manpage converter processor.
#[derive(Clone, Debug)]
pub struct Processor<'a> {
    options: Options,
    parser_options: ParserOptions<'a>,
    pub(crate) references: Rc<HashMap<&'a str, Reference<'a>>>,
    /// Keeps a cross-reference inside a resolved target's text from recursing.
    pub(crate) xref_guard: XrefGuard,
    pub(crate) top_level_section_ids: Rc<HashSet<&'a str>>,
    pub(crate) static_media_warning: Rc<Cell<bool>>,
    pub(crate) inline_role_warning: Rc<Cell<bool>>,
    pub(crate) index_entries: Rc<RefCell<Vec<IndexTermEntry>>>,
    pub(crate) has_valid_index_section: bool,
    /// Substitutions active for the block currently being rendered, resolved
    /// from `[subs="…"]` (or the block-kind baseline when absent). Shared
    /// across clones so sub-visitors inherit the outer block's effective
    /// subs. `Cell<SubsFlags>` is a single-byte load/store with no borrow
    /// tracking.
    ///
    /// Only present when the `pre-spec-subs` feature is enabled; otherwise
    /// the converter applies typography unconditionally (asciidoctor default).
    #[cfg(feature = "pre-spec-subs")]
    pub(crate) current_subs: Rc<Cell<SubsFlags>>,
}

impl Processor<'_> {
    /// Convert a document to manpage output, writing to the provided writer.
    ///
    /// # Errors
    ///
    /// Returns an error if conversion or writing fails.
    pub fn write_document<'doc, W: Write>(
        &self,
        doc: &Document<'doc>,
        writer: W,
        source_file: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Error> {
        let mut attrs: DocumentAttributes<'doc> = doc.attributes.clone();

        if attrs.get("revdate").is_none()
            && let Some(date_str) = source_file.and_then(file_modified_date)
        {
            attrs = ParserOptions::builder()
                .with_attributes(attrs.into_inputs())
                .with_attribute("revdate", date_str)
                .build()?
                .into_document_attributes();
        }

        // Per-conversion processor borrows from `doc`; lifetime independent of `self`.
        let processor: Processor<'doc> = Processor {
            options: self.options.clone(),
            parser_options: ParserOptions::default().with_document_attributes(attrs),
            references: Rc::new(doc.references.clone()),
            xref_guard: XrefGuard::default(),
            top_level_section_ids: Rc::new(
                doc.toc_entries
                    .iter()
                    .filter(|entry| entry.level == 1)
                    .map(|entry| entry.id)
                    .collect(),
            ),
            static_media_warning: Rc::new(Cell::new(false)),
            inline_role_warning: Rc::new(Cell::new(false)),
            index_entries: Rc::new(RefCell::new(Vec::new())),
            has_valid_index_section: has_index_section(&doc.blocks),
            #[cfg(feature = "pre-spec-subs")]
            current_subs: Rc::new(Cell::new(SubsFlags::all())),
        };
        let mut traversal = TraversalContext::new(&doc.attributes);
        let mut visitor = ManpageVisitor::new(writer, &processor, diagnostics.reborrow());
        visitor.visit_document(&mut traversal, doc)
    }

    /// Determine the output file extension based on the volume number.
    fn output_extension(doc: &Document<'_>) -> String {
        // Read manvolnum from document attributes (set by parser)
        acdc_converters_core::document_attribute_text((doc.attributes).get("manvolnum"))
            .map_or_else(|| String::from("1"), str::to_owned)
    }
}

impl<'a> Converter<'a> for Processor<'a> {
    type Error = Error;

    fn document_attributes_defaults() -> &'static [(&'static str, AttributeValue<'static>)] {
        &[(
            "man-linkstyle",
            AttributeValue::String(Cow::Borrowed("blue R < >")),
        )]
    }

    fn new(
        options: Options,
        parser_options: acdc_parser::OptionsBuilder<'a>,
    ) -> Result<Self, Self::Error> {
        let mut parser_options = parser_options;
        for (name, value) in Self::document_attributes_defaults() {
            if parser_options.attribute(name).is_none() {
                parser_options = parser_options.with_default_attribute(*name, value.clone());
            }
        }
        parser_options = parser_options.with_attribute("doctype", Doctype::Manpage.as_str());
        parser_options =
            MANPAGE_BACKEND.apply(parser_options, Doctype::Manpage, options.embedded());

        let parser_options = parser_options.build()?;
        Ok(Self {
            options,
            parser_options,
            references: Rc::new(HashMap::new()),
            xref_guard: XrefGuard::default(),
            top_level_section_ids: Rc::new(HashSet::new()),
            static_media_warning: Rc::new(Cell::new(false)),
            inline_role_warning: Rc::new(Cell::new(false)),
            index_entries: Rc::new(RefCell::new(Vec::new())),
            has_valid_index_section: false,
            #[cfg(feature = "pre-spec-subs")]
            current_subs: Rc::new(Cell::new(SubsFlags::all())),
        })
    }

    fn options(&self) -> &Options {
        &self.options
    }

    fn parser_options(&self) -> &ParserOptions<'a> {
        &self.parser_options
    }

    fn derive_output_path(
        &self,
        input: &Path,
        doc: &Document<'_>,
    ) -> Result<Option<PathBuf>, Error> {
        let extension = Self::output_extension(doc);
        let manpage_path = input.with_extension(&extension);
        // Avoid overwriting the input file
        if manpage_path == input {
            return Err(Error::OutputPathSameAsInput(input.to_path_buf()));
        }
        Ok(Some(manpage_path))
    }

    fn write_to<W: Write>(
        &self,
        doc: &Document<'_>,
        writer: W,
        source_file: Option<&Path>,
        _output_path: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Self::Error> {
        self.write_document(doc, writer, source_file, diagnostics)
    }

    fn name(&self) -> &'static str {
        "manpage"
    }
}

/// Get a file's modification date as a `YYYY-MM-DD` string.
fn file_modified_date(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let datetime: chrono::DateTime<chrono::Local> = modified.into();
    Some(datetime.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_applies_manpage_backend_profile() -> Result<(), Box<dyn std::error::Error>> {
        let processor = Processor::new(Options::default(), ParserOptions::builder())?;

        assert_eq!(
            processor
                .document_attributes()
                .get("backend")
                .and_then(|value| value.text()),
            Some("manpage")
        );
        assert_eq!(
            processor
                .document_attributes()
                .get("doctype")
                .and_then(|value| value.text()),
            Some("manpage")
        );
        assert_eq!(
            processor
                .document_attributes()
                .get("man-linkstyle")
                .and_then(|value| value.text()),
            Some("blue R < >")
        );
        assert!(
            processor
                .document_attributes()
                .contains_key("backend-manpage-doctype-manpage")
        );
        Ok(())
    }

    #[test]
    fn canonical_body_attributes_apply_in_source_order() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= attr-order(1)\n:doctype: manpage\n:manname: attr-order\n:manpurpose: show attribute source order\n:imagesdir: images/header\n\n== Name\n\naudio::before.mp3[]\n\n:imagesdir: images/body\n\naudio::after.mp3[]\n",
            &ParserOptions::default(),
        )?;
        let processor = Processor::new(
            Options::default(),
            ParserOptions::builder()
                .with_attributes(parsed.document().attributes.clone().into_inputs()),
        )?;
        let source = processor.warning_source();
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let mut output = Vec::new();

        processor.write_to(parsed.document(), &mut output, None, None, &mut diagnostics)?;

        let manpage = String::from_utf8(output)?;
        assert!(manpage.contains("images/header/before.mp3"));
        assert!(manpage.contains("images/body/after.mp3"));
        Ok(())
    }
}
