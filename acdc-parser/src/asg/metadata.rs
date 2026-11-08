use serde_json::{Map, Value};

use crate::{
    Anchor, AttributeValue, BlockMetadata, Comment, ContentAttribute, ElementAttributes,
    InlineMetadata, Location, Title,
};

use super::{Error, Profile, content, pointer, projection::inlines, unsupported};

pub(super) fn location(
    node: &mut Map<String, Value>,
    value: &Location,
    path: &str,
) -> Result<(), Error> {
    location_property(node, "location", value, path)
}

pub(super) fn location_property(
    node: &mut Map<String, Value>,
    property: &str,
    value: &Location,
    path: &str,
) -> Result<(), Error> {
    if value.is_unknown() {
        return Ok(());
    }
    let path = pointer(path, property);
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
    node.insert(property.into(), Value::Array(boundaries));
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
    value.id_anchor()
}

pub(super) fn title(
    node: &mut Map<String, Value>,
    value: &Title<'_>,
    path: &str,
    profile: Profile,
) -> Result<(), Error> {
    if value.is_present() {
        node.insert(
            "title".into(),
            inlines(value.as_inlines(), &pointer(path, "title"), profile)?,
        );
    }
    Ok(())
}

pub(super) fn check_substitutions(value: &BlockMetadata<'_>, path: &str) -> Result<(), Error> {
    if let Some(entry) = value
        .substitutions
        .as_ref()
        .and_then(|spec| spec.unsupported_entries().next())
    {
        return Err(unsupported(
            &pointer(&pointer(path, "metadata"), "substitutions"),
            format!("unsupported substitution entry {entry:?} has no resolved graph meaning"),
        ));
    }
    Ok(())
}

pub(super) fn check(value: &BlockMetadata<'_>, path: &str, profile: Profile) -> Result<(), Error> {
    let at = pointer(path, "metadata");
    check_substitutions(value, path)?;
    value.validate_attributes(&[]).map_err(|error| {
        unsupported(
            &pointer(&pointer(&at, "attributes"), &error.name),
            error.to_string(),
        )
    })?;
    for (present, name) in [
        (value.has_prefix_events(), "prefix_events"),
        (
            profile == Profile::Upstream && value.caption.is_some(),
            "caption",
        ),
        (
            profile == Profile::Upstream && value.attribution.is_some(),
            "attribution",
        ),
        (
            profile == Profile::Upstream && value.citetitle.is_some(),
            "citetitle",
        ),
    ] {
        if present {
            return Err(unsupported(
                &pointer(&at, name),
                "this semantic property has no supported upstream mapping",
            ));
        }
    }
    if profile == Profile::Upstream && usize::from(value.id.is_some()) + value.anchors.len() > 1 {
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
        if profile == Profile::Upstream
            && let (Some(selected), Some(other)) = (&value.reftext, &anchor.xreflabel)
            && selected != other
        {
            return Err(unsupported(
                &pointer(&at, "reftext"),
                "independent reference labels cannot share one upstream field",
            ));
        }
    }
    for (present, property) in [
        (value.alt_source.is_some(), "alt_source"),
        (value.first_source.is_some(), "first_source"),
        (value.reftext_source.is_some(), "reftext_source"),
    ] {
        if present {
            return Err(unsupported(
                &pointer(&at, property),
                "pending content must be resolved before export",
            ));
        }
    }
    Ok(())
}

pub(super) fn apply(
    profile: Profile,
    node: &mut Map<String, Value>,
    value: &BlockMetadata<'_>,
    path: &str,
) -> Result<(), Error> {
    apply_with_primary(profile, node, value, anchor(value), path)
}

pub(super) fn apply_heading(
    profile: Profile,
    node: &mut Map<String, Value>,
    value: &BlockMetadata<'_>,
    path: &str,
) -> Result<(), Error> {
    apply_with_primary(profile, node, value, value.heading_anchor(), path)
}

