//! Checked export to the pinned `AsciiDoc` ASG draft or the named acdc extension.
//!
//! This format is separate from native AST JSON. Export reads resolved AST values
//! directly and fails if their meaning has no supported mapping. It never reparses
//! strings, changes the AST, or runs a converter.

mod content;
mod diagnostics;
mod images;
mod metadata;
mod projection;
mod references;

use std::sync::OnceLock;

use serde_json::{Map, Value};

use crate::{Document, InlineNode, Location, Source};

/// Schema identifier used by this exporter.
pub const SCHEMA_ID: &str = "https://schemas.asciidoc.org/asg/1-0-0/draft-01";
/// Upstream revision, independent of the schema's draft identifier.
pub const SCHEMA_REVISION: &str = "68ed0b22e8d9b919897542d8ae14f03e7dbdd2e4";

static VALIDATOR: OnceLock<Result<jsonschema::Validator, String>> = OnceLock::new();

/// Identifier of the versioned acdc extension schema.
pub const ACDC_SCHEMA_ID: &str = "urn:acdc:asg:1";

/// The output contract, independent of the parser's compatibility mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum Profile {
    /// The pinned upstream subset; reject unsupported semantics.
    #[default]
    Upstream,
    /// The closed acdc extension schema identified by [`ACDC_SCHEMA_ID`].
    Acdc,
}

static ACDC_VALIDATOR: OnceLock<Result<jsonschema::Validator, String>> = OnceLock::new();

/// A document cannot be represented by the selected ASG profile.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The vendored schema could not be loaded.
    #[error("could not load ASG schema: {0}")]
    Schema(String),
    /// A semantic value has no supported mapping. `path` is a JSON Pointer into
    /// the attempted output, or a native property when no output field exists.
    #[error("unsupported ASG value at {path}: {message}")]
    Unsupported {
        /// JSON Pointer to the unsupported node or property.
        path: String,
        /// Native semantic owner of the value, when known.
        construct: Option<&'static str>,
        /// Known source coordinates of that owner; never fabricated.
        location: Option<Box<Location>>,
        /// Why the value cannot be represented.
        message: String,
    },
    /// Available source coordinates violate the ASG location contract.
    #[error("invalid ASG location at {path}: {message}")]
    InvalidLocation {
        /// JSON Pointer to the invalid boundary.
        path: String,
        /// Native owner of the invalid location, when known.
        construct: Option<&'static str>,
        /// Valid source coordinates, if the owner has any.
        location: Option<Box<Location>>,
        /// Which location requirement failed.
        message: String,
    },
    /// Projected output violates the selected schema.
    #[error("invalid ASG projection at {path}: {message}")]
    Projection {
        /// JSON Pointer into the projected output.
        path: String,
        /// The schema validation failure.
        message: String,
    },
}

/// Export a document without changing its native AST or derived catalogs.
///
/// # Errors
/// Returns an error for any unsupported nested value or invalid location. No
/// partial output is returned. Success preserves the documented supported
/// semantics and validates against [`SCHEMA_REVISION`]; it does not establish
/// source round-trip fidelity or complete language conformance.
pub fn document(document: &Document<'_>) -> Result<Value, Error> {
    document_with_profile(document, Profile::Upstream)
}

/// Export using an explicit schema profile.
///
/// The acdc profile adds a required `profile` identifier and supported content
/// properties. It does not claim upstream schema conformance. Both profiles
/// read resolved values directly and leave the native AST unchanged.
///
/// # Errors
/// Returns an error for unsupported nested values, invalid locations, or output
/// that does not validate against the selected schema. No partial output is returned.
pub fn document_with_profile(document: &Document<'_>, profile: Profile) -> Result<Value, Error> {
    let value = projection::document(document, profile)
        .map_err(|error| error.with_context("document", &document.location))?;
    validate(&value, "", profile)?;
    Ok(value)
}

/// Export resolved inline content as an array, using the document export rules.
///
/// # Errors
/// Returns an error for unsupported content or locations, including unresolved
/// passthrough operations and unsupported identity or controls on inline spans.
pub fn inlines(inlines: &[InlineNode<'_>]) -> Result<Value, Error> {
    inlines_with_profile(inlines, Profile::Upstream)
}

/// Export resolved inline content under the selected schema profile.
///
/// # Errors
/// Returns an error for unsupported nested values or invalid locations. The
/// returned array validates as inline content under the selected profile.
pub fn inlines_with_profile(inlines: &[InlineNode<'_>], profile: Profile) -> Result<Value, Error> {
    let value = projection::inlines(inlines, "", profile)?;
    let paragraph = Map::from_iter([
        ("name".into(), "paragraph".into()),
        ("type".into(), "block".into()),
        ("inlines".into(), value),
    ]);
    let mut wrapper = Map::from_iter([
        ("name".into(), "document".into()),
        ("type".into(), "block".into()),
        (
            "blocks".into(),
            Value::Array(vec![Value::Object(paragraph)]),
        ),
    ]);
    if profile == Profile::Acdc {
        wrapper.insert("profile".into(), ACDC_SCHEMA_ID.into());
    }
    let mut wrapper = Value::Object(wrapper);
    validate(&wrapper, "/blocks/0/inlines", profile)?;
    wrapper
        .pointer_mut("/blocks/0/inlines")
        .map(Value::take)
        .ok_or_else(|| Error::Projection {
            path: String::new(),
            message: "validated inline wrapper has no content".into(),
        })
}

fn unsupported(path: &str, message: impl Into<String>) -> Error {
    Error::Unsupported {
        path: path.into(),
        construct: None,
        location: None,
        message: message.into(),
    }
}

fn target(source: &Source<'_>, path: &str) -> Result<String, Error> {
    match source {
        Source::Path(value) => value.to_str().map(str::to_owned).ok_or_else(|| {
            unsupported(
                &pointer(path, "target"),
                "a non-UTF-8 target cannot be represented as JSON text",
            )
        }),
        Source::Url(value) => Ok(value.to_string()),
        Source::Name(value) => Ok((*value).into()),
    }
}

fn pointer(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn validate(value: &Value, prefix: &str, profile: Profile) -> Result<(), Error> {
    let (cache, bytes): (_, &[u8]) = match profile {
        Profile::Upstream => (&VALIDATOR, include_bytes!("../../schemas/asg/schema.json")),
        Profile::Acdc => (
            &ACDC_VALIDATOR,
            include_bytes!("../../schemas/asg/acdc-v1.json"),
        ),
    };
    let validator = cache
        .get_or_init(|| {
            let schema = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
            jsonschema::validator_for(&schema).map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|message| Error::Schema(message.clone()))?;
    validator.validate(value).map_err(|error| {
        let path = error.instance_path().to_string();
        Error::Projection {
            path: path.strip_prefix(prefix).unwrap_or(&path).into(),
            message: error.to_string(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{Error, Profile, validate};

    #[test]
    fn invalid_projection_is_not_reported_as_unsupported_source() {
        let value = serde_json::json!({"name":"document", "type":"block", "blocks":[{
            "name":"paragraph", "type":"block", "inlines":"wrong shape"
        }]});
        assert!(
            matches!(validate(&value, "", Profile::Upstream), Err(Error::Projection { path, .. }) if path == "/blocks/0")
        );
    }
}
