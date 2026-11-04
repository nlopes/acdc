use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{BlockKind, BlockRef},
    parse, parse_file, parse_from_reader,
};

type Error = Box<dyn std::error::Error>;

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
            let document = parsed.document_ref();
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
                    parsed.document_ref().blocks().next().map(BlockRef::kind),
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
            parsed.document_ref().toc_entries().count(),
            usize::from(mode == CompatibilityMode::Asciidoctor)
        );
        if let Some(entry) = parsed.document_ref().toc_entries().next() {
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
            parsed.document_ref().toc_entries().count(),
            usize::from((5..=7).contains(&width)),
            "width {width}"
        );
    }
    Ok(())
}
