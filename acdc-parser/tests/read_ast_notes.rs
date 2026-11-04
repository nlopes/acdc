use acdc_parser::{
    Anchor, CalloutRefKind, CompatibilityMode, ContentAttribute, InlineMacro, InlineNode, Location,
    Options, SafeMode,
    ast::{
        AnchorRef, BlockKind, CalloutRef, ContentRef, DocumentRef, FootnoteRef, InlineKind,
        LocationRef,
    },
    parse, parse_file, parse_inline,
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

#[test]
fn footnote_catalog_reads_preserve_registration_snapshots() -> Result<(), Error> {
    let parsed = parse(
        ":word: Expanded\n\n[subs=\"specialcharacters,macros,attributes\"]\nfootnote:n[{word}] footnote:n[]",
        &Options::default(),
    )?;
    let mut occurrences = notes(paragraph(parsed.document_ref())?);
    assert_eq!(
        text(occurrences.next().ok_or("definition")?.content())?,
        "{word}"
    );
    assert!(occurrences.next().ok_or("reference")?.content().is_empty());
    let definition = parsed
        .document_ref()
        .footnotes()
        .next()
        .ok_or("catalog definition")?;
    assert_eq!(text(definition.content())?, "{word}");
    assert_eq!(definition.number(), 1);
    Ok(())
}

#[test]
fn footnote_owner_reads_do_not_resolve_or_register_content() -> Result<(), Error> {
    let parsed = parse_inline("footnote:n[Body] footnote:n[]", &Options::default())?;
    let [
        InlineNode::Macro(InlineMacro::Footnote(owner)),
        _,
        InlineNode::Macro(InlineMacro::Footnote(reference)),
    ] = parsed.inlines()
    else {
        return Err("definition and reference".into());
    };
    let mut owner = owner.clone();
    owner.content.clear();
    owner.number = 17;
    let read = FootnoteRef::from(&owner);
    assert_eq!(read.number(), 17);
    assert_eq!(read.id(), Some("n"));
    assert!(read.content().is_empty());
    assert!(FootnoteRef::from(reference).content().is_empty());
    assert!(
        parse("No notes.", &Options::default())?
            .document_ref()
            .footnotes()
            .next()
            .is_none()
    );
    Ok(())
}

#[rstest::rstest]
fn bibliography_reads_separate_citation_from_display_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "[bibliography]\n* [[[book,*Bold*]]] Entry.\n* [[[fallback]]] Entry.\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document_ref())?;
    let BlockKind::UnorderedList(list) =
        parsed.document_ref().blocks().next().ok_or("list")?.kind()
    else {
        return Err("bibliography".into());
    };
    let anchors: Vec<_> = list
        .items()
        .filter_map(|item| {
            item.principal().inlines().find_map(|inline| {
                if let InlineKind::Anchor(anchor) = inline.kind() {
                    Some(anchor)
                } else {
                    None
                }
            })
        })
        .collect();
    let &[anchor, fallback] = anchors.as_slice() else {
        return Err("two bibliography anchors".into());
    };
    shared(anchor);
    assert!(anchor.is_bibliography());
    assert_eq!(anchor.id(), "book");
    assert_eq!(text(anchor.reftext().ok_or("reference text")?)?, "Bold");
    let display = anchor.bibliography_label().ok_or("display label")?;
    assert_eq!(text(display)?, "Bold");
    assert!(matches!(
        display.inlines().next().ok_or("bold")?.kind(),
        InlineKind::Bold(_)
    ));
    assert_eq!(
        slice(source, display.inlines().next().ok_or("bold")?.location()),
        Some("*Bold*")
    );
    assert!(fallback.is_bibliography());
    assert_eq!(fallback.id(), "fallback");
    assert!(fallback.bibliography_label().is_none());
    assert_eq!(serde_json::to_value(parsed.document_ref())?, before);
    Ok(())
}

#[test]
fn ordinary_anchor_reads_preserve_absent_and_empty_labels() -> Result<(), Error> {
    let mut owner = Anchor::new("entry", Location::default());
    assert!(AnchorRef::from(&owner).reftext().is_none());
    owner.xreflabel = Some(ContentAttribute::default());
    let read = AnchorRef::from(&owner);
    assert!(read.reftext().ok_or("present empty")?.is_empty());
    assert!(!read.is_bibliography());
    assert!(read.bibliography_label().is_none());
    let source = "é [[inline,Label]] here.";
    let parsed = parse(source, &Options::default())?;
    let anchor = paragraph(parsed.document_ref())?
        .inlines()
        .find_map(|inline| {
            if let InlineKind::Anchor(anchor) = inline.kind() {
                Some(anchor)
            } else {
                None
            }
        })
        .ok_or("inline anchor")?;
    assert_eq!(anchor.id(), "inline");
    assert_eq!(text(anchor.reftext().ok_or("label")?)?, "Label");
    assert!(!anchor.is_bibliography());
    assert_eq!(slice(source, anchor.location()), Some("[[inline,Label]]"));
    Ok(())
}

