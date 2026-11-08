use acdc_converters_core::{
    Converter, Diagnostics, Options as ConverterOptions, TraversalContext, WarningSource,
};
use acdc_parser::{Options as ParserOptions, parse};

use crate::{ManpageVisitor, Processor};

type Error = Box<dyn std::error::Error>;

#[test]
fn unhandled_parser_block_warning_is_structured() -> Result<(), Error> {
    let parsed = parse("Paragraph.\n", &ParserOptions::default())?;
    let doc = parsed.document();
    let block = doc.blocks().next().ok_or("missing test block")?;
    let processor = Processor::new(
        ConverterOptions::default(),
        ParserOptions::builder()
            .with_attributes(acdc_parser::DocumentAttributes::from(doc.attributes()).into_inputs()),
    )?;
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("manpage");
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    {
        let attribute_header =
            acdc_converters_core::Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = ManpageVisitor::new(&mut output, &processor, diagnostics.reborrow());
        visitor.visit_unhandled_block(&mut traversal, block);
    }

    assert_eq!(output, [] as [u8; 0]);
    let warning = warnings.first().ok_or("missing fallback warning")?;
    assert_eq!(warning.source.converter, "manpage");
    assert!(warning.message.contains("omitted from manpage output"));
    assert!(warning.advice().is_some());
    Ok(())
}
