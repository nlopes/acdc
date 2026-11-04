//! Conversion entry points accept parsed and caller-constructed typed documents.

use std::path::Path;

use acdc_converters_core::{Converter, Diagnostics, Options, WarningSource};
use acdc_converters_html::{Processor, RenderOptions};
use acdc_parser::{
    Block, Document, InlineNode, Location, Paragraph, Plain, ast::DocumentRef, parse,
};

#[test]
fn parsed_and_constructed_documents_share_conversion_entrypoints()
-> Result<(), Box<dyn std::error::Error>> {
    let processor = Processor::new(
        Options::builder().embedded(true).build(),
        acdc_parser::Options::builder(),
    )?;
    let parsed = parse("Hello.", processor.parser_options())?;
    let mut constructed = Document::default();
    constructed.blocks.push(Block::Paragraph(Paragraph::new(
        vec![InlineNode::PlainText(Plain {
            content: "Hello.",
            location: Location::default(),
            escaped: false,
        })],
        Location::default(),
    )));
    let options = RenderOptions {
        embedded: true,
        ..RenderOptions::default()
    };
    let expected = processor.convert_to_string(parsed.document_ref(), &options)?;
    for document in [parsed.document_ref(), DocumentRef::from(&constructed)] {
        assert_eq!(processor.convert_to_string(document, &options)?, expected);
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let mut output = Vec::new();
        processor.convert_to_writer(document, &mut output, &options, &mut diagnostics)?;
        assert_eq!(String::from_utf8(output)?, expected);
        let mut output = Vec::new();
        processor.write_to(document, &mut output, None, None, &mut diagnostics)?;
        assert_eq!(String::from_utf8(output)?, expected);
        assert_eq!(
            processor.derive_output_path(Path::new("sample.adoc"), document)?,
            Some(Path::new("sample.html").to_path_buf())
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(document.blocks().count(), 1);
    }
    Ok(())
}