#[test]
fn bibliography_display_reads_do_not_replace_reference_text() -> Result<(), Error> {
    let parsed = parse(
        "[bibliography]\n* [[[entry,pass:a[{safe-mode-name}]]]] Entry.\n",
        &Options::builder()
            .with_safe_mode(SafeMode::Secure)
            .build()?,
    )?;
    let BlockKind::UnorderedList(list) =
        parsed.document_ref().blocks().next().ok_or("list")?.kind()
    else {
        return Err("bibliography".into());
    };
    let InlineKind::Anchor(anchor) = list
        .items()
        .next()
        .ok_or("item")?
        .principal()
        .inlines()
        .next()
        .ok_or("anchor")?
        .kind()
    else {
        return Err("anchor".into());
    };
    assert_eq!(
        text(anchor.reftext().ok_or("reference text")?)?,
        "pass:a[{safe-mode-name}"
    );
    assert_eq!(
        text(anchor.bibliography_label().ok_or("display label")?)?,
        "secure"
    );
    Ok(())
}

#[test]
fn callout_leaves_keep_local_numbers_and_marker_locations() -> Result<(), Error> {
    let source = "----\nfirst <.>\nsecond <2>\n----\n<.> First\n<2> Second\n\n----\nnew sequence <.>\n----\n<.> Again\n";
    let parsed = parse(source, &Options::default())?;
    let before = serde_json::to_value(parsed.document_ref())?;
    let mut found = Vec::new();
    for block in parsed.document_ref().blocks() {
        if let BlockKind::Listing(listing) = block.kind() {
            for inline in listing.content().inlines() {
                if let InlineKind::Callout(callout) = inline.kind() {
                    shared(callout);
                    found.push((
                        callout.kind(),
                        callout.number(),
                        slice(source, callout.location()),
                    ));
                }
            }
        }
    }
    assert_eq!(
        found,
        [
            (CalloutRefKind::Auto, 1, Some("<.>")),
            (CalloutRefKind::Explicit, 2, Some("<2>")),
            (CalloutRefKind::Auto, 1, Some("<.>"))
        ]
    );
    let owner = acdc_parser::CalloutRef::explicit(9, Location::default());
    assert_eq!(CalloutRef::from(&owner).number(), 9);
    assert_eq!(serde_json::to_value(parsed.document_ref())?, before);
    Ok(())
}

#[rstest::rstest]
fn footnote_reads_preserve_included_and_table_cell_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let include = directory.path().join("notes.adoc");
    std::fs::write(
        &main,
        "include::notes.adoc[]\n\n[cols=a]\n|===\n|footnote:n[] footnote:cell[Cell.]\n|===\n",
    )?;
    let source = "é footnote:n[Included *body*.]\n";
    std::fs::write(&include, source)?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let first = notes(paragraph(parsed.document_ref())?)
        .next()
        .ok_or("included footnote")?;
    let resolved = parsed.resolve_location(first.location());
    assert_eq!(resolved.file, Some(include.clone()));
    assert_eq!(resolved.location.start.line, 1);
    assert_eq!(resolved.location.start.column, 3);
    assert_eq!(
        slice(source, first.location()),
        Some("footnote:n[Included *body*.]")
    );
    let definition = parsed
        .document_ref()
        .footnotes()
        .next()
        .ok_or("definition")?;
    assert_eq!(
        parsed.resolve_location(definition.location()).file,
        Some(include)
    );
    let BlockKind::Table(table) = parsed.document_ref().blocks().nth(1).ok_or("table")?.kind()
    else {
        return Err("table".into());
    };
    let cell = table
        .rows()
        .next()
        .ok_or("row")?
        .cells()
        .next()
        .ok_or("cell")?;
    let BlockKind::Paragraph(paragraph) = cell.blocks().next().ok_or("cell paragraph")?.kind()
    else {
        return Err("cell paragraph".into());
    };
    let mut occurrences = notes(paragraph.content());
    let reference = occurrences.next().ok_or("cell reference")?;
    assert_eq!(reference.number(), first.number());
    assert!(reference.content().is_empty());
    let definition = occurrences.next().ok_or("cell definition")?;
    assert_eq!(definition.number(), 2);
    assert_eq!(text(definition.content())?, "Cell.");
    assert_eq!(
        parsed.resolve_location(definition.location()).file,
        Some(main)
    );
    assert_eq!(parsed.document_ref().footnotes().count(), 2);
    Ok(())
}