fn apply_with_primary(
    profile: Profile,
    node: &mut Map<String, Value>,
    value: &BlockMetadata<'_>,
    primary: Option<&Anchor<'_>>,
    path: &str,
) -> Result<(), Error> {
    check(value, path, profile)?;
    content::apply(node, value, path, profile)?;
    super::identities::apply(
        node,
        primary,
        value.id.iter().chain(value.anchors.iter()),
        profile,
        path,
    )?;
    if let Some(label) = label(value) {
        node.insert(
            "reftext".into(),
            inlines(label.as_inlines(), &pointer(path, "reftext"), profile)?,
        );
    }
    let mut metadata = Map::new();
    let at = pointer(&pointer(path, "metadata"), "attributes");
    let mut attributes = scalar_attributes(&value.attributes, &at)?;
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
    if !value.roles.is_absent() {
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
    if !value.options.is_absent() {
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

pub(super) fn apply_inline(
    node: &mut Map<String, Value>,
    value: &InlineMetadata<'_>,
    attributes: &ElementAttributes<'_>,
    options: &crate::TokenAssignment<'_>,
    path: &str,
) -> Result<(), Error> {
    if let Some(id) = &value.id {
        node.insert("id".into(), id.id.into());
    }
    let mut metadata = Map::new();
    let attributes = scalar_attributes(
        attributes,
        &pointer(&pointer(path, "metadata"), "attributes"),
    )?;
    if !attributes.is_empty() {
        metadata.insert("attributes".into(), Value::Object(attributes));
    }
    for (name, values) in [("roles", &value.roles), ("options", options)] {
        if !values.is_absent() {
            metadata.insert(name.into(), serde_json::json!(values));
        }
    }
    if !metadata.is_empty() {
        node.insert("metadata".into(), Value::Object(metadata));
    }
    Ok(())
}

fn scalar_attributes(
    value: &ElementAttributes<'_>,
    path: &str,
) -> Result<Map<String, Value>, Error> {
    let mut attributes = Map::new();
    for (name, attribute) in value.iter() {
        let text = match attribute {
            AttributeValue::String(text) => text.as_ref(),
            AttributeValue::Bool(true) => "",
            AttributeValue::None => {
                return Err(unsupported(
                    &pointer(path, name),
                    "an element value without a defined scalar meaning cannot be encoded as an empty string",
                ));
            }
            AttributeValue::Bool(false) => {
                return Err(unsupported(
                    &pointer(path, name),
                    "an unset element attribute cannot be encoded as a scalar string",
                ));
            }
        };
        attributes.insert(name.to_string(), text.into());
    }
    Ok(attributes)
}

pub(super) fn header(value: &BlockMetadata<'_>) -> Result<(), Error> {
    empty(value, "/header")
}

pub(super) fn empty(value: &BlockMetadata<'_>, path: &str) -> Result<(), Error> {
    check(value, path, Profile::Upstream)?;
    for (present, property) in [
        (!value.attributes.is_empty(), "attributes"),
        (
            !value.raw_positional_attributes().is_empty(),
            "positional_attributes",
        ),
        (!value.roles.is_absent(), "roles"),
        (!value.options.is_absent(), "options"),
        (value.style.is_some(), "style"),
        (value.id.is_some(), "id"),
        (!value.anchors.is_empty(), "anchors"),
        (value.reftext.is_some(), "reftext"),
    ] {
        if present {
            return Err(unsupported(
                &pointer(&pointer(path, "metadata"), property),
                "this owner has no supported upstream metadata mapping",
            ));
        }
    }
    Ok(())
}

pub(super) fn check_comment(value: &Comment<'_>, path: &str) -> Result<(), Error> {
    if value.prefix_events.is_some() {
        return Err(unsupported(
            &pointer(path, "prefix_events"),
            "ordered source events must be emitted before a comment can be omitted",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Error, Profile, check};
    use crate::{
        BlockMetadata, DocumentAttribute, DocumentAttributeAssignment, Location,
        grammar::BlockAttributeMode, model::content_attribute::PendingContentAttribute,
    };

    #[test]
    fn pending_content_is_rejected_by_property() {
        let pending = || {
            Box::new(PendingContentAttribute {
                source: "content",
                input: 0,
                mode: BlockAttributeMode::Macro,
            })
        };
        for (property, metadata) in [
            (
                "alt_source",
                BlockMetadata {
                    alt_source: Some(pending()),
                    ..BlockMetadata::default()
                },
            ),
            (
                "first_source",
                BlockMetadata {
                    first_source: Some(pending()),
                    ..BlockMetadata::default()
                },
            ),
            (
                "reftext_source",
                BlockMetadata {
                    reftext_source: Some(0),
                    ..BlockMetadata::default()
                },
            ),
        ] {
            assert!(
                matches!(check(&metadata, "/blocks/0", Profile::Upstream), Err(Error::Unsupported {path, ..})
                if path == format!("/blocks/0/metadata/{property}"))
            );
        }
    }

    #[test]
    fn retained_document_events_are_not_omitted() {
        let mut metadata = BlockMetadata::default();
        metadata.push_prefix_event(crate::Block::DocumentAttribute(
            DocumentAttribute::accepted(
                "later".into(),
                DocumentAttributeAssignment::Set("value".into()),
                Location::default(),
            ),
        ));
        assert!(
            matches!(check(&metadata, "/blocks/0", Profile::Upstream), Err(Error::Unsupported {path, ..})
            if path == "/blocks/0/metadata/prefix_events")
        );
    }
}
