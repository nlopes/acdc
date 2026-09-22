use acdc_parser::{Block, InlineMacro, InlineNode, Options, SafeMode, parse, parse_file};

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
            .toc_entries
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>(),
        ["_same", "_same_2"]
    );
    let first = parsed
        .document()
        .references
        .get("_same")
        .ok_or("missing first section")?;
    assert_eq!(first.location.start.line, 1);
    assert_eq!(first.location.end.line, 1);
    Ok(())
}

#[cfg(feature = "setext")]
#[test]
fn generated_section_ids_include_setext_headings() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\nSame\n----\n\n== Same\n",
        &Options::builder().with_setext().build()?,
    )?;
    assert_eq!(
        parsed
            .document()
            .toc_entries
            .iter()
            .map(|entry| entry.id)
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
    let ids: Vec<_> = document.toc_entries.iter().map(|entry| entry.id).collect();
    assert_eq!(ids, ["_same", "_same_3", "_same_4"]);
    for id in ids {
        assert!(document.references.contains_key(id));
    }
    let Block::Section(second) = document.blocks.get(2).ok_or("missing section")? else {
        return Err("expected section".into());
    };
    assert_eq!(second.id(), "_same_3");
    let Block::Section(child) = second.content.first().ok_or("missing child")? else {
        return Err("expected child section".into());
    };
    assert_eq!(child.id(), "_same_4");
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[test]
fn natural_section_references_follow_unique_ids_after_slug_collisions() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n== Same!\n\n== Same?\n\nSee <<Same!>> and <<Same?>>.\n",
        &Options::default(),
    )?;
    let Block::Section(section) = parsed.document().blocks.get(1).ok_or("missing section")? else {
        return Err("expected section".into());
    };
    let Block::Paragraph(paragraph) = section.content.first().ok_or("missing paragraph")? else {
        return Err("expected paragraph".into());
    };
    let targets: Vec<_> = paragraph
        .content
        .iter()
        .filter_map(|inline| {
            if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline {
                Some(xref.target)
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
            .toc_entries
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>(),
        ["_same", "_same_3"]
    );
    assert!(document.references.contains_key("_same_2"));
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}
