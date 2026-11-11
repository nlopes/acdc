//! Conversion entry points accept parsed, constructed, and extension-generated documents.

use std::path::Path;

use acdc_converters_core::{Converter as _, Diagnostics, Options, WarningSource};
use acdc_converters_html::{Converter, RenderOptions};
use acdc_parser::{
    Block, CompatibilityMode, Document, Error, InlineNode, Location, Paragraph, Plain,
    ast::DocumentRef,
    extensions::{
        BlockBuilder, BlockMacro, BlockMacroOutput, BlockMacroProcessor, DocumentEditor,
        MacroInvocation, Registry, TreeProcessor, TreeProcessorRegistration,
    },
    parse,
};

#[test]
fn parsed_and_constructed_documents_share_conversion_entrypoints()
-> Result<(), Box<dyn std::error::Error>> {
    let converter = Converter::new(
        Options::builder().embedded(true).build(),
        acdc_parser::Options::builder(),
    )?;
    let parsed = parse("Hello.", converter.parser_options())?;
    let mut constructed = Document::default();
    constructed.push_block(Block::Paragraph(Paragraph::new(
        vec![InlineNode::PlainText(Plain::new(
            "Hello.",
            Location::default(),
        ))],
        Location::default(),
    )));
    let options = RenderOptions {
        embedded: true,
        ..RenderOptions::default()
    };
    let expected = converter.convert_to_string(parsed.document(), &options)?;
    for document in [parsed.document(), DocumentRef::from(&constructed)] {
        assert_eq!(converter.convert_to_string(document, &options)?, expected);
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let mut output = Vec::new();
        converter.convert_to_writer(document, &mut output, &options, &mut diagnostics)?;
        assert_eq!(String::from_utf8(output)?, expected);
        let mut output = Vec::new();
        converter.write_to(document, &mut output, None, None, &mut diagnostics)?;
        assert_eq!(String::from_utf8(output)?, expected);
        assert_eq!(
            converter.derive_output_path(Path::new("sample.adoc"), document)?,
            Some(Path::new("sample.html").to_path_buf())
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(document.blocks().count(), 1);
    }
    Ok(())
}

struct ListRequest;

impl BlockMacroProcessor for ListRequest {
    fn process(&mut self, _: &MacroInvocation<'_>) -> Result<BlockMacroOutput, String> {
        Ok(BlockMacroOutput::Deferred)
    }
}

struct FillList;

impl TreeProcessor for FillList {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let targets: Vec<_> = editor
            .deferred_blocks()
            .iter()
            .map(|request| request.target().clone())
            .collect();
        for target in targets {
            editor.replace_placeholder(
                &target,
                BlockBuilder::unordered_list(["*literal* <&>", "{counter:x}"]),
            )?;
        }
        Ok(())
    }
}

#[test]
fn extension_generated_lists_preserve_metadata_references_and_literal_text()
-> Result<(), Box<dyn std::error::Error>> {
    let mut builder = Registry::builder();
    builder.register(BlockMacro::new("request", || ListRequest))?;
    builder.register_tree_processor(TreeProcessorRegistration::new("fill", || FillList))?;
    let registry = builder.build();
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let converter = Converter::new(
            Options::builder().embedded(true).build(),
            acdc_parser::Options::builder().with_compatibility_mode(mode),
        )?;
        let parsed = registry
            .parse(
                ".Generated *title*\n[#generated]\nrequest::[]\n\nSee <<generated>>.",
                converter.parser_options(),
            )?
            .finish()?;
        let output = converter.convert_to_string(
            parsed.document(),
            &RenderOptions {
                embedded: true,
                ..RenderOptions::default()
            },
        )?;
        for fragment in [
            "id=\"generated\"",
            "Generated <strong>title</strong>",
            "href=\"#generated\"",
            "*literal* &lt;&amp;&gt;",
            "{counter:x}",
        ] {
            assert!(output.contains(fragment), "{mode:?}: missing {fragment}");
        }
        assert!(!output.contains("&amp;lt;"));
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    }
    Ok(())
}
