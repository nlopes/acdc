use serde_json::{Map, Value};

use crate::Icon;

use super::{Error, Profile, metadata, pointer, projection::inlines, target};

pub(super) fn project(value: &Icon<'_>, path: &str) -> Result<Map<String, Value>, Error> {
    let mut result = Map::from_iter([
        ("name".into(), "icon".into()),
        ("type".into(), "inline".into()),
        ("target".into(), target(&value.target, path)?.into()),
    ]);
    for (name, content) in [
        ("alt", value.alt.as_ref()),
        ("tooltip", value.tooltip.as_ref()),
    ] {
        if let Some(content) = content {
            result.insert(
                name.into(),
                inlines(content.as_inlines(), &pointer(path, name), Profile::Acdc)?,
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
