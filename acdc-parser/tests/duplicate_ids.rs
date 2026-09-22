use std::path::Path;

use acdc_parser::{InlineNode, Options, WarningKind, parse, parse_file};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("= T\n\n[[same]]\n== First\n\n[[same]]\n== Second\n")]
#[case("[[same]]\nFirst.\n\nAnother [[same]]target.\n")]
#[case("A [#same]*first* span and [#same]*second* span.\n")]
#[case("link:https://example.com[First,id=same]\n\nlink:https://example.org[Second,id=same]\n")]
#[case("[bibliography]\n* [[[same]]] First.\n* [[[same]]] Second.\n")]
fn duplicate_ids_warn_once_with_both_source_positions(#[case] input: &str) -> Result<(), Error> {
    let parsed = parse(input, &Options::default())?;
    let [warning] = parsed.warnings() else {
        return Err(format!("expected one warning, got {:?}", parsed.warnings()).into());
    };
    let WarningKind::DuplicateId { id, first } = &warning.kind else {
        return Err(format!("unexpected warning: {warning}").into());
    };
    assert_eq!(id, "same");
    let duplicate = warning
        .source_location()
        .ok_or("missing duplicate location")?;
    assert!(first.location.absolute_start < duplicate.location.absolute_start);
    assert!(warning.kind.to_string().contains("id already in use: same"));
    assert!(warning.advice().is_some());
    Ok(())
}

#[test]
fn duplicate_sections_keep_the_first_reference_text() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n[[same]]\n== First\n\n[[same]]\n== Second\n\nSee <<same>>.\n",
        &Options::default(),
    )?;
    let title = parsed
        .document()
        .references
        .get("same")
        .and_then(|reference| reference.title.as_ref())
        .ok_or("missing reference title")?;
    assert!(matches!(&title[..], [InlineNode::PlainText(text)] if text.content == "First"));
    Ok(())
}

#[test]
fn duplicate_sections_do_not_register_later_title_aliases() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n[[same]]\n== First\n\n[[same]]\n== Second\n\nSee <<Second>>.\n",
        &Options::default(),
    )?;
    assert!(parsed.warnings().iter().any(|warning| {
        matches!(&warning.kind, WarningKind::UnresolvedReference { target } if target == "Second")
    }));
    Ok(())
}

#[test]
fn duplicate_section_does_not_replace_an_earlier_block() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n[[same]]\n.First block\n====\nContent.\n====\n\n[[same]]\n== Second section\n",
        &Options::default(),
    )?;
    let title = parsed
        .document()
        .references
        .get("same")
        .and_then(|reference| reference.title.as_ref())
        .ok_or("missing reference title")?;
    assert!(matches!(&title[..], [InlineNode::PlainText(text)] if text.content == "First block"));
    assert_eq!(parsed.warnings().len(), 1);
    Ok(())
}

#[test]
fn synthetic_admonition_paragraph_is_not_a_duplicate_definition() -> Result<(), Error> {
    let parsed = parse("[[same]]\nNOTE: Content.\n", &Options::default())?;
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[test]
fn duplicate_ids_report_both_original_included_files() -> Result<(), Error> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/preprocessor");
    let parsed = parse_file(
        directory.join("duplicate_ids_root.adoc"),
        &Options::default(),
    )?;
    let [warning] = parsed.warnings() else {
        return Err(format!("expected one warning, got {:?}", parsed.warnings()).into());
    };
    let WarningKind::DuplicateId { first, .. } = &warning.kind else {
        return Err("expected duplicate ID warning".into());
    };
    let duplicate = warning
        .source_location()
        .ok_or("missing duplicate location")?;
    assert_eq!(
        first.file.as_deref(),
        Some(directory.join("duplicate_ids_first.adoc").as_path())
    );
    assert_eq!(
        duplicate.file.as_deref(),
        Some(directory.join("duplicate_ids_second.adoc").as_path())
    );
    assert_eq!(first.location.start.line, 1);
    assert_eq!(duplicate.location.start.line, 1);
    assert!(
        warning
            .kind
            .to_string()
            .contains("duplicate_ids_first.adoc:1:1")
    );
    Ok(())
}
