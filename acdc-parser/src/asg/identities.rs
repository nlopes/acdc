use serde_json::{Map, Value};

use crate::Anchor;

use super::{Error, Profile, metadata, pointer, projection::inlines, unsupported};

fn declaration(
    anchor: &Anchor<'_>,
    path: &str,
    profile: Profile,
) -> Result<Map<String, Value>, Error> {
    check(anchor, path)?;
    let mut result = Map::from_iter([("id".into(), anchor.id.into())]);
    if let Some(label) = &anchor.xreflabel {
        result.insert(
            "reftext".into(),
            inlines(label.as_inlines(), &pointer(path, "reftext"), profile)?,
        );
    }
    Ok(result)
}

fn check(anchor: &Anchor<'_>, path: &str) -> Result<(), Error> {
    if anchor.reftext_source.is_some() || anchor.is_bibliography() {
        return Err(unsupported(
            path,
            "pending or bibliography anchors need a complete semantic mapping",
        )
        .with_context("anchor", &anchor.location));
    }
    Ok(())
}

pub(super) fn inline(anchor: &Anchor<'_>, path: &str) -> Result<Map<String, Value>, Error> {
    let mut result = declaration(anchor, path, Profile::Acdc)?;
    result.insert("name".into(), "anchor".into());
    result.insert("type".into(), "inline".into());
    Ok(result)
}

pub(super) fn apply<'a, 'doc>(
    result: &mut Map<String, Value>,
    primary: Option<&'a Anchor<'doc>>,
    declarations: impl Iterator<Item = &'a Anchor<'doc>>,
    profile: Profile,
    path: &str,
) -> Result<(), Error> {
    if let Some(primary) = primary {
        result.insert("id".into(), primary.id.into());
        if profile == Profile::Acdc {
            check(primary, path)?;
            metadata::location_property(result, "id_location", &primary.location, path)?;
        }
    }
    if profile == Profile::Upstream {
        return Ok(());
    }
    let mut anchors = Vec::new();
    for anchor in declarations {
        if primary.is_some_and(|primary| std::ptr::eq(primary, anchor)) {
            continue;
        }
        let at = pointer(&pointer(path, "anchors"), &anchors.len().to_string());
        let mut value = declaration(anchor, &at, profile)?;
        metadata::location(&mut value, &anchor.location, &at)?;
        anchors.push(Value::Object(value));
    }
    if !anchors.is_empty() {
        result.insert("anchors".into(), Value::Array(anchors));
    }
    Ok(())
}
