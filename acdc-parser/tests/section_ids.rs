use acdc_parser::{
    Options, SafeMode,
    ast::{BlockKind, InlineKind, TocEntryRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn generated_section_ids_include_preceding_included_sections() -> Result<(), Error> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/preprocessor/section_ids_root.adoc");
    let parsed = parse_file(
        &root,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    assert_eq!(
        parsed
            .document()
            .toc_entries()
            .map(TocEntryRef::id)
            .collect::<Vec<_>>(),
        ["_same", "_same_2"]
    );
    let first = parsed
        .document()
        .references()
        .get("_same")
        .ok_or("missing first section")?;
    assert_eq!(first.location().start().line(), 1);
    assert_eq!(first.location().end().line(), 1);
    Ok(())
}

#[test]
fn generated_section_ids_include_setext_headings() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\nSame\n----\n\n== Same\n",
        &Options::builder().build()?,
    )?;
    assert_eq!(
        parsed
            .document()
            .toc_entries()
            .map(TocEntryRef::id)
            .collect::<Vec<_>>(),
        ["_same", "_same_2"]
    );
    Ok(())
}

#[test]
fn generated_section_ids_skip_occupied_suffixes_and_keep_toc_targets() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n[[_same_2]]\nReserved.\n\n== Same\n\n== Same\n\n=== Same\n\nSee <<_same_3>> and <<_same_4>>.\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let ids: Vec<_> = document.toc_entries().map(TocEntryRef::id).collect();
    assert_eq!(ids, ["_same", "_same_3", "_same_4"]);
    for id in ids {
        assert!(document.references().contains(id));
    }
    let BlockKind::Section(second) = document.blocks().nth(2).ok_or("missing section")?.kind()
    else {
        return Err("expected section".into());
    };
    assert_eq!(second.resolve_id(), "_same_3");
    let BlockKind::Section(child) = second.blocks().next().ok_or("missing child")?.kind() else {
        return Err("expected child section".into());
    };
    assert_eq!(child.resolve_id(), "_same_4");
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[test]
fn natural_section_references_follow_unique_ids_after_slug_collisions() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n== Same!\n\n== Same?\n\nSee <<Same!>> and <<Same?>>.\n",
        &Options::default(),
    )?;
    let BlockKind::Section(section) = parsed
        .document()
        .blocks()
        .nth(1)
        .ok_or("missing section")?
        .kind()
    else {
        return Err("expected section".into());
    };
    let BlockKind::Paragraph(paragraph) =
        section.blocks().next().ok_or("missing paragraph")?.kind()
    else {
        return Err("expected paragraph".into());
    };
    let targets: Vec<_> = paragraph
        .content()
        .inlines()
        .filter_map(|inline| {
            if let InlineKind::CrossReference(xref) = inline.kind() {
                Some(xref.target())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(targets, ["_same", "_same_2"]);
    Ok(())
}

#[test]
fn generated_section_ids_are_shared_across_asciidoc_table_cells() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n== Same\n\n[cols=\"a\"]\n|===\n|== Same\n\ncell\n|===\n\n== Same\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    assert_eq!(
        document
            .toc_entries()
            .map(TocEntryRef::id)
            .collect::<Vec<_>>(),
        ["_same", "_same_3"]
    );
    assert!(document.references().contains("_same_2"));
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}
