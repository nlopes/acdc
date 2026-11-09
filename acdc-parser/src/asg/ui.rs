use serde_json::{Map, Value};

use crate::{ContentAttribute, InlineMacro};

use super::{
    Error, pointer,
    projection::{InlineContext, project_inlines},
    unsupported,
};

pub(super) fn project(
    value: &InlineMacro<'_>,
    path: &str,
    settings: InlineContext,
) -> Result<Map<String, Value>, Error> {
    let (name, field, content) = match value {
        InlineMacro::Button(value) => (
            "button",
            "inlines",
            project_inlines(
                value.label.as_inlines(),
                &pointer(path, "inlines"),
                settings,
            )?,
        ),
        InlineMacro::KeyboardShortcut(value) => (
            "keyboard",
            "keys",
            labels(&value.keys, &pointer(path, "keys"), settings)?,
        ),
        InlineMacro::MenuPath(value) => {
            return Ok(Map::from_iter([
                ("name".into(), "menu".into()),
                ("type".into(), "inline".into()),
                (
                    "target".into(),
                    project_inlines(
                        value.target.as_inlines(),
                        &pointer(path, "target"),
                        settings,
                    )?,
                ),
                (
                    "items".into(),
                    labels(&value.items, &pointer(path, "items"), settings)?,
                ),
            ]));
        }
        InlineMacro::Link(_)
        | InlineMacro::UrlMacro(_)
        | InlineMacro::Mailto(_)
        | InlineMacro::Autolink(_)
        | InlineMacro::CrossReference(_)
        | InlineMacro::Footnote(_)
        | InlineMacro::Icon(_)
        | InlineMacro::Image(_)
        | InlineMacro::Passthrough(_)
        | InlineMacro::Stem(_)
        | InlineMacro::IndexTerm(_) => {
            return Err(unsupported(path, "expected a supported interface macro"));
        }
    };
    Ok(Map::from_iter([
        ("name".into(), name.into()),
        ("type".into(), "inline".into()),
        (field.into(), content),
    ]))
}

fn labels(
    values: &[ContentAttribute<'_>],
    path: &str,
    settings: InlineContext,
) -> Result<Value, Error> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            project_inlines(
                value.as_inlines(),
                &pointer(path, &index.to_string()),
                settings,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}
