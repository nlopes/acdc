use serde_json::{Map, Value};

use crate::{Image, InlineImage};

use super::{Error, Profile, metadata, pointer, projection::inlines, target, unsupported};

pub(super) fn inline(
    value: &InlineImage<'_>,
    path: &str,
    profile: Profile,
) -> Result<Map<String, Value>, Error> {
    let mut result = Map::from_iter([
        ("name".into(), "image".into()),
        ("type".into(), "inline".into()),
        ("target".into(), target(&value.source, path)?.into()),
    ]);
    for (name, content) in [
        ("alt", value.alt.as_ref()),
        ("tooltip", value.tooltip.as_ref()),
    ] {
        if let Some(content) = content {
            result.insert(
                name.into(),
                inlines(content, &pointer(path, name), profile)?,
            );
        }
    }
    if let Some(id) = &value.metadata.id {
        result.insert("id".into(), id.id.into());
    }
    let mut metadata = Map::new();
    let attributes = metadata::scalar_attributes(
        value.attributes(),
        &pointer(&pointer(path, "metadata"), "attributes"),
    )?;
    if !attributes.is_empty() {
        metadata.insert("attributes".into(), Value::Object(attributes));
    }
    for (name, values) in [
        ("roles", value.metadata.roles.as_slice()),
        ("options", value.options()),
    ] {
        if !values.is_empty() {
            metadata.insert(
                name.into(),
                Value::Array(
                    values
                        .iter()
                        .map(|value| Value::String((*value).into()))
                        .collect(),
                ),
            );
        }
    }
    if !metadata.is_empty() {
        result.insert("metadata".into(), Value::Object(metadata));
    }
    Ok(result)
}

pub(super) fn block(
    result: &mut Map<String, Value>,
    value: &Image<'_>,
    path: &str,
    profile: Profile,
) -> Result<(), Error> {
    value
        .metadata
        .validate_attributes(&["title", "alt"])
        .map_err(|error| {
            unsupported(
                &pointer(
                    &pointer(&pointer(path, "metadata"), "attributes"),
                    &error.name,
                ),
                error.to_string(),
            )
        })?;
    if let Some(content) = &value.alt {
        result.insert(
            "alt".into(),
            inlines(content, &pointer(path, "alt"), profile)?,
        );
    }
    Ok(())
}
