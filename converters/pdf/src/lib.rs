//! PDF converter for `AsciiDoc` documents.
//!
//! This converter renders the acdc AST into a generated Typst document and then
//! uses Typst's Rust compiler and PDF exporter in-process. The first supported
//! surface focuses on high-quality paged output for core text documents:
//! headings, paragraphs, inline formatting, lists, tables, admonitions, links,
//! footnotes, table of contents, page breaks, and thematic breaks.

use acdc_converters_core::{Diagnostics, Options, visitor::Visitor};
use acdc_parser::{Document, DocumentAttributes};
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;

mod converter;
mod error;
mod pdf_visitor;
mod visitor;

pub use error::Error;

use pdf_visitor::PdfVisitor;

const MAIN_TYP: &str = "main.typ";

fn escape_math(value: &str) -> String {
    value.replace('$', "\\$")
}

/// PDF converter processor.
#[derive(Clone, Debug)]
pub struct Processor<'a> {
    options: Options,
    document_attributes: DocumentAttributes<'a>,
}

impl Processor<'_> {
    /// Convert a parsed document into generated Typst source.
    ///
    /// This is exposed for tests and for diagnosing PDF conversion without
    /// having to inspect binary PDF bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the AST traversal fails.
    pub fn convert_to_typst_source(
        &self,
        doc: &Document<'_>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<String, Error> {
        let processor = Processor {
            options: self.options.clone(),
            document_attributes: doc.attributes.clone(),
        };
        let mut visitor = PdfVisitor::new(processor, diagnostics.reborrow());
        visitor.visit_document(doc)?;
        Ok(visitor.source)
    }

    fn compile_pdf(source: &str) -> Result<Vec<u8>, Error> {
        let engine = TypstEngine::builder()
            .with_static_source_file_resolver([(MAIN_TYP, source)])
            .fonts(typst_assets::fonts())
            .build();

        let warned = engine.compile::<_, PagedDocument>(MAIN_TYP);
        let document = warned
            .output
            .map_err(|err| Error::TypstCompile(format!("{err:?}")))?;
        typst_pdf::pdf(&document, &PdfOptions::default())
            .map_err(|err| Error::PdfExport(format!("{err:?}")))
    }
}

fn typst_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn sanitize_label(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == ':' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "anchor".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use acdc_converters_core::{Converter, WarningSource};

    use super::*;

    #[test]
    fn typst_string_escapes_control_characters() {
        assert_eq!(
            typst_string("a \"quote\" \\ path\nnext"),
            "\"a \\\"quote\\\" \\\\ path\\nnext\""
        );
    }

    #[test]
    fn labels_are_typst_safe() {
        assert_eq!(sanitize_label("_hello world!"), "_hello_world_");
        assert_eq!(sanitize_label(""), "anchor");
    }

    #[test]
    fn converts_simple_document_to_typst_source() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Title\n\n== Section\n\nA *bold* link:https://example.com[link].\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;

        assert!(typst.contains("#heading(level: 1)"));
        assert!(typst.contains("#strong["));
        assert!(typst.contains("#link(\"https://example.com\")"));
        Ok(())
    }

    #[test]
    fn renders_simple_pdf_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "= Title\n\n== Section\n\nA paragraph.\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = WarningSource::new("pdf");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let typst = processor.convert_to_typst_source(parsed.document(), &mut diagnostics)?;
        let pdf = Processor::compile_pdf(&typst)?;

        assert!(pdf.starts_with(b"%PDF-"));
        Ok(())
    }
}
