//! A diagram-shaped consumer using only public extension contracts.
use acdc_converters_core::{Converter as _, Options};
use acdc_converters_html::{Converter, RenderOptions};
use acdc_parser::{
    CaptionKind, CompatibilityMode, Error,
    ast::{AttributeValueRef, BlockKind},
    extensions::{
        BlockBuilder, BlockContext, BlockMacro, BlockMacroOutput, DocumentEditor, MacroInvocation,
        Registry, TreeProcessor, TreeProcessorRegistration,
    },
};

struct DiagramProducer;
impl TreeProcessor for DiagramProducer {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let diagrams: Vec<_> = editor
            .query()
            .select(BlockContext::Listing)
            .filter(|view| {
                view.attributes()
                    .and_then(|attributes| attributes.get("style"))
                    .is_some_and(|style| matches!(style, AttributeValueRef::Text("diagram")))
            })
            .map(|view| view.target().clone())
            .collect();
        for target in diagrams {
            editor.replace_block(
                &target,
                BlockBuilder::image("diagram.svg")
                    .with_attributes("'Alt <&>',320,200,link=target.html"),
            )?;
        }
        let requests: Vec<_> = editor
            .deferred_blocks()
            .iter()
            .map(|request| request.target().clone())
            .collect();
        for target in requests {
            editor.replace_placeholder(
                &target,
                BlockBuilder::literal("<&>\n*literal*\n  spaces")
                    .with_caption_kind(CaptionKind::Figure),
            )?;
        }
        Ok(())
    }
}

#[test]
fn diagram_consumer_builds_images_and_literal_figures_with_normal_rendering()
-> Result<(), Box<dyn std::error::Error>> {
    let source = "= Diagrams\n:imagesdir: images\n:figure-caption: Picture\n:label: Early\n\n.Image *{label}*\n[#diagram.diagram-role%nowrap,reftext='Caption *label*']\n[diagram]\n----\na -> b\n----\n\n.Text *{label}*\n[#text]\ndiagram::text[]\n\n:label: Late\n\nSee <<diagram>> and <<text>>.";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let mut builder = Registry::builder();
        builder.register(BlockMacro::new("diagram", || {
            |_: &MacroInvocation<'_>, _: &mut acdc_parser::extensions::MacroExecutionContext| {
                Ok(BlockMacroOutput::Deferred)
            }
        }))?;
        builder.register_tree_processor(TreeProcessorRegistration::new("diagram", || {
            DiagramProducer
        }))?;
        let converter = Converter::new(
            Options::builder().embedded(true).build(),
            acdc_parser::Options::builder().with_compatibility_mode(mode),
        )?;
        let mut working = builder.build().parse(source, converter.parser_options())?;
        let image = working
            .query()
            .select(BlockContext::Image)
            .next()
            .ok_or("image")?;
        assert!(matches!(image.block().kind(), BlockKind::Image(_)));
        working.normalize()?;
        working.normalize()?;
        let parsed = working.finish()?;
        assert_eq!(parsed.warnings(), []);
        let render = RenderOptions {
            embedded: true,
            ..Default::default()
        };
        let html = converter.convert_to_string(parsed.document(), &render)?;
        assert!(html.contains("src=\"images/diagram.svg\""), "{html}");
        assert!(html.contains("alt=\"Alt &lt;&amp;&gt;\""), "{html}");
        assert!(
            html.contains("Picture 1. Image <strong>Early</strong>"),
            "{html}"
        );
        assert!(
            html.contains("Picture 2. Text <strong>Early</strong>"),
            "{html}"
        );
        assert!(html.contains("&lt;&amp;&gt;\n*literal*"), "{html}");
        assert!(!html.contains("&amp;lt;"), "{html}");
        assert!(html.contains("href=\"#diagram\""), "{html}");
        assert!(html.contains("href=\"#text\""), "{html}");
    }
    Ok(())
}
