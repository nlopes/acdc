use crate::{
    CompatibilityMode, Options,
    ast::{
        BlockKind, ContentRef, DocumentRef, FootnoteDefinitionRef, FootnoteRef, InlineKind,
        LocationRef,
    },
    parse,
};

type Error = Box<dyn std::error::Error>;

fn shared<T: Copy + Send + Sync>(_: T) {}

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    content
        .inlines()
        .map(|inline| {
            if let InlineKind::Plain(node) = inline.kind() {
                Ok(node.text().to_owned())
            } else if let InlineKind::Raw(node) = inline.kind() {
                Ok(node.text().to_owned())
            } else if let InlineKind::Bold(node) = inline.kind() {
                text(node.content())
            } else {
                Err("expected text or bold content".into())
            }
        })
        .collect()
}

fn notes(content: ContentRef<'_>) -> impl Iterator<Item = FootnoteRef<'_>> + Send + Sync {
    content.inlines().filter_map(|inline| {
        if let InlineKind::Footnote(note) = inline.kind() {
            Some(note)
        } else {
            None
        }
    })
}

fn paragraph(document: DocumentRef<'_>) -> Result<ContentRef<'_>, Error> {
    if let Some(block) = document.blocks().next()
        && let BlockKind::Paragraph(paragraph) = block.kind()
    {
        return Ok(paragraph.content());
    }
    Err("paragraph".into())
}

fn slice<'source>(source: &'source str, location: LocationRef<'_>) -> Option<&'source str> {
    source.get(location.absolute_start()..=location.absolute_end())
}

#[rstest::rstest]
fn footnote_occurrences_and_definitions_remain_separate(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "é footnote:n[A *bold* note.] footnote:n[] footnote:n[Discarded footnote:[Nested.]] footnote:[Anonymous.]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document_storage())?;
    let catalog_before = parsed.document_storage().footnotes.clone();
    let warnings_before = format!("{:?}", parsed.warnings());
    let occurrences: Vec<_> = notes(paragraph(parsed.document())?).collect();
    let &[first, reference, repeated, anonymous] = occurrences.as_slice() else {
        return Err("four footnote occurrences".into());
    };
    assert_eq!(
        occurrences
            .iter()
            .map(|note| note.number())
            .collect::<Vec<_>>(),
        [1, 1, 1, 2]
    );
    assert_eq!(
        occurrences.iter().map(|note| note.id()).collect::<Vec<_>>(),
        [Some("n"), Some("n"), Some("n"), None]
    );
    assert_eq!(
        text(first.content().ok_or("definition body")?)?,
        "A bold note."
    );
    assert!(reference.content().is_none());
    assert!(repeated.content().is_none());
    assert_eq!(
        text(anonymous.content().ok_or("anonymous body")?)?,
        "Anonymous."
    );
    assert_eq!(
        slice(source, first.location()),
        Some("footnote:n[A *bold* note.]")
    );
    assert_eq!(slice(source, reference.location()), Some("footnote:n[]"));
    let definitions = {
        let document = parsed.document();
        document.footnotes()
    };
    let definitions: Vec<FootnoteDefinitionRef<'_>> = definitions.collect();
    assert_eq!(definitions.len(), 2);
    for (definition, occurrence) in definitions.iter().zip([first, anonymous]) {
        shared(*definition);
        shared(occurrence);
        assert_eq!(definition.id(), occurrence.id());
        assert_eq!(definition.number(), occurrence.number());
        assert_eq!(Some(definition.content()), occurrence.content());
        assert_eq!(
            definition.location().absolute_start(),
            occurrence.location().absolute_start()
        );
    }
    let bold = first
        .content()
        .ok_or("definition body")?
        .inlines()
        .find(|inline| matches!(inline.kind(), InlineKind::Bold(_)))
        .ok_or("bold")?;
    assert_eq!(slice(source, bold.location()), Some("*bold*"));
    assert_eq!(serde_json::to_value(parsed.document_storage())?, before);
    assert_eq!(parsed.document_storage().footnotes, catalog_before);
    assert_eq!(format!("{:?}", parsed.warnings()), warnings_before);
    Ok(())
}
