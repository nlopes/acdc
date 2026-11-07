use serde_json::{Map, Value};

use crate::{DocumentAttribute, DocumentAttributeAssignment, DocumentAttributeValue};

use super::{Error, Profile, pointer, unsupported};

pub(super) fn value(attribute: &DocumentAttributeValue<'_>, path: &str) -> Result<Value, Error> {
    if !attribute.inline_fragments().is_empty() || !attribute.passthrough_ranges().is_empty() {
        return Err(unsupported(
            path,
            "attribute content protection cannot be flattened into a scalar string",
        ));
    }
    let mut text = String::new();
    attribute
        .write_text(&mut text)
        .map_err(|error| unsupported(path, error.to_string()))?;
    Ok(Value::String(text))
}

pub(super) fn block(
    attribute: &DocumentAttribute<'_>,
    path: &str,
    profile: Profile,
) -> Result<Map<String, Value>, Error> {
    if profile == Profile::Upstream {
        return Err(unsupported(
            path,
            "ordered body assignments have no supported upstream mapping",
        ));
    }
    let at = pointer(path, "assignment");
    if !attribute.is_accepted() {
        return Err(unsupported(
            &at,
            "a rejected parser assignment is not an accepted attribute event",
        ));
    }
    let assignment = assignment(attribute.assignment(), &at)?;
    Ok(Map::from_iter([
        ("name".into(), "attribute".into()),
        ("type".into(), "block".into()),
        ("attribute".into(), attribute.name.as_ref().into()),
        ("assignment".into(), assignment),
    ]))
}

pub(super) fn assignment(
    value: &DocumentAttributeAssignment<'_>,
    path: &str,
) -> Result<Value, Error> {
    Ok(match value {
        DocumentAttributeAssignment::Set(attribute) => {
            serde_json::json!({"kind":"set", "value":self::value(attribute, &pointer(path, "value"))?})
        }
        DocumentAttributeAssignment::Unset => serde_json::json!({"kind":"unset"}),
    })
}

#[cfg(test)]
mod tests {
    use super::{Error, Profile, block};
    use crate::{DocumentAttribute, Location};

    #[test]
    fn rejected_assignments_do_not_become_unsets() {
        let rejected = DocumentAttribute::rejected("feature".into(), Location::default());
        assert!(
            matches!(block(&rejected, "/blocks/0", Profile::Acdc), Err(Error::Unsupported {path, ..}) if path == "/blocks/0/assignment")
        );
    }
}
