use std::{error::Error, fmt::Write};

use jsonschema::Validator;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(super) type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
pub(super) const REVISION: &str = "68ed0b22e8d9b919897542d8ae14f03e7dbdd2e4";
pub(super) const ID: &str = "https://schemas.asciidoc.org/asg/1-0-0/draft-01";
pub(super) const SHA256: &str = "61fc73c612d7fe79e795dc0db2fd8bf8393d146dfcc8815fa7feca864f769675";
pub(super) const BYTES: &[u8] = include_bytes!("../../schemas/asg/schema.json");

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(super) struct Failure {
    pub(super) instance_path: String,
    pub(super) schema_path: String,
    pub(super) message: String,
}

pub(super) fn failures(validator: &Validator, value: &Value) -> Vec<Failure> {
    validator
        .iter_errors(value)
        .map(|error| Failure {
            instance_path: error.instance_path().to_string(),
            schema_path: error.schema_path().to_string(),
            message: error.to_string(),
        })
        .collect()
}

pub(super) fn load() -> Result<(Value, Validator, Validator)> {
    let mut actual = String::with_capacity(64);
    for byte in Sha256::digest(BYTES) {
        write!(&mut actual, "{byte:02x}")?;
    }
    if actual != SHA256 {
        return Err("vendored ASG schema hash differs from the pinned revision".into());
    }
    let schema: Value = serde_json::from_slice(BYTES)?;
    if schema.get("$id").and_then(Value::as_str) != Some(ID) {
        return Err("vendored ASG schema identifier differs from its provenance".into());
    }
    jsonschema::meta::validate(&schema).map_err(|error| error.to_string())?;
    let validator = jsonschema::validator_for(&schema).map_err(|error| error.to_string())?;
    let locations = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$ref": "#/$defs/location", "$defs": schema.get("$defs").ok_or("missing schema definitions")?,
    });
    let locations = jsonschema::validator_for(&locations).map_err(|error| error.to_string())?;
    Ok((schema, validator, locations))
}

pub(super) fn controls(validator: &Validator) -> Result<Value> {
    let valid = json!({"name":"document","type":"block","attributes":{},
        "header":{"title":[{"name":"span","type":"inline","variant":"strong",
            "form":"constrained","inlines":[{"name":"text","type":"string","value":"Correct"}]}]}});
    if !validator.is_valid(&valid) {
        return Err(format!(
            "positive validator control failed: {:?}",
            failures(validator, &valid)
        )
        .into());
    }
    let mut malformed = valid.clone();
    *malformed
        .pointer_mut("/header/title")
        .ok_or("missing control title")? = json!("not an inline array");
    if validator.is_valid(&malformed) {
        return Err("negative title control was accepted".into());
    }
    let mut wrong = valid.clone();
    *wrong
        .pointer_mut("/header/title/0/inlines/0/value")
        .ok_or("missing control text")? = json!("Wrong meaning");
    if !validator.is_valid(&wrong) {
        return Err("schema-valid semantic-limit control was rejected".into());
    }
    let unknown = json!({"name":"document","type":"block","unexpected":true});
    if validator.is_valid(&unknown) {
        return Err("unknown-property control was accepted".into());
    }
    Ok(
        json!({"formatted_title_valid":true,"malformed_title_rejected":true,
        "unknown_property_rejected":true,"semantically_wrong_title_is_schema_valid":true,
        "limit":"Schema validity does not prove semantic preservation or AsciiDoc conformance."}),
    )
}

pub(super) fn pointer(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}

pub(super) fn locations(
    value: &Value,
    path: &str,
    validator: &Validator,
    findings: &mut Vec<Value>,
) {
    match value {
        Value::Object(object) => {
            if let Some(location) = object.get("location") {
                let at = pointer(path, "location");
                let issues = failures(validator, location);
                if !issues.is_empty() {
                    findings.push(json!({"path":at,"kind":"invalid_or_unknown_location",
                        "value":location,"errors":issues}));
                }
            } else if object.contains_key("name") && object.contains_key("type") {
                findings.push(json!({"path":path,"kind":"missing_node_location"}));
            }
            for (key, child) in object {
                locations(child, &pointer(path, key), validator, findings);
            }
        }
        Value::Array(array) => {
            for (index, child) in array.iter().enumerate() {
                locations(
                    child,
                    &pointer(path, &index.to_string()),
                    validator,
                    findings,
                );
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}
