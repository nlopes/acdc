use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn verse_inline_locations_and_catalogs_follow_the_source(
    #[values("\n", "\r\n")] newline: &str,
) -> Result<(), Error> {
    let source = "= Verse\n:value: expanded\n\n[verse]\n____\nÉva *Bold* {value}\n  link:https://example.org[Link]\n\nanchor:target[]footnote:note[Verse note.] ((Term))\n____\n\nReuse footnote:note[].\n"
        .replace('\n', newline);
    let parsed = parse(&source, &Options::default())?;
    let Some(BlockKind::Verse(block)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected verse block".into());
    };
    let nodes = block.content();
    assert!(
        nodes
            .inlines()
            .any(|node| matches!(node.kind(), InlineKind::Bold(_)))
    );
    for expected in [
        "*Bold*",
        "link:https://example.org[Link]",
        "footnote:note[Verse note.]",
        "((Term))",
        "anchor:target[]",
    ] {
        assert!(
            nodes.inlines().any(|node| {
                let location = node.location();
                parsed
                    .source()
                    .get(location.absolute_start()..=location.absolute_end())
                    == Some(expected)
            }),
            "missing complete source span: {expected}"
        );
    }
    let [note] = &*parsed.document().footnotes().collect::<Vec<_>>() else {
        return Err("expected one reused footnote".into());
    };
    assert!(!note.content().is_empty());
    assert_eq!(note.number(), 1);
    Ok(())
}

#[test]
fn verse_explicit_substitutions_follow_the_feature_contract() -> Result<(), Error> {
    let source = "[verse,subs=\"none\"]\n____\n*Bold* footnote:[Inactive.]\n____\n";
    let parsed = parse(source, &Options::default())?;
    assert_eq!(parsed.document().footnotes().count(), 0);
    Ok(())
}
