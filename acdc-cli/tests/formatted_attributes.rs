#![cfg(all(feature = "html", feature = "pdf", feature = "manpage"))]

use acdc_converters_core::{Converter, Diagnostics, Options as ConverterOptions, WarningSource};
use acdc_parser::{Document, Options, parse};

type Error = Box<dyn std::error::Error>;

fn render<'a, C: Converter<'a>>(converter: &C, doc: &Document<'_>) -> Result<Vec<u8>, Error> {
    let source = WarningSource::new(converter.name());
    let mut warnings = Vec::new();
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    let mut output = Vec::new();
    converter
        .write_to(doc, &mut output, None, None, &mut diagnostics)
        .map_err(|error| error.to_string())?;
    assert_ne!(output, [] as [u8; 0]);
    Ok(output)
}

#[test]
fn formatted_attribute_document_can_be_parsed_once_for_three_backends() -> Result<(), Error> {
    let parsed = parse(
        include_str!("../../acdc-parser/fixtures/tests/document_attribute_formatted.adoc"),
        &Options::default(),
    )?;
    let doc = parsed.document();
    let text = serde_json::to_value(&doc.attributes)?;
    let tree = serde_json::to_value(doc)?;
    let notes = doc.footnotes.clone();
    let html = acdc_converters_html::Processor::new(
        ConverterOptions::builder().embedded(true).build(),
        Options::builder(),
    )?;
    let manpage =
        acdc_converters_manpage::Processor::new(ConverterOptions::default(), Options::builder())?;
    let pdf = acdc_converters_pdf::Processor::new(ConverterOptions::default(), Options::builder())?;
    let output = render(&html, doc)?;
    assert!(String::from_utf8(output)?.contains("<strong>Early</strong>"));
    let output = render(&manpage, doc)?;
    assert!(String::from_utf8(output)?.contains("Early"));
    let output = render(&pdf, doc)?;
    assert!(output.starts_with(b"%PDF-"));
    assert_eq!(text, serde_json::to_value(&doc.attributes)?);
    assert_eq!(tree, serde_json::to_value(doc)?);
    assert_eq!(notes, doc.footnotes);
    Ok(())
}
