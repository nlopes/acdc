use crate::{ElementId, Form, InlineNode};

use super::{Error, InlineContext, Node, Profile, node, pointer, project_inlines, unsupported};

pub(super) fn project(
    value: &InlineNode<'_>,
    path: &str,
    settings: InlineContext,
) -> Result<Node, Error> {
    match value {
        InlineNode::BoldText(span) => span_node(
            "strong",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        ),
        InlineNode::ItalicText(span) => span_node(
            "emphasis",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        ),
        InlineNode::MonospaceText(span) => span_node(
            "code",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        ),
        InlineNode::HighlightText(span) => span_node(
            "mark",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        ),
        InlineNode::SubscriptText(span) => span_node(
            "subscript",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        ),
        InlineNode::SuperscriptText(span) => span_node(
            "superscript",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        ),
        InlineNode::CurvedQuotationText(span) => span_node(
            "quotation",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        )
        .map(|result| quotation(result, ["“", "”"])),
        InlineNode::CurvedApostropheText(span) => span_node(
            "quotation",
            &span.form,
            span.role,
            span.id.as_ref(),
            &span.content,
            path,
            settings,
        )
        .map(|result| quotation(result, ["‘", "’"])),
        InlineNode::PlainText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::Symbol(_)
        | InlineNode::CharacterReference(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::RawText(_)
        | InlineNode::Macro(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::CalloutRef(_) => Err(unsupported(path, "expected a supported inline span")),
    }
}

fn quotation(mut result: Node, quotes: [&str; 2]) -> Node {
    result.insert("quotes".into(), serde_json::json!(quotes));
    result
}

fn span_node(
    name: &str,
    form: &Form,
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
            Form::Constrained => "constrained",
            Form::Unconstrained => "unconstrained",
        }
        .into(),
    );
    result.insert(
        "inlines".into(),
        project_inlines(content, &pointer(path, "inlines"), settings)?,
    );
    Ok(result)
}
