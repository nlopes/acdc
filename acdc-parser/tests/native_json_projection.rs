use acdc_parser::{CompatibilityMode, InlineMacro, InlineNode, Options, parse_inline};
use serde_json::Value;

type TestError = Box<dyn std::error::Error>;

fn owner_json(owner: &InlineMacro<'_>) -> Result<Value, TestError> {
    Ok(match owner {
        InlineMacro::Footnote(value) => serde_json::to_value(value)?,
        InlineMacro::Icon(value) => serde_json::to_value(value)?,
        InlineMacro::Image(value) => serde_json::to_value(value)?,
        InlineMacro::Keyboard(value) => serde_json::to_value(value)?,
        InlineMacro::Button(value) => serde_json::to_value(value)?,
        InlineMacro::Menu(value) => serde_json::to_value(value)?,
        InlineMacro::Url(value) => serde_json::to_value(value)?,
        InlineMacro::Link(value) => serde_json::to_value(value)?,
        InlineMacro::Mailto(value) => serde_json::to_value(value)?,
        InlineMacro::Autolink(value) => serde_json::to_value(value)?,
        InlineMacro::CrossReference(value) => serde_json::to_value(value)?,
        InlineMacro::Pass(value) => serde_json::to_value(value)?,
        InlineMacro::Stem(value) => serde_json::to_value(value)?,
        InlineMacro::IndexTerm(value) => serde_json::to_value(value)?,
        _ => return Err("uncovered macro family".into()),
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
    let [node] = parsed.inlines() else {
        return Err(format!("expected one macro for {source}: {:?}", parsed.inlines()).into());
    };
    let InlineNode::Macro(owner) = node else {
        return Err(format!("expected macro for {source}: {node:?}").into());
    };
    let expected = serde_json::to_value(node)?;
    assert_eq!(serde_json::to_value(owner)?, expected, "{source}");
    assert_eq!(owner_json(owner)?, expected, "{source}");
    if let InlineMacro::Autolink(autolink) = owner {
        assert_eq!(
            expected.get("bracketed").and_then(Value::as_bool),
            autolink.bracketed.then_some(true)
        );
    }
    if let InlineMacro::Pass(pass) = owner {
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
    let line_break = acdc_parser::LineBreak::new(acdc_parser::Location::default());
    let node = InlineNode::LineBreak(line_break.clone());
    assert_eq!(
        serde_json::to_value(&line_break)?,
        serde_json::to_value(&node)?
    );
    Ok(())
}
