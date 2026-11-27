use acdc_parser::{CompatibilityMode, Options, edit::InlineNode, parse_inline};
use serde_json::Value;

type TestError = Box<dyn std::error::Error>;

fn owner_json(owner: &InlineNode<'_>) -> Result<Value, TestError> {
    Ok(match owner {
        InlineNode::Footnote(value) => serde_json::to_value(value)?,
        InlineNode::Icon(value) => serde_json::to_value(value)?,
        InlineNode::Image(value) => serde_json::to_value(value)?,
        InlineNode::KeyboardShortcut(value) => serde_json::to_value(value)?,
        InlineNode::Button(value) => serde_json::to_value(value)?,
        InlineNode::MenuPath(value) => serde_json::to_value(value)?,
        InlineNode::UrlMacro(value) => serde_json::to_value(value)?,
        InlineNode::Link(value) => serde_json::to_value(value)?,
        InlineNode::Mailto(value) => serde_json::to_value(value)?,
        InlineNode::Autolink(value) => serde_json::to_value(value)?,
        InlineNode::CrossReference(value) => serde_json::to_value(value)?,
        InlineNode::Passthrough(value) => serde_json::to_value(value)?,
        InlineNode::Stem(value) => serde_json::to_value(value)?,
        InlineNode::IndexTerm(value) => serde_json::to_value(value)?,
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
        | _ => return Err("uncovered macro family".into()),
    })
}

#[rstest::rstest]
#[case("footnote:note[Body.]")]
#[case("icon:heart[alt=Alt,title=Tooltip,role=large]")]
#[case("image:photo.png[Alt,title=Tooltip,width=40,role=large]")]
#[case("kbd:[Ctrl+C]")]
#[case("btn:[Save]")]
#[case("menu:File[Open > Recent]")]
#[case("https://example.com[Label]")]
#[case("link:manual.html[Label,title=Tooltip]")]
#[case("mailto:team@example.com[Team,Subject,Body]")]
#[case("<https://example.com>")]
#[case("https://example.com")]
#[case("xref:chapter[Label]")]
#[case("stem:[x^2]")]
#[case("indexterm:[Primary,Secondary]")]
#[case(r"pass:m[pass:[<b>Raw</b>\]]")]
#[case(r"pass:m[pass:[\]]")]
#[case(r"pass:m[pass:q[*Formatted*\]]")]
#[case("mailto:team@example.com[Team,\"\",\"\"]")]
#[case("mailto:team@example.com[Team]")]
fn macros_have_one_projection_at_each_serialization_boundary(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] source: &str,
) -> Result<(), TestError> {
    let parsed = parse_inline(
        source,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_attribute("experimental", true)
            .build()?,
    )?;
    let parsed_inlines = acdc_parser::edit::ContentAttribute::from(parsed.content()).into_inlines();
    let [node] = parsed_inlines.as_slice() else {
        return Err(format!(
            "expected one macro for {source}: {:?}",
            parsed_inlines.as_slice()
        )
        .into());
    };
    let expected = serde_json::to_value(node)?;
    assert_eq!(owner_json(node)?, expected, "{source}");
    if let InlineNode::Autolink(autolink) = node {
        assert_eq!(
            expected.get("bracketed").and_then(Value::as_bool),
            acdc_parser::ast::AutolinkRef::from(autolink)
                .is_bracketed()
                .then_some(true)
        );
    }
    if let InlineNode::Passthrough(pass) = node {
        let mut copy = pass.clone();
        let text = copy.replace_text(None);
        let substitutions = copy.take_substitutions();
        assert_eq!(expected.get("text").and_then(Value::as_str), text);
        if !substitutions.is_empty() {
            assert_eq!(
                expected.get("substitutions"),
                Some(&serde_json::to_value(&substitutions)?)
            );
        }
    }
    Ok(())
}

#[test]
fn line_break_has_the_same_projection_as_its_inline_node() -> Result<(), TestError> {
    let line_break = acdc_parser::edit::LineBreak::new(acdc_parser::Location::default());
    let node = InlineNode::LineBreak(line_break.clone());
    assert_eq!(
        serde_json::to_value(&line_break)?,
        serde_json::to_value(&node)?
    );
    Ok(())
}
