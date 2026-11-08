use acdc_converters_core::{
    Converter, Diagnostics, Options as ConverterOptions, TraversalContext, WarningSource,
    visitor::DocumentVisitor,
};
use acdc_parser::{Options as ParserOptions, parse};

use crate::{Capabilities, Processor, TerminalVisitor};

type Error = Box<dyn std::error::Error>;

const TEXT_TERMINAL: Capabilities = Capabilities {
    unicode: true,
    osc8_links: false,
};

#[test]
fn buffered_blocks_share_attributes_but_asciidoc_cells_restore_them() -> Result<(), Error> {
    for (wrapper, close, scoped) in [
        ("[NOTE]\n====", "====", false),
        ("====", "====", false),
        ("[quote]\n____", "____", false),
        ("****", "****", false),
        ("--", "--", false),
        ("* Item\n+\n--", "--", false),
        ("[cols=a]\n|===\n|", "|===", true),
    ] {
        let source = format!("= T\n\n{wrapper}\n:imagesdir: inside\n\nContent.\n{close}\n");
        let parsed = parse(&source, &ParserOptions::default())?;
        let document = parsed.document();
        let processor = Processor::new(
            ConverterOptions::default(),
            ParserOptions::builder().with_attributes(
                acdc_parser::DocumentAttributes::from(document.attributes()).into_inputs(),
            ),
        )?
        .with_terminal_capabilities(TEXT_TERMINAL);
        let mut traversal = TraversalContext::new(document.attributes());
        let source = WarningSource::new("terminal");
        let mut warnings = Vec::new();
        let diagnostics = Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(Vec::new(), &processor, diagnostics);
        visitor.visit_document(&mut traversal, document)?;
        drop(visitor);
        assert_eq!(
            traversal.get("imagesdir").and_then(|value| value.text()),
            if scoped { None } else { Some("inside") },
            "{wrapper}"
        );
    }
    Ok(())
}

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
    let source = WarningSource::new("terminal");
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    {
        let attribute_header =
            acdc_converters_core::Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(&mut output, &processor, diagnostics.reborrow());
        visitor.visit_unhandled_block(&mut traversal, block);
    }

    assert_eq!(output, [] as [u8; 0]);
    let warning = warnings.first().ok_or("missing fallback warning")?;
    assert_eq!(warning.source.converter, "terminal");
    assert!(warning.message.contains("omitted from terminal output"));
    assert!(warning.advice().is_some());
    Ok(())
}
