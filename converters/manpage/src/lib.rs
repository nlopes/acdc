//! Manpage converter for `AsciiDoc` documents.
//!
//! This converter outputs native roff/troff format suitable for the `man` command.
//! It targets modern GNU groff and produces semantically equivalent output to
//! Asciidoctor's manpage backend.
//!
//! # Example
//!
//! ```ignore
//! use acdc_converters_manpage::Processor;
//! use acdc_converters_core::{Converter, Options};
//!
//! let options = Options::default();
//! let processor = Processor::new(options, Default::default());
//! processor.convert(&document, Some(Path::new("cmd.adoc")))?;
//! // Outputs: cmd.1 (or other extension based on volume number)
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
    BackendTraits, Converter, Diagnostics, Doctype, Options, section::last_section_has_style,
    visitor::Visitor, xref::XrefGuard,
};

use acdc_parser::{AttributeValue, Document, DocumentAttributes, Reference};

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

/// Intrinsic traits for the manpage backend.
const BACKEND_TRAITS: BackendTraits = BackendTraits::new("manpage", "manpage", "man", ".man");

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
    document_attributes: DocumentAttributes<'a>,
    pub(crate) references: Rc<HashMap<&'a str, Reference<'a>>>,
    /// Keeps a cross-reference inside a resolved target's text from recursing.
    pub(crate) xref_guard: XrefGuard,
    pub(crate) top_level_section_ids: Rc<HashSet<&'a str>>,
    pub(crate) static_media_warning: Rc<Cell<bool>>,
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

        if !attrs.contains_key("revdate")
            && let Some(date_str) = source_file.and_then(file_modified_date)
        {
            attrs.insert(
                "revdate".into(),
                AttributeValue::String(Cow::Owned(date_str)),
            );
        }

        // Per-conversion processor borrows from `doc`; lifetime independent of `self`.
        let processor: Processor<'doc> = Processor {
            options: self.options.clone(),
            document_attributes: attrs,
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
            index_entries: Rc::new(RefCell::new(Vec::new())),
            has_valid_index_section: last_section_has_style(&doc.blocks, "index"),
            #[cfg(feature = "pre-spec-subs")]
            current_subs: Rc::new(Cell::new(SubsFlags::all())),
        };
        let mut visitor = ManpageVisitor::new(writer, processor, diagnostics.reborrow());
        visitor.visit_document(doc)
    }

    /// Determine the output file extension based on the volume number.
    fn output_extension(doc: &Document) -> String {
        // Read manvolnum from document attributes (set by parser)
        doc.attributes
            .get("manvolnum")
            .and_then(|v| match v {
                acdc_parser::AttributeValue::String(s) => Some(s.clone().into_owned()),
                acdc_parser::AttributeValue::Bool(_) | acdc_parser::AttributeValue::None | _ => {
                    None
                }
            })
            .unwrap_or_else(|| String::from("1"))
    }
}

impl<'a> Converter<'a> for Processor<'a> {
    type Error = Error;

    fn document_attributes_defaults() -> DocumentAttributes<'static> {
        let mut attrs: DocumentAttributes<'static> = DocumentAttributes::default();
        // man-linkstyle controls how links are rendered in the manpage
        // Format: "color style <text>" - blue R <> means blue, regular, angle brackets
        attrs.insert("man-linkstyle".into(), "blue R <>".into());
        attrs
    }

    fn new(options: Options, document_attributes: DocumentAttributes<'a>) -> Self {
        let mut document_attributes = document_attributes;
        for (name, value) in Self::document_attributes_defaults().iter() {
            document_attributes.insert(name.clone(), value.clone());
        }
        document_attributes.set("doctype".into(), Doctype::Manpage.as_str().into());
        BACKEND_TRAITS.apply(&mut document_attributes, Doctype::Manpage);

        Self {
            options,
            document_attributes,
            references: Rc::new(HashMap::new()),
            xref_guard: XrefGuard::default(),
            top_level_section_ids: Rc::new(HashSet::new()),
            static_media_warning: Rc::new(Cell::new(false)),
            index_entries: Rc::new(RefCell::new(Vec::new())),
            has_valid_index_section: false,
            #[cfg(feature = "pre-spec-subs")]
            current_subs: Rc::new(Cell::new(SubsFlags::all())),
        }
    }

    fn options(&self) -> &Options {
        &self.options
    }

    fn document_attributes(&self) -> &DocumentAttributes<'a> {
        &self.document_attributes
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
    fn constructor_applies_manpage_backend_traits() {
        let processor = Processor::new(Options::default(), DocumentAttributes::default());

        assert_eq!(
            processor
                .document_attributes()
                .get_string("backend")
                .as_deref(),
            Some("manpage")
        );
        assert_eq!(
            processor
                .document_attributes()
                .get_string("doctype")
                .as_deref(),
            Some("manpage")
        );
        assert!(
            processor
                .document_attributes()
                .contains_key("backend-manpage-doctype-manpage")
        );
    }
}
