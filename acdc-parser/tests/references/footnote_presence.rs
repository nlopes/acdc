use acdc_parser::{
    Document, Footnote, FootnoteDefinition, InlineNode, Location, Plain,
    ast::{ContentRef, DocumentRef, FootnoteDefinitionRef, FootnoteRef, InlineKind},
};

type Error = Box<dyn std::error::Error>;

#[test]
fn references_and_empty_definitions_have_distinct_content_and_json() -> Result<(), Error> {
    let reference = Footnote::reference(Some("key"), 7, Location::default());
    let empty = Footnote::new(Vec::new(), 7, Location::default());
    assert!(FootnoteRef::from(&reference).content().is_none());
    assert!(
        FootnoteRef::from(&empty)
            .content()
            .is_some_and(ContentRef::is_empty)
    );
    let reference_json = serde_json::to_value(reference)?;
    let empty_json = serde_json::to_value(empty)?;
    assert!(reference_json.get("inlines").is_none());
    assert_eq!(empty_json.get("inlines"), Some(&serde_json::json!([])));
    Ok(())
}

#[test]
fn body_transfers_preserve_presence_and_the_original_buffer() -> Result<(), Error> {
    let source = String::from("prepared *text*");
    let body = vec![InlineNode::PlainText(Plain::new(
        &source,
        Location::default(),
    ))];
    let pointer = body.as_ptr();
    let mut note = Footnote::reference(None, 1, Location::default());
    assert_eq!(note.replace_content(Some(body)), None);
    let retained = note.take_content().ok_or("definition body")?;
    assert_eq!(retained.as_ptr(), pointer);
    assert!(FootnoteRef::from(&note).content().is_none());
    note.replace_content(Some(Vec::new()));
    assert_eq!(note.take_content(), Some(Vec::new()));
    note.push_inline(InlineNode::PlainText(Plain::new(
        &source,
        Location::default(),
    )));
    let read = FootnoteRef::from(&note);
    let InlineKind::Plain(text) = read
        .content()
        .ok_or("promoted definition")?
        .inlines()
        .next()
        .ok_or("text")?
        .kind()
    else {
        return Err("plain text".into());
    };
    assert_eq!(text.text(), source);
    assert_eq!(Footnote::from(read), note);
    Ok(())
}

#[test]
fn canonical_definitions_always_have_a_body_and_transfer_independently() -> Result<(), Error> {
    let source = String::from("body");
    let mut definition = FootnoteDefinition::new(Vec::new(), 3, Location::default());
    definition.replace_id(Some(&source));
    definition.push_inline(InlineNode::PlainText(Plain::new(
        &source,
        Location::default(),
    )));
    let read = FootnoteDefinitionRef::from(&definition);
    let mut copy = FootnoteDefinition::from(read);
    assert_eq!(read.id(), Some(source.as_str()));
    assert_eq!(read.number(), 3);
    assert_eq!(read.content().inlines().count(), 1);
    assert_eq!(copy, definition);
    copy.take_content();
    assert_eq!(
        FootnoteDefinitionRef::from(&definition)
            .content()
            .inlines()
            .count(),
        1
    );
    let mut document = Document::default();
    document.set_footnotes([copy]);
    assert!(
        DocumentRef::from(&document)
            .footnotes()
            .next()
            .ok_or("empty catalog")?
            .content()
            .is_empty()
    );
    assert_eq!(
        DocumentRef::from(&document)
            .footnotes()
            .next()
            .ok_or("catalog")?
            .content()
            .inlines()
            .count(),
        0
    );
    assert_eq!(document.take_footnotes().count(), 1);
    Ok(())
}
