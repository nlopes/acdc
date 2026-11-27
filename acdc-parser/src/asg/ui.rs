use serde_json::{Map, Value};

use crate::{ContentAttribute, InlineNode};

use super::{
    Error, pointer,
    projection::{InlineContext, project_inlines},
    unsupported,
};

pub(super) fn project(
    value: &InlineNode<'_>,
    path: &str,
    settings: InlineContext,
) -> Result<Map<String, Value>, Error> {
    let (name, field, content) = match value {
        InlineNode::Button(value) => (
            "button",
            "inlines",
            project_inlines(
                value.label.as_inlines(),
                &pointer(path, "inlines"),
                settings,
            )?,
        ),
        InlineNode::KeyboardShortcut(value) => (
            "keyboard",
            "keys",
            labels(&value.keys, &pointer(path, "keys"), settings)?,
        ),
        InlineNode::MenuPath(value) => {
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
        InlineNode::Anchor(_)
        | InlineNode::Bold(_)
        | InlineNode::Callout(_)
        | InlineNode::CharacterReference(_)
        | InlineNode::DoubleQuotedSpan(_)
        | InlineNode::Extension(_)
        | InlineNode::Highlight(_)
        | InlineNode::Italic(_)
        | InlineNode::LineBreak(_)
        | InlineNode::Monospace(_)
        | InlineNode::Plain(_)
        | InlineNode::Raw(_)
        | InlineNode::SingleQuotedSpan(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::Subscript(_)
        | InlineNode::Superscript(_)
        | InlineNode::Symbol(_)
        | InlineNode::Verbatim(_)
        | InlineNode::Link(_)
        | InlineNode::UrlMacro(_)
        | InlineNode::Mailto(_)
        | InlineNode::Autolink(_)
        | InlineNode::CrossReference(_)
        | InlineNode::Footnote(_)
        | InlineNode::Icon(_)
        | InlineNode::Image(_)
        | InlineNode::Passthrough(_)
        | InlineNode::Stem(_)
        | InlineNode::IndexTerm(_) => {
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
