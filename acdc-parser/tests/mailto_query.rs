use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

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
        let [BlockKind::Paragraph(paragraph)] = &*parsed
            .document_ref()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected paragraph".into());
        };
        let [InlineKind::Mailto(mailto)] = &*paragraph
            .content()
            .inlines()
            .map(InlineRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err(format!("expected mailto: {input}").into());
        };
        assert_eq!(
            (mailto.subject(), mailto.body()),
            (subject, body),
            "{input}"
        );
    }
    Ok(())
}

#[test]
fn mailto_query_freezes_arguments_before_late_attributes() -> Result<(), Error> {
    let source = "= Mail\n:label: One, two\n:subject: Expanded\n\n[subs=\"macros,attributes\"]\nmailto:user@example.org[{label},{subject},Body]\n";
    let parsed = acdc_parser::parse(source, &Options::default())?;
    let [BlockKind::Paragraph(paragraph)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected paragraph".into());
    };
    let [InlineKind::Mailto(mailto)] = &*paragraph
        .content()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected mailto".into());
    };
    assert_eq!(mailto.subject(), Some("{subject}"));
    assert_eq!(mailto.body(), Some("Body"));
    let [InlineKind::Plain(label)] = &*mailto
        .text()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected label".into());
    };
    assert_eq!(label.text(), "One, two");
    assert_eq!(
        &source[label.location().absolute_start()..=label.location().absolute_end()],
        "{label}"
    );
    Ok(())
}
