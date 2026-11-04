//! Typed table reads retain authored delimiter spelling and source origins.

use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{BlockKind, StructuralRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn nested_tables_retain_delimiters_and_unicode_source_offsets(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "π 😀 Before.\n\n[cols=a]\n|===\n|\n!===\n!Nested\n!===\n|===\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let tables = parsed
        .document()
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::Block(block) = node
                && let BlockKind::Table(table) = block.kind()
            {
                return Some(table);
            }
            None
        })
        .collect::<Vec<_>>();
    assert_eq!(tables.len(), 2);
    for (table, delimiter, opening, closing) in [
        (tables.first().ok_or("outer")?, "|===", 4, 9),
        (tables.get(1).ok_or("nested")?, "!===", 6, 8),
    ] {
        assert_eq!(table.delimiter(), delimiter);
        for (location, line) in [
            (table.open_delimiter_location().ok_or("opening")?, opening),
            (table.close_delimiter_location().ok_or("closing")?, closing),
        ] {
            assert_eq!(location.start().line(), line);
            assert_eq!(
                source.get(location.absolute_start()..=location.absolute_end()),
                Some(delimiter)
            );
        }
    }
    Ok(())
}

#[rstest::rstest]
fn selected_table_delimiters_resolve_to_original_include_ranges(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let included = directory.path().join("body.adoc");
    std::fs::write(&main, "include::body.adoc[lines=2..4,indent=0]\n")?;
    std::fs::write(&included, "// skipped\n  |===\n  |Cell\n  |===\n")?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let BlockKind::Table(table) = parsed.document().blocks().next().ok_or("block")?.kind() else {
        return Err("table".into());
    };
    for (location, line) in [
        (table.open_delimiter_location().ok_or("opening")?, 2),
        (table.close_delimiter_location().ok_or("closing")?, 4),
    ] {
        let source = parsed.resolve_location(location);
        assert_eq!(source.file.as_ref(), Some(&included));
        assert_eq!(source.location.start.line, line);
        assert_eq!(source.location.start.column, 3);
    }
    Ok(())
}
