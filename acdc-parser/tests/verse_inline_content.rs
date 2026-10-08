use acdc_parser::{Block, DelimitedBlockType, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn verse_inline_locations_and_catalogs_follow_the_source(
    #[values("\n", "\r\n")] newline: &str,
) -> Result<(), Error> {
    let source = "= Verse\n:value: expanded\n\n[verse]\n____\nÉva *Bold* {value}\n  link:https://example.org[Link]\n\nanchor:target[]footnote:note[Verse note.] ((Term))\n____\n\nReuse footnote:note[].\n"
        .replace('\n', newline);
    let parsed = parse(&source, &Options::default())?;
    let Some(Block::DelimitedBlock(block)) = parsed.document().blocks.first() else {
        return Err("expected verse block".into());
    };
    let DelimitedBlockType::DelimitedVerse(nodes) = &block.inner else {
        return Err("expected verse content".into());
    };
    assert!(
        nodes
            .iter()
            .any(|node| matches!(node, InlineNode::BoldText(_)))
    );
    for expected in [
        "*Bold*",
        "link:https://example.org[Link]",
        "footnote:note[Verse note.]",
        "((Term))",
        "anchor:target[]",
    ] {
        assert!(
            nodes.iter().any(|node| {
                let location = node.location();
                parsed
                    .source()
                    .get(location.absolute_start..=location.absolute_end)
                    == Some(expected)
            }),
            "missing complete source span: {expected}"
        );
    }
    let [note] = parsed.document().footnotes.as_slice() else {
        return Err("expected one reused footnote".into());
    };
    assert_ne!(note.content, []);
    assert_eq!(note.number, 1);
    Ok(())
}

#[test]
fn verse_explicit_substitutions_follow_the_feature_contract() -> Result<(), Error> {
    let source = "[verse,subs=\"none\"]\n____\n*Bold* footnote:[Inactive.]\n____\n";
    let parsed = parse(source, &Options::default())?;
    assert_eq!(
        parsed.document().footnotes.len(),
        usize::from(!cfg!(feature = "pre-spec-subs"))
    );
    Ok(())
}
