#![allow(clippy::expect_used)]

use acdc_parser::{
    ElementAttributes, Error,
    extensions::{
        BlockBuilder, BlockMacro, BlockMacroOutput, BlockMacroProcessor, DocumentEditor,
        MacroInvocation, Registry, TreeProcessor, TreeProcessorRegistration,
    },
};

#[derive(Default)]
struct Record {
    calls: usize,
}

impl BlockMacroProcessor for Record {
    fn process(
        &mut self,
        invocation: &MacroInvocation<'_>,
        _: &mut acdc_parser::extensions::MacroExecutionContext,
    ) -> Result<BlockMacroOutput, String> {
        self.calls += 1;
        let text = serde_json::json!({
            "call": self.calls,
            "target": invocation.target,
            "positional": invocation.positional,
            "attributes": invocation.attributes,
            "mode": invocation.document_attributes.get("mode").and_then(acdc_parser::DocumentAttributeValue::as_str),
        });
        Ok(BlockMacroOutput::Paragraph(text.to_string()))
    }
}

pub(super) struct Defer;

impl BlockMacroProcessor for Defer {
    fn process(
        &mut self,
        _: &MacroInvocation<'_>,
        _: &mut acdc_parser::extensions::MacroExecutionContext,
    ) -> Result<BlockMacroOutput, String> {
        Ok(BlockMacroOutput::Deferred)
    }
}

pub(super) fn registry() -> Registry {
    let mut defaults = ElementAttributes::default();
    defaults.set("first".into(), "fallback".into());
    let mut builder = Registry::builder();
    builder
        .register(
            BlockMacro::new("record", Record::default)
                .with_positional_attributes(["first", "second", "third"])
                .with_defaults(defaults),
        )
        .expect("valid fixture macro");
    builder
        .register(BlockMacro::new("defer", || Defer))
        .expect("valid fixture macro");
    builder
        .register(BlockMacro::new("configure", || configure).with_positional_attributes(["value"]))
        .expect("valid fixture macro");
    builder
        .register(BlockMacro::new("query", || Defer))
        .expect("valid query fixture macro");
    builder
        .register_tree_processor(TreeProcessorRegistration::new("query", || Query))
        .expect("valid query fixture processor");
    builder.build()
}

struct Query;

impl TreeProcessor for Query {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let requests: Vec<_> = editor
            .deferred_blocks()
            .iter()
            .filter(|request| request.invocation.name == "query")
            .map(|request| request.target().clone())
            .collect();
        if requests.is_empty() {
            return Ok(());
        }
        let targets: Vec<_> = editor
            .query()
            .blocks()
            .filter(|view| view.attributes().is_some() && !requests.contains(view.target()))
            .map(|view| view.target().clone())
            .collect();
        for target in &targets {
            let id = editor.ensure_id(target)?.to_owned();
            assert_eq!(editor.ensure_id(target)?, id);
        }
        let query = editor.query();
        let observations: Vec<_> = query.blocks()
            .filter(|view| !requests.contains(view.target()))
            .map(|view| {
                let title = view.title_source();
                serde_json::json!({
                    "context": format!("{:?}", view.context()),
                    "id": view.output_id(),
                    "section": view.containing_section().and_then(|target| query.get(target)).and_then(|section| section.output_id()),
                    "raw_title": title.map(acdc_parser::extensions::TitleSource::raw_source),
                    "title_line": title.map(|title| title.source_location().location.start.line),
                    "substitutions": title.map(acdc_parser::extensions::TitleSource::substitutions),
                    "label": title.and_then(|title| title.attributes().get("label")).and_then(acdc_parser::DocumentAttributeValue::as_str),
                    "caption": view.caption().map(|caption| format!("{caption:?}")),
                })
            }).collect();
        let text = serde_json::Value::Array(observations).to_string();
        for request in requests {
            editor.replace_placeholder(&request, BlockBuilder::paragraph(text.clone()))?;
        }
        Ok(())
    }
}

pub(super) fn configure(
    invocation: &MacroInvocation<'_>,
    context: &mut acdc_parser::extensions::MacroExecutionContext,
) -> Result<BlockMacroOutput, String> {
    if invocation.target.is_empty() {
        return Err("configure requires an attribute name".into());
    }
    let name = invocation.target.to_string();
    if invocation.attributes.get("unset").is_some() {
        context.unset_attribute(name);
    } else {
        let value = invocation
            .attributes
            .get("value")
            .and_then(|value| {
                if let acdc_parser::AttributeValue::String(text) = value {
                    Some(text.as_ref())
                } else {
                    None
                }
            })
            .unwrap_or("");
        context.set_attribute(name, value.to_owned());
    }
    Ok(BlockMacroOutput::None)
}
