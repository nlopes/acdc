#![allow(clippy::expect_used)]

use acdc_parser::{
    ElementAttributes,
    extensions::{BlockMacro, BlockMacroOutput, BlockMacroProcessor, MacroInvocation, Registry},
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
    builder.build()
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
