use serde_json::{Map, Value, json};

use crate::StemNotation;

pub(super) fn apply(
    node: &mut Map<String, Value>,
    text: &str,
    notation: &StemNotation,
    escaping: bool,
) {
    node.insert("notation".into(), notation.to_string().into());
    let content = if text.is_empty() {
        Vec::new()
    } else {
        vec![json!({"name": if escaping { "text" } else { "raw" }, "type":"string", "value": text})]
    };
    node.insert("inlines".into(), Value::Array(content));
}
