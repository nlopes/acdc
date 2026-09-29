use acdc_parser::{InlineMacro, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

// The ASG fixture format omits subject/body, so check these public values directly.
#[test]
fn mailto_query_arguments_keep_empty_slots_and_ignore_named_values() -> Result<(), Error> {
    for (arguments, subject, body) in [
        ("Label", None, None),
        ("Label,Subject", Some("Subject"), None),
        ("Label,,Body", Some(""), Some("Body")),
        ("Label,", Some(""), None),
        ("Label,Subject,Body,Ignored", Some("Subject"), Some("Body")),
        ("Label,role=green,Body", None, Some("Body")),
        ("Label,subject=Ignored,body=Ignored", None, None),
        ("Label,2=Ignored,3=Ignored", None, None),
        ("Label,+one,two+,{plus}", Some("one,two"), Some("+")),
        (r"Label,'It\'s fine',Body", Some("It's fine"), Some("Body")),
    ] {
        let input = format!("mailto:user@example.org[{arguments}]");
        let parsed = parse(&input, &Options::default())?;
        let [acdc_parser::Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected paragraph".into());
        };
        let [InlineNode::Macro(InlineMacro::Mailto(mailto))] = paragraph.content.as_slice() else {
            return Err(format!("expected mailto: {input}").into());
        };
        assert_eq!((mailto.subject, mailto.body), (subject, body), "{input}");
    }
    Ok(())
}

#[test]
#[cfg(feature = "pre-spec-subs")]
fn mailto_query_freezes_arguments_before_late_attributes() -> Result<(), Error> {
    let source = "= Mail\n:label: One, two\n:subject: Expanded\n\n[subs=\"macros,attributes\"]\nmailto:user@example.org[{label},{subject},Body]\n";
    let parsed = acdc_parser::parse(source, &Options::default())?;
    let [acdc_parser::Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected paragraph".into());
    };
    let [InlineNode::Macro(InlineMacro::Mailto(mailto))] = paragraph.content.as_slice() else {
        return Err("expected mailto".into());
    };
    assert_eq!(mailto.subject, Some("{subject}"));
    assert_eq!(mailto.body, Some("Body"));
    let [InlineNode::PlainText(label)] = mailto.text.as_slice() else {
        return Err("expected label".into());
    };
    assert_eq!(label.content, "One, two");
    assert_eq!(
        &source[label.location.absolute_start..=label.location.absolute_end],
        "{label}"
    );
    Ok(())
}
