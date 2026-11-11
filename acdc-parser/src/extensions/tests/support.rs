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
    fn process(&mut self, invocation: &MacroInvocation<'_>) -> Result<BlockMacroOutput, String> {
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
    fn process(&mut self, _: &MacroInvocation<'_>) -> Result<BlockMacroOutput, String> {
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
    builder.build()
}
