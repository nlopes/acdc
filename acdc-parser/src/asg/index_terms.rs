use serde_json::{Map, Value};

use crate::{IndexTerm, IndexTermKind, IndexTermRelationship};

use super::{
    Error, metadata, pointer,
    projection::{InlineContext, project_inlines},
    unsupported,
};

pub(super) fn project(
    value: &IndexTerm<'_>,
    path: &str,
    settings: InlineContext,
) -> Result<Map<String, Value>, Error> {
    let mut result = labels(value, path, settings)?;
    result.insert("name".into(), "indexterm".into());
    result.insert("type".into(), "inline".into());
    metadata::apply_inline(
        &mut result,
        &value.metadata,
        value.attributes(),
        value.options(),
        path,
    )?;
    let captured = value.catalog_entry();
    if captured.kind != value.kind
        || captured.relationship != value.relationship
        || captured.location != value.location
    {
        let at = pointer(path, "catalog");
        let mut catalog = labels(captured, &at, InlineContext::normal(settings.profile))?;
        metadata::location(&mut catalog, &captured.location, &at)?;
        result.insert("catalog".into(), Value::Object(catalog));
    }
    Ok(result)
}

fn labels(
    value: &IndexTerm<'_>,
    path: &str,
    settings: InlineContext,
) -> Result<Map<String, Value>, Error> {
    if value.catalog_substitutions.is_some() {
        return Err(unsupported(
            &pointer(path, "catalog_substitutions"),
            "index registration substitutions are not resolved",
        ));
    }
    let mut result = Map::new();
    result.insert(
        "visibility".into(),
        match value.kind {
            IndexTermKind::Flow(_) => "visible",
            IndexTermKind::Concealed { .. } => "hidden",
        }
        .into(),
    );
    for (name, content) in [
        ("term", Some(value.term())),
        ("secondary", value.secondary()),
        ("tertiary", value.tertiary()),
    ] {
        if let Some(content) = content {
            result.insert(
                name.into(),
                project_inlines(content, &pointer(path, name), settings)?,
            );
        }
    }
    if let Some(relationship) = &value.relationship {
        let at = pointer(path, "relationship");
        let mut relation = Map::new();
        match relationship {
            IndexTermRelationship::See { target } => {
                relation.insert("kind".into(), "see".into());
                relation.insert(
                    "target".into(),
                    project_inlines(target, &pointer(&at, "target"), settings)?,
                );
            }
            IndexTermRelationship::SeeAlso { targets } => {
                relation.insert("kind".into(), "see-also".into());
                let targets = targets
                    .iter()
                    .enumerate()
                    .map(|(index, target)| {
                        project_inlines(
                            target,
                            &pointer(&pointer(&at, "targets"), &index.to_string()),
                            settings,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                relation.insert("targets".into(), Value::Array(targets));
            }
        }
        result.insert("relationship".into(), Value::Object(relation));
    }
    Ok(result)
}
