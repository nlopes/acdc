//! Checked export to the pinned, incomplete `AsciiDoc` ASG draft.
//!
//! This format is separate from native AST JSON. Export reads resolved AST values
//! directly and fails if their meaning has no supported mapping. It never reparses
//! strings, changes the AST, or runs a converter.

mod metadata;
mod projection;

use std::sync::OnceLock;

use serde_json::{Map, Value};

use crate::{Document, InlineNode};

/// Schema identifier used by this exporter.
pub const SCHEMA_ID: &str = "https://schemas.asciidoc.org/asg/1-0-0/draft-01";
/// Upstream revision, independent of the schema's draft identifier.
pub const SCHEMA_REVISION: &str = "68ed0b22e8d9b919897542d8ae14f03e7dbdd2e4";

static VALIDATOR: OnceLock<Result<jsonschema::Validator, String>> = OnceLock::new();

/// A document cannot be represented by the pinned ASG subset.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The vendored schema could not be loaded.
    #[error("could not load ASG schema: {0}")]
    Schema(String),
    /// A semantic value has no supported mapping. `path` is a JSON Pointer into
    /// the attempted output, or a native property when no output field exists.
    #[error("unsupported ASG value at {path}: {message}")]
    Unsupported { path: String, message: String },
    /// Available source coordinates violate the pinned location contract.
    #[error("invalid ASG location at {path}: {message}")]
    InvalidLocation { path: String, message: String },
}

/// Export a document without changing its native AST or derived catalogs.
///
/// # Errors
/// Returns an error for any unsupported nested value or invalid location. No
/// partial output is returned. Success preserves the documented supported
/// semantics and validates against [`SCHEMA_REVISION`]; it does not establish
/// source round-trip fidelity or complete language conformance.
pub fn document(document: &Document<'_>) -> Result<Value, Error> {
    let value = projection::document(document)?;
    validate(&value, "")?;
    Ok(value)
}

/// Export resolved inline content as an array, using the document export rules.
///
/// # Errors
/// Returns an error for unsupported content or locations, including unresolved
/// passthrough operations and unsupported identity or controls on inline spans.
pub fn inlines(inlines: &[InlineNode<'_>]) -> Result<Value, Error> {
    let value = projection::inlines(inlines, "")?;
    let paragraph = Map::from_iter([
        ("name".into(), "paragraph".into()),
        ("type".into(), "block".into()),
        ("inlines".into(), value),
    ]);
    let mut wrapper = Value::Object(Map::from_iter([
        ("name".into(), "document".into()),
        ("type".into(), "block".into()),
        (
            "blocks".into(),
            Value::Array(vec![Value::Object(paragraph)]),
        ),
    ]));
    validate(&wrapper, "/blocks/0/inlines")?;
    wrapper
        .pointer_mut("/blocks/0/inlines")
        .map(Value::take)
        .ok_or_else(|| Error::Schema("validated inline wrapper has no content".into()))
}

fn unsupported(path: &str, message: impl Into<String>) -> Error {
    Error::Unsupported {
        path: path.into(),
        message: message.into(),
    }
}

fn pointer(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn validate(value: &Value, prefix: &str) -> Result<(), Error> {
    let validator = VALIDATOR
        .get_or_init(|| {
            let schema = serde_json::from_slice(include_bytes!("../../schemas/asg/schema.json"))
                .map_err(|error| error.to_string())?;
            jsonschema::validator_for(&schema).map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|message| Error::Schema(message.clone()))?;
    validator.validate(value).map_err(|error| {
        let path = error.instance_path().to_string();
        unsupported(
            path.strip_prefix(prefix).unwrap_or(&path),
            error.to_string(),
        )
    })
}
