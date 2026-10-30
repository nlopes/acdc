use std::{collections::BTreeMap, error::Error, fs, iter::FusedIterator};

use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{
        BlockKind, ContentFieldRef, ContentOwnerRef, ContentRole, InlineKind, InlineRef,
        StructuralRef,
    },
    parse, parse_file,
};

use ContentRole::{
    Alt, Attribution, BibliographyLabel, Body, ButtonLabel, Caption, CiteTitle, DescriptionTerm,
    Footnote, IndexPrimary, IndexSecondary, IndexSee, IndexSeeAlso, IndexTertiary, KeyboardKey,
    LinkText, ListPrincipal, MenuItem, MenuTarget, ReferenceText, Span, Subtitle, Title, Tooltip,
};

type TestResult = Result<(), Box<dyn Error>>;

fn shared<T: Send + Sync>(_: &T) {}
fn fused<T: FusedIterator>(_: &T) {}

fn words<'a>(fields: impl Iterator<Item = ContentFieldRef<'a>>) -> Vec<String> {
    fields
        .flat_map(|field| field.content().inlines())
        .filter_map(|inline| match inline.kind() {
            InlineKind::Plain(text) => Some(text.text().to_owned()),
            InlineKind::Raw(text) => Some(text.text().to_owned()),
            InlineKind::Verbatim(text) => Some(text.text().to_owned()),
            InlineKind::Bold(_)
            | InlineKind::Italic(_)
            | InlineKind::Monospace(_)
            | InlineKind::Highlight(_)
            | InlineKind::Subscript(_)
            | InlineKind::Superscript(_)
            | InlineKind::CurvedQuotation(_)
            | InlineKind::CurvedApostrophe(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::LineBreak(_)
            | InlineKind::Anchor(_)
            | InlineKind::Callout(_)
            | InlineKind::Footnote(_)
            | InlineKind::Icon(_)
            | InlineKind::Image(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Button(_)
            | InlineKind::Menu(_)
            | InlineKind::Url(_)
            | InlineKind::Link(_)
            | InlineKind::Mailto(_)
            | InlineKind::Autolink(_)
            | InlineKind::CrossReference(_)
            | InlineKind::Pass(_)
            | InlineKind::Stem(_)
            | InlineKind::IndexTerm(_)
            | _ => None,
        })
        .collect()
}

#[rstest::rstest]
#[case("*text*", &[Span])]
#[case("_text_", &[Span])]
#[case("`text`", &[Span])]
#[case("#text#", &[Span])]
#[case("~text~", &[Span])]
#[case("^text^", &[Span])]
#[case("\"`text`\"", &[Span])]
#[case("'`text`'", &[Span])]
#[case("image:bird.png[alt=Alt,title=Tip]", &[Alt, Tooltip])]
#[case("icon:heart[alt=Alt,title=Tip]", &[Alt, Tooltip])]
#[case("link:guide.adoc[Label,title=Tip]", &[LinkText, Tooltip])]
#[case("https://example.org[Label,title=Tip]", &[LinkText, Tooltip])]
#[case("mailto:a@example.org[Label,title=Tip]", &[LinkText, Tooltip])]
#[case("xref:target[Label]", &[LinkText])]
#[case("footnote:[Note]", &[Footnote])]
#[case("[[target,Label]]", &[ReferenceText])]
#[case("btn:[Save]", &[ButtonLabel])]
#[case("kbd:[Ctrl+A]", &[KeyboardKey, KeyboardKey])]
#[case("menu:File[Open > Recent]", &[MenuTarget, MenuItem, MenuItem])]
#[case("indexterm:[One,Two,Three,see=Other]", &[IndexPrimary, IndexSecondary, IndexTertiary, IndexSee])]
#[case("indexterm2:[One,see-also=\"Two,Three\"]", &[IndexPrimary, IndexSeeAlso, IndexSeeAlso])]
#[case("plain", &[])]
#[case("https://example.org", &[])]
#[case("stem:[x]", &[])]
#[case("+++raw+++", &[])]
fn inline_field_inventory_is_ordered_and_shallow(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] markup: &str,
    #[case] expected: &[ContentRole],
) -> TestResult {
    let parsed = parse(
        &format!(":experimental:\n\nBefore {markup} after."),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let block = parsed.document_ref().blocks().next().ok_or("paragraph")?;
    let BlockKind::Paragraph(paragraph) = block.kind() else {
        return Err("paragraph".into());
    };
    let fields = paragraph
        .content()
        .inlines()
        .flat_map(InlineRef::content_fields)
        .collect::<Vec<_>>();
    assert_eq!(
        fields
            .iter()
            .copied()
            .map(ContentFieldRef::role)
            .collect::<Vec<_>>(),
        expected,
        "{markup}"
    );
    for field in fields {
        shared(&field);
        assert!(matches!(
            field.owner(),
            ContentOwnerRef::Inline(_) | ContentOwnerRef::Anchor(_)
        ));
    }
    Ok(())
}

#[rstest::rstest]
#[case("Paragraph.", &[Body])]
#[case("NOTE: Message", &[])]
#[case("// comment", &[])]
#[case("----\nlisting\n----", &[Body])]
#[case("....\nliteral\n....", &[Body])]
#[case("++++\npass\n++++", &[Body])]
#[case("[verse]\n____\nverse\n____", &[Body])]
#[case("////\ncomment\n////", &[Body])]
#[case("image::bird.png[alt=Bird]", &[Alt])]
#[case("audio::song.ogg[]", &[])]
#[case("video::clip.mp4[]", &[])]
#[case("***", &[])]
#[case("<<<\n\nAfter.", &[])]
#[case("toc::[]", &[])]
#[case("====\nparagraph\n====", &[])]
#[case("--\nparagraph\n--", &[])]
#[case("****\nparagraph\n****", &[])]
#[case("____\nparagraph\n____", &[])]
#[case("|===\n|cell\n|===", &[])]
#[case("[stem]\n++++\nx\n++++", &[])]
#[case("* Item", &[])]
#[case(". Item", &[])]
#[case("Term:: Description", &[])]
#[case("<1> Explanation", &[])]
#[case("[discrete]\n== Heading", &[])]
#[case("== Section", &[])]
fn block_fields_have_one_metadata_owner_and_no_child_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] markup: &str,
    #[case] body_roles: &[ContentRole],
) -> TestResult {
    let parsed = parse(
        &format!("{markup}\n"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let block = parsed.document_ref().blocks().next().ok_or("block")?;
    let actual = StructuralRef::from(block)
        .content_fields()
        .map(ContentFieldRef::role)
        .collect::<Vec<_>>();
    let expected = if markup.contains("== ") {
        &[Title][..]
    } else {
        body_roles
    };
    assert_eq!(actual, expected, "{markup}: {block:?}");
    Ok(())
}

#[rstest::rstest]
fn titles_metadata_and_items_have_stable_semantic_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let source = "= Header: Subtitle\n\n.Title\n[attribution='Credit',citetitle='Citation',caption='Prefix',reftext='Reference']\nParagraph.\n\nTerm:: Description\n\n* Item\n\n<1> Callout\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let doc = parsed.document_ref();
    let header = StructuralRef::from(doc.header().ok_or("header")?);
    assert_eq!(
        header
            .content_fields()
            .map(ContentFieldRef::role)
            .collect::<Vec<_>>(),
        [Title, Subtitle]
    );
    let first = doc.blocks().next().ok_or("paragraph")?;
    assert_eq!(
        StructuralRef::from(first)
            .content_fields()
            .map(ContentFieldRef::role)
            .collect::<Vec<_>>(),
        [Title, Body, Attribution, CiteTitle, Caption, ReferenceText]
    );
    let roles = doc
        .content_occurrences()
        .map(ContentFieldRef::role)
        .collect::<Vec<_>>();
    assert_eq!(
        roles.iter().filter(|&&role| role == ListPrincipal).count(),
        3
    );
    assert_eq!(
        roles
            .iter()
            .filter(|&&role| role == DescriptionTerm)
            .count(),
        1
    );
    assert_eq!(StructuralRef::from(doc).content_fields().count(), 0);
    Ok(())
}

#[rstest::rstest]
fn traversal_excludes_catalogs_and_generated_bibliography_text(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let source = "= Header\n:toc:\n:term: Expanded term\n\n== Section\n\n[[para,Reference label]]\nParagraph footnote:n[Note body] footnote:n[] image:x[Alt text,title=Tooltip text].\n\n[subs=\"macros,attributes,quotes\"]\nindexterm2:[{term}].\n\n[bibliography]\n* [[[book,Entry label]]] Bibliography body.\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = format!("{:?}", parsed.document());
    assert_eq!(parsed.document().footnotes.len(), 1);
    assert_eq!(parsed.document().toc_entries.len(), 1);
    assert!(!parsed.document().references.is_empty());
    let fields = parsed.document_ref().content_occurrences();
    shared(&fields);
    fused(&fields);
    let words = words(fields);
    for text in [
        "Header",
        "Section",
        "Reference label",
        "Note body",
        "Alt text",
        "Tooltip text",
        "Expanded term",
        "Entry label",
    ] {
        assert_eq!(
            words.iter().filter(|word| word.as_str() == text).count(),
            1,
            "{text}: {words:?}"
        );
    }
    assert!(
        !words
            .iter()
            .any(|word| word == "{term}" || word == "[Entry label]")
    );
    assert_eq!(
        parsed
            .document_ref()
            .content_occurrences()
            .filter(|field| field.role() == BibliographyLabel)
            .count(),
        1
    );
    assert_eq!(format!("{:?}", parsed.document()), before);
    Ok(())
}

#[test]
fn empty_fields_survive_and_subtree_skipping_keeps_siblings() -> TestResult {
    let parsed = parse(
        "image:x[alt='',title='*Tip*'] image:y[alt='*Bird*',title=''] link:z[]",
        &Options::default(),
    )?;
    let mut walk = parsed.document_ref().content_occurrences();
    let mut roles = Vec::new();
    while let Some(field) = walk.next() {
        roles.push((field.role(), field.content().is_empty()));
        if field.role() == Tooltip {
            walk.skip_descendants();
            walk.skip_descendants();
        }
    }
    assert_eq!(
        roles,
        [
            (Body, false),
            (Alt, true),
            (Tooltip, false),
            (Alt, false),
            (Span, false),
            (Tooltip, true),
            (LinkText, true)
        ]
    );
    assert!(walk.next().is_none());
    walk.skip_descendants();
    assert!(walk.next().is_none());
    let block = parsed.document_ref().blocks().next().ok_or("paragraph")?;
    assert_eq!(block.content_occurrences().count(), 8);
    let BlockKind::Paragraph(paragraph) = block.kind() else {
        return Err("paragraph".into());
    };
    let image = paragraph.content().inlines().next().ok_or("image")?;
    assert_eq!(
        image
            .content_occurrences()
            .map(ContentFieldRef::role)
            .collect::<Vec<_>>(),
        [Alt, Tooltip, Span]
    );
    Ok(())
}

#[rstest::rstest]
fn content_locations_keep_selected_include_and_table_origins(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let dir = tempfile::tempdir()?;
    let child =
        "// omitted\n// tag::keep[]\n    é image:x[alt='*Included*',title='Tip']\n// end::keep[]\n";
    fs::write(dir.path().join("child.adoc"), child)?;
    let source = ":value: Expanded\n\ninclude::child.adoc[tag=keep,indent=0]\n\n[cols=a]\n|===\n|image:y[alt='*Cell*',title='{value}']\n|===\n";
    let path = dir.path().join("main.adoc");
    fs::write(&path, source)?;
    let parsed = parse_file(
        &path,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let mut found = BTreeMap::new();
    for field in parsed.document_ref().content_occurrences() {
        for inline in field.content().inlines() {
            if let InlineKind::Plain(text) = inline.kind()
                && ["Included", "Tip", "Cell", "Expanded"].contains(&text.text())
            {
                let resolved = parsed.resolve_location(inline.location());
                let expected = if ["Included", "Tip"].contains(&text.text()) {
                    "child.adoc"
                } else {
                    "main.adoc"
                };
                assert_eq!(
                    resolved.file.ok_or("source file")?.canonicalize()?,
                    dir.path().join(expected).canonicalize()?
                );
                found.insert(
                    text.text().to_owned(),
                    (
                        inline.location().start().line(),
                        inline.location().start().column(),
                        inline.location().end().column(),
                    ),
                );
            }
        }
    }
    for (word, original, line, spelling) in [
        ("Included", child, 3, "Included"),
        ("Tip", child, 3, "Tip"),
        ("Cell", source, 7, "Cell"),
        ("Expanded", source, 7, "{value}"),
    ] {
        let text = original.lines().nth(line - 1).ok_or("line")?;
        let byte = text.find(spelling).ok_or("spelling")?;
        let start = u32::try_from(text[..byte].chars().count() + 1)?;
        let end = start + u32::try_from(spelling.chars().count() - 1)?;
        assert_eq!(
            found.get(word),
            Some(&(u32::try_from(line)?, start, end)),
            "{word}"
        );
    }
    Ok(())
}

#[rstest::rstest]
fn retained_anchor_labels_have_their_own_typed_owner(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let source = "[[earlier,Discarded]]\n[[later,Retained]]\nParagraph.\n\n[[other,Discarded too]]\n[reftext='Selected']\nSecond.\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let labels = parsed
        .document_ref()
        .content_occurrences()
        .filter(|field| field.role() == ReferenceText)
        .collect::<Vec<_>>();
    let [first, second] = labels.as_slice() else {
        return Err("two retained labels".into());
    };
    let ContentOwnerRef::Anchor(anchor) = first.owner() else {
        return Err("anchor owner".into());
    };
    assert_eq!(anchor.id(), "later");
    assert_eq!(words(anchor.content_occurrences()), ["Retained"]);
    assert!(matches!(
        second.owner(),
        ContentOwnerRef::Structural(StructuralRef::Block(_))
    ));
    assert_eq!(words(labels.into_iter()), ["Retained", "Selected"]);
    Ok(())
}
