use serde_json::{Map, Value};

use crate::{Anchor, AttributeValue, BlockMetadata, ContentAttribute, Location, Title};

use super::{Error, pointer, projection::inlines, unsupported};

pub(super) fn location(
    node: &mut Map<String, Value>,
    value: &Location,
    path: &str,
) -> Result<(), Error> {
    if value == &Location::default() {
        return Ok(());
    }
    let path = pointer(path, "location");
    let mut boundaries = Vec::with_capacity(2);
    for (index, position) in [&value.start, &value.end].into_iter().enumerate() {
        if position.line == 0 || position.file.as_ref().is_some_and(|chain| chain.is_empty()) {
            return Err(Error::InvalidLocation { path: pointer(&path, &index.to_string()),
                construct: None, location: None,
                message: "a known boundary needs a positive line and a nonempty include chain when supplied".into() });
        }
        let mut boundary = Map::new();
        boundary.insert("line".into(), position.line.into());
        boundary.insert("col".into(), position.column.into());
        if let Some(chain) = &position.file {
            boundary.insert(
                "file".into(),
                Value::Array(
                    chain
                        .iter()
                        .map(|part| Value::String(part.clone()))
                        .collect(),
                ),
            );
        }
        boundaries.push(Value::Object(boundary));
    }
    node.insert("location".into(), Value::Array(boundaries));
    Ok(())
}

pub(super) fn label<'a, 'doc>(
    value: &'a BlockMetadata<'doc>,
) -> Option<&'a ContentAttribute<'doc>> {
    value.reftext.as_ref().or_else(|| {
        value
            .anchors
            .last()
            .and_then(|anchor| anchor.xreflabel.as_ref())
    })
}

pub(super) fn anchor<'a, 'doc>(value: &'a BlockMetadata<'doc>) -> Option<&'a Anchor<'doc>> {
    value.id.as_ref().or_else(|| value.anchors.first())
}

pub(super) fn title(
    node: &mut Map<String, Value>,
    value: &Title<'_>,
    path: &str,
) -> Result<(), Error> {
    if value.is_present() {
        node.insert(
            "title".into(),
            inlines(value.as_ref(), &pointer(path, "title"))?,
        );
    }
    Ok(())
}

pub(super) fn check(value: &BlockMetadata<'_>, path: &str) -> Result<(), Error> {
    let at = pointer(path, "metadata");
    value.validate_attributes(&[]).map_err(|error| {
        unsupported(
            &pointer(&pointer(&at, "attributes"), &error.name),
            error.to_string(),
        )
    })?;
    for (present, name) in [
        (value.has_document_attributes(), "document_attributes"),
        (value.caption.is_some(), "caption"),
        (value.attribution.is_some(), "attribution"),
        (value.citetitle.is_some(), "citetitle"),
        (value.substitutions.is_some(), "substitutions"),
    ] {
        if present {
            return Err(unsupported(
                &pointer(&at, name),
                "this semantic property has no supported upstream mapping",
            ));
        }
    }
    if usize::from(value.id.is_some()) + value.anchors.len() > 1 {
        return Err(unsupported(
            &pointer(&at, "anchors"),
            "multiple block identities need a lossless mapping",
        ));
    }
    if let Some(anchor) = anchor(value) {
        if anchor.is_bibliography() {
            return Err(unsupported(
                &pointer(&at, "id"),
                "bibliography identity has no supported upstream mapping",
            ));
        }
        if let (Some(selected), Some(other)) = (&value.reftext, &anchor.xreflabel)
            && selected != other
        {
            return Err(unsupported(
                &pointer(&at, "reftext"),
                "independent reference labels cannot share one upstream field",
            ));
        }
    }
    if value.alt_source.is_some() || value.first_source.is_some() || value.reftext_source.is_some()
    {
        return Err(unsupported(
            &at,
            "pending content must be resolved before export",
        ));
    }
    Ok(())
}

pub(super) fn apply(
    node: &mut Map<String, Value>,
    value: &BlockMetadata<'_>,
    path: &str,
) -> Result<(), Error> {
    check(value, path)?;
    if let Some(anchor) = anchor(value) {
        node.insert("id".into(), anchor.id.into());
    }
    if let Some(label) = label(value) {
        node.insert(
            "reftext".into(),
            inlines(label.as_ref(), &pointer(path, "reftext"))?,
        );
    }
    let mut metadata = Map::new();
    let mut attributes = Map::new();
    let at = pointer(&pointer(path, "metadata"), "attributes");
    for (name, attribute) in value.attributes.iter() {
        let text = match attribute {
            AttributeValue::String(text) => text.as_ref(),
            AttributeValue::Bool(true) => "",
            AttributeValue::None => {
                return Err(unsupported(
                    &pointer(&at, name),
                    "an element value without a defined scalar meaning cannot be encoded as an empty string",
                ));
            }
            AttributeValue::Bool(false) => {
                return Err(unsupported(
                    &pointer(&at, name),
                    "an unset element attribute cannot be encoded as a scalar string",
                ));
            }
        };
        attributes.insert(name.to_string(), text.into());
    }
    if let Some(style) = value.style {
        attributes.insert("style".into(), style.into());
    }
    for (index, slot) in value.raw_positional_attributes().iter().enumerate() {
        if !slot.value.is_empty() || slot.quoted_empty {
            let key = format!("${}", index + 1);
            if attributes.insert(key.clone(), slot.value.into()).is_some() {
                return Err(unsupported(
                    &pointer(&at, &key),
                    "a named attribute conflicts with a retained positional slot",
                ));
            }
        }
    }
    if !attributes.is_empty() {
        metadata.insert("attributes".into(), Value::Object(attributes));
    }
    if !value.roles.is_empty() {
        metadata.insert(
            "roles".into(),
            Value::Array(
                value
                    .roles
                    .iter()
                    .map(|role| Value::String((*role).into()))
                    .collect(),
            ),
        );
    }
    if !value.options.is_empty() {
        metadata.insert(
            "options".into(),
            Value::Array(
                value
                    .options
                    .iter()
                    .map(|option| Value::String((*option).into()))
                    .collect(),
            ),
        );
    }
    if let Some(source) = &value.location {
        location(&mut metadata, source, &pointer(path, "metadata"))?;
    }
    if !metadata.is_empty() {
        node.insert("metadata".into(), Value::Object(metadata));
    }
    Ok(())
}

pub(super) fn header(value: &BlockMetadata<'_>) -> Result<(), Error> {
    check(value, "/header")?;
    if !value.attributes.is_empty()
        || !value.raw_positional_attributes().is_empty()
        || !value.roles.is_empty()
        || !value.options.is_empty()
        || value.style.is_some()
        || value.id.is_some()
        || !value.anchors.is_empty()
        || value.reftext.is_some()
    {
        return Err(unsupported(
            "/header/metadata",
            "the upstream header has no identity or metadata fields",
        ));
    }
    Ok(())
}
