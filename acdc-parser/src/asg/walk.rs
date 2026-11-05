use serde_json::{Map, Value};

use super::{Error, pointer};

pub(super) fn objects(
    node: &Map<String, Value>,
    path: &str,
    excluded: &[&str],
    visitor: &mut impl FnMut(&Map<String, Value>, &str) -> Result<(), Error>,
) -> Result<(), Error> {
    visitor(node, path)?;
    for (name, value) in node {
        if !matches!(
            name.as_str(),
            "attributes"
                | "metadata"
                | "location"
                | "id_location"
                | "delimited_location"
                | "callout_location"
        ) && !excluded.contains(&name.as_str())
        {
            visit(value, &pointer(path, name), excluded, visitor)?;
        }
    }
    Ok(())
}

fn visit(
    value: &Value,
    path: &str,
    excluded: &[&str],
    visitor: &mut impl FnMut(&Map<String, Value>, &str) -> Result<(), Error>,
) -> Result<(), Error> {
    match value {
        Value::Object(node) => objects(node, path, excluded, visitor),
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                visit(value, &pointer(path, &index.to_string()), excluded, visitor)?;
            }
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(()),
    }
}
