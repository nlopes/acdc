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
                inlines(content.as_inlines(), &pointer(path, name), profile)?,
            );
        }
    }
    metadata::apply_inline(
        &mut result,
        &value.metadata,
        value.attributes(),
        value.options(),
        path,
    )?;
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
            inlines(content.as_inlines(), &pointer(path, "alt"), profile)?,
        );
    }
    Ok(())
}
