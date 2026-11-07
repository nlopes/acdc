use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{BlockKind, BlockRef, InlineKind},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn comments_do_not_consume_a_list_attachment_or_register_hidden_macros() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for comment in [
            "// Hidden footnote:[Hidden]\n",
            "////\nHidden footnote:[Hidden]\n////\n",
            "// One\n// Two\n",
        ] {
            let source = format!("* First\n+\n{comment}Attached footnote:[Visible].\n\n* Second\n");
            let parsed = parse(
                &source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let Some(BlockKind::UnorderedList(list)) =
                parsed.document().blocks().next().map(BlockRef::kind)
            else {
                return Err("expected list".into());
            };
            assert_eq!(list.items().count(), 2);
            let Some(BlockKind::Paragraph(paragraph)) = list
                .items()
                .next()
                .ok_or("missing first item")?
                .blocks()
                .next()
                .map(BlockRef::kind)
            else {
                return Err("expected attached paragraph".into());
            };
            assert_eq!(
                paragraph
                    .content()
                    .inlines()
                    .filter(|node| matches!(node.kind(), InlineKind::Footnote(_)))
                    .count(),
                1
            );
            assert_eq!(parsed.document().footnotes().count(), 1);
            assert_eq!(parsed.document().blocks().count(), 1);
        }
    }
    Ok(())
}

#[test]
fn attached_selected_include_content_keeps_original_ranges() -> Result<(), Error> {
    let root = tempfile::tempdir()?;
    let child = root.path().join("child.adoc");
    let main = root.path().join("main.adoc");
    let source = "Excluded.\n// Leading comment\nAttached π body.\nExcluded tail.\n";
    std::fs::write(&child, source)?;
    std::fs::write(
        &main,
        "* First\n+\ninclude::child.adoc[lines=2..3]\n\n* Second\n",
    )?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &main,
            &Options::builder()
                .with_compatibility_mode(mode)
                .with_safe_mode(SafeMode::Unsafe)
                .build()?,
        )?;
        let Some(BlockKind::UnorderedList(list)) =
            parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("expected list".into());
        };
        let Some(BlockKind::Paragraph(paragraph)) = list
            .items()
            .next()
            .ok_or("missing first item")?
            .blocks()
            .next()
            .map(BlockRef::kind)
        else {
            return Err("expected attached paragraph".into());
        };
        assert_eq!(paragraph.location().start().line(), 3);
        assert_eq!(paragraph.location().end().line(), 3);
        assert_eq!(
            paragraph.location().absolute_start(),
            source.find("Attached").ok_or("source body")?
        );
        assert_eq!(
            paragraph.location().absolute_end(),
            source.find("body.").ok_or("source end")? + "body".len()
        );
        assert_eq!(list.items().count(), 2);
        assert_eq!(parsed.document().blocks().count(), 1);
    }
    Ok(())
}

#[test]
fn a_blank_line_after_the_comment_ends_the_attachment() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for comment in ["// Comment\n", "////\nComment\n////\n"] {
            let source = format!("* First\n+\n{comment}\nOutside.\n\n* Second\n");
            let parsed = parse(
                &source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            assert_eq!(parsed.document().blocks().count(), 3);
            assert!(matches!(
                parsed.document().blocks().nth(1).map(BlockRef::kind),
                Some(BlockKind::Paragraph(_))
            ));
        }
    }
    Ok(())
}
