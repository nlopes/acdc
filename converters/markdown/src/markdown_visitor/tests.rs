use acdc_converters_core::{
    Converter as _, Diagnostics, Options as ConverterOptions, TraversalContext, WarningSource,
};
use acdc_parser::{Options as ParserOptions, parse};

use crate::{Converter, MarkdownVisitor};

type Error = Box<dyn std::error::Error>;

#[test]
fn unhandled_blocks_emit_a_structured_warning_with_source_context() -> Result<(), Error> {
    let parsed = parse("Paragraph.\n", &ParserOptions::default())?;
    let doc = parsed.document();
    let block = doc.blocks().next().ok_or("missing test block")?;
    let converter = Converter::new(
        ConverterOptions::default(),
        ParserOptions::builder()
            .with_attributes(acdc_parser::DocumentAttributes::from(doc.attributes()).into_inputs()),
    )?;
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("markdown");
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    {
        let attribute_header =
            acdc_converters_core::Converter::document_attributes(&converter).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = MarkdownVisitor::new(&mut output, &converter, diagnostics.reborrow());
        visitor.visit_unhandled_block(&mut traversal, block);
    }

    assert_eq!(output, [] as [u8; 0]);
    let warning = warnings.first().ok_or("missing fallback warning")?;
    assert!(
        warning.message.contains("unknown parser block feature"),
        "{warnings:?}"
    );
    assert!(warning.advice().is_some(), "{warnings:?}");
    assert!(warning.source_location().is_some(), "{warnings:?}");
    Ok(())
}
