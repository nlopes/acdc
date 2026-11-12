use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{BlockKind, BlockRef, StructuralRef},
    parse, parse_file, parse_from_reader,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case::paragraph("Before.\n")]
#[case::unordered("* Before.\n")]
#[case::ordered(". Before.\n")]
#[case::description("Term:: Before.\n")]
#[case::callout("----\nx <1>\n----\n<1> Before.\n")]
#[case::attached("* item\n+\nBefore.\n")]
fn setext_continuation_lines_do_not_create_sections(
    #[case] prefix: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("= Doc\n\n{prefix}Title\n^^^^^\nAfter.\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(!parsed.document().descendants().any(|node| {
        matches!(node, StructuralRef::Block(block) if matches!(block.kind(), BlockKind::Section(_)))
    }));
    assert_eq!(parsed.document().toc_entries().count(), 0);
    assert!(parsed.document().references().get("_title").is_none());
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
#[case::blank_line("Before.\n\n")]
#[case::anchor("Before.\n[[title]]\n")]
#[case::attributes("Before.\n[#title]\n")]
#[case::listing("----\ncode\n----\n")]
fn setext_headings_still_require_a_block_boundary_and_asciidoctor_mode(
    #[case] prefix: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("= Doc\n\n{prefix}Title\n^^^^^\nAfter.\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(
        parsed.document().toc_entries().count(),
        usize::from(mode == CompatibilityMode::Asciidoctor)
    );
    Ok(())
}

#[rstest::rstest]
fn adjacent_setext_sibling_sections_keep_their_boundaries(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Doc\n\nFirst\n-----\nOther\n-----\nAfter.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(
        parsed.document().toc_entries().count(),
        if mode == CompatibilityMode::Asciidoctor {
            2
        } else {
            0
        }
    );
    Ok(())
}

#[rstest::rstest]
#[case::listing("-----", "code")]
#[case::example("=====", "content")]
#[case::passthrough("+++++", "raw")]
#[case::open_extension("~~~~~", "content")]
fn setext_like_underlines_still_open_delimited_blocks_inside_paragraphs(
    #[case] delimiter: &str,
    #[case] content: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("Before.\nTitle\n{delimiter}\n{content}\n{delimiter}\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let blocks = parsed.document().blocks().collect::<Vec<_>>();
    let [paragraph, delimited] = &*blocks else {
        return Err("expected paragraph and delimited block".into());
    };
    assert!(matches!(paragraph.kind(), BlockKind::Paragraph(_)));
    assert_eq!(paragraph.location().end().line(), 2);
    assert_eq!(delimited.location().start().line(), 3);
    assert_eq!(delimited.location().end().line(), 5);
    assert!(match delimiter {
        "-----" => matches!(delimited.kind(), BlockKind::Listing(_)),
        "=====" => matches!(delimited.kind(), BlockKind::Example(_)),
        "+++++" => matches!(delimited.kind(), BlockKind::Passthrough(_)),
        "~~~~~" => matches!(delimited.kind(), BlockKind::Open(_)),
        _ => false,
    });
    assert_eq!(parsed.document().toc_entries().count(), 0);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[test]
fn mode_selects_underlined_headings_for_every_input_entry_point() -> Result<(), Error> {
    let source = "Guide\n=====\n\nInstallation\n------------\n\nBody.\n";
    let root = tempfile::tempdir()?;
    let path = root.path().join("guide.adoc");
    std::fs::write(&path, source)?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for parsed in [
            parse(source, &options)?,
            parse_from_reader(source.as_bytes(), &options)?,
            parse_file(&path, &options)?,
        ] {
            let document = parsed.document();
            assert_eq!(
                document.header().is_some(),
                mode == CompatibilityMode::Asciidoctor
            );
            assert_eq!(
                document.toc_entries().count(),
                usize::from(mode == CompatibilityMode::Asciidoctor)
            );
            if let Some(entry) = document.toc_entries().next() {
                assert_eq!(entry.location().start().line(), 4);
                assert_eq!(entry.location().end().line(), 4);
            }
        }
    }
    Ok(())
}

#[test]
fn document_controls_do_not_select_underlined_heading_rules() -> Result<(), Error> {
    for control in [":compat-mode:", ":compat-mode!:", ":setext:", ":setext!:"] {
        let source = format!("= Guide\n{control}\n\nInstallation\n------------\n\nBody.\n");
        for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
            let options = Options::builder().with_compatibility_mode(mode).build()?;
            let parsed = parse(&source, &options)?;
            assert_eq!(
                matches!(
                    parsed.document().blocks().next().map(BlockRef::kind),
                    Some(BlockKind::Section(_))
                ),
                mode == CompatibilityMode::Asciidoctor,
                "{mode:?}: {control}"
            );
        }
    }
    Ok(())
}

#[test]
fn selected_includes_keep_underlined_heading_source_ranges() -> Result<(), Error> {
    let root = tempfile::tempdir()?;
    let child = root.path().join("child.adoc");
    let main = root.path().join("main.adoc");
    std::fs::write(
        &child,
        "Excluded\nRésumé\n------\n\nBody.\nExcluded tail.\n",
    )?;
    std::fs::write(&main, "= Main\n\ninclude::child.adoc[lines=2..5]\n")?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?;
        let parsed = parse_file(&main, &options)?;
        assert_eq!(
            parsed.document().toc_entries().count(),
            usize::from(mode == CompatibilityMode::Asciidoctor)
        );
        if let Some(entry) = parsed.document().toc_entries().next() {
            assert_eq!(entry.location().start().line(), 2);
            assert_eq!(entry.location().end().line(), 2);
            assert_eq!(entry.location().absolute_start(), "Excluded\n".len());
            assert_eq!(
                entry.location().absolute_end(),
                "Excluded\nRésumé".len() - 'é'.len_utf8()
            );
        }
    }
    Ok(())
}

#[test]
fn underline_width_matches_asciidoctor_character_tolerance() -> Result<(), Error> {
    for width in 3..=9 {
        let source = format!("= Guide\n\nRésumé\n{}\n\nBody.\n", "-".repeat(width));
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(
            parsed.document().toc_entries().count(),
            usize::from((5..=7).contains(&width)),
            "width {width}"
        );
    }
    Ok(())
}
