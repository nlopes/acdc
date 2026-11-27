use crate::{ElementId, FormattingForm, InlineNode};

use super::{Error, InlineContext, Node, Profile, node, pointer, project_inlines, unsupported};

pub(super) fn project(
    value: &InlineNode<'_>,
    path: &str,
    settings: InlineContext,
) -> Result<Node, Error> {
    match value {
        InlineNode::Bold(span) => span_node(
            "strong",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        ),
        InlineNode::Italic(span) => span_node(
            "emphasis",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        ),
        InlineNode::Monospace(span) => span_node(
            "code",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        ),
        InlineNode::Highlight(span) => span_node(
            "mark",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        ),
        InlineNode::Subscript(span) => span_node(
            "subscript",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        ),
        InlineNode::Superscript(span) => span_node(
            "superscript",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        ),
        InlineNode::DoubleQuotedSpan(span) => span_node(
            "quotation",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        )
        .map(|result| quotation(result, ["“", "”"])),
        InlineNode::SingleQuotedSpan(span) => span_node(
            "quotation",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        )
        .map(|result| quotation(result, ["‘", "’"])),
        InlineNode::Autolink(_)
        | InlineNode::Button(_)
        | InlineNode::CrossReference(_)
        | InlineNode::Footnote(_)
        | InlineNode::Icon(_)
        | InlineNode::Image(_)
        | InlineNode::IndexTerm(_)
        | InlineNode::KeyboardShortcut(_)
        | InlineNode::Link(_)
        | InlineNode::Mailto(_)
        | InlineNode::MenuPath(_)
        | InlineNode::Passthrough(_)
        | InlineNode::Stem(_)
        | InlineNode::UrlMacro(_)
        | InlineNode::Extension(_)
        | InlineNode::Plain(_)
        | InlineNode::Verbatim(_)
        | InlineNode::Symbol(_)
        | InlineNode::CharacterReference(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::Raw(_)
        | InlineNode::LineBreak(_)
        | InlineNode::Anchor(_)
        | InlineNode::Callout(_) => Err(unsupported(path, "expected a supported inline span")),
    }
}

fn quotation(mut result: Node, quotes: [&str; 2]) -> Node {
    result.insert("quotes".into(), serde_json::json!(quotes));
    result
}

fn span_node(
    name: &str,
    form: &FormattingForm,
    role: Option<&str>,
    id: Option<&ElementId<'_>>,
    content: &[InlineNode<'_>],
    path: &str,
    settings: InlineContext,
) -> Result<Node, Error> {
    for (present, property) in [(id.is_some(), "id"), (role.is_some(), "role")] {
        if settings.profile == Profile::Upstream && present {
            return Err(unsupported(
                &pointer(path, property),
                "upstream inline spans have no identity or role fields",
            ));
        }
    }
    let mut result = node("span", "inline");
    let variant = if name == "mark" && (id.is_some() || role.is_some()) {
        "generic"
    } else {
        name
    };
    result.insert("variant".into(), variant.into());
    if let Some(id) = id {
        result.insert("id".into(), id.id.into());
    }
    if let Some(role) = role {
        let roles: Vec<_> = role.split_whitespace().collect();
        result.insert("metadata".into(), serde_json::json!({"roles": roles}));
    }
    result.insert(
        "form".into(),
        match form {
            FormattingForm::Constrained => "constrained",
            FormattingForm::Unconstrained => "unconstrained",
        }
        .into(),
    );
    result.insert(
        "inlines".into(),
        project_inlines(content, &pointer(path, "inlines"), settings)?,
    );
    Ok(result)
}
