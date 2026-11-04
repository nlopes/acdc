use serde_json::{Map, Value};

use crate::{BlockMetadata, Caption, CaptionKind};

use super::{Error, pointer, projection::inlines};

pub(super) fn apply(
    node: &mut Map<String, Value>,
    value: &BlockMetadata<'_>,
    path: &str,
) -> Result<(), Error> {
    for (name, content) in [
        ("attribution", value.attribution.as_ref()),
        ("citetitle", value.citetitle.as_ref()),
    ] {
        if let Some(content) = content {
            node.insert(name.into(), inlines(content, &pointer(path, name))?);
        }
    }
    if let Some(value) = &value.caption {
        node.insert("caption".into(), caption(value, &pointer(path, "caption"))?);
    }
    Ok(())
}

fn caption(value: &Caption<'_>, path: &str) -> Result<Value, Error> {
    let mut result = Map::new();
    match value {
        Caption::Unnumbered => {
            result.insert("kind".into(), "unnumbered".into());
        }
        Caption::Custom(text) => {
            result.insert("kind".into(), "custom".into());
            let content = if text.is_empty() {
                Vec::new()
            } else {
                vec![serde_json::json!({"name":"text", "type":"string", "value":text})]
            };
            result.insert("prefix".into(), Value::Array(content));
        }
        Caption::CustomContent(content) => {
            result.insert("kind".into(), "custom".into());
            result.insert("prefix".into(), inlines(content, &pointer(path, "prefix"))?);
        }
        Caption::Numbered {
            kind,
            label,
            number,
        } => {
            result.insert("kind".into(), "numbered".into());
            let counter = match kind {
                CaptionKind::Example => "example",
                CaptionKind::Figure => "figure",
                CaptionKind::Listing => "listing",
                CaptionKind::Table => "table",
            };
            result.insert("counter".into(), counter.into());
            result.insert("label".into(), label.as_ref().into());
            if let Some(number) = number {
                result.insert("number".into(), number.get().into());
            }
        }
    }
    Ok(Value::Object(result))
}
