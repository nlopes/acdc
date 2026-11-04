//! Strict export identifies unsupported owner properties without changing the AST.
#![cfg(feature = "asg")]

use acdc_parser::{AttributeValue, Block, Document, Header, Location, Paragraph, Title, asg};

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn absent_header_title_reports_the_required_property() {
    let mut document = Document::default();
    document.header = Some(Header::new(Title::default(), Location::default()));
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::Unsupported {path, ..}) if path == "/header/title")
    );
}

#[test]
fn dedicated_metadata_cannot_be_shadowed_by_scalar_attributes() {
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    paragraph.metadata.style = Some("example");
    paragraph
        .metadata
        .attributes
        .set("style".into(), AttributeValue::String("source".into()));
    let mut document = Document::default();
    document.blocks.push(Block::Paragraph(paragraph));
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/metadata/attributes/style")
    );
}

#[test]
fn unsupported_nested_content_keeps_its_own_source() -> Result<(), Error> {
    for mode in [
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft,
    ] {
        let source = "é *Before image:photo.png[Alt] after*.";
        let parsed = acdc_parser::parse(
            source,
            &acdc_parser::Options::builder()
                .with_compatibility_mode(mode)
                .build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let Err(asg::Error::Unsupported {
            path,
            construct,
            location,
            ..
        }) = asg::document(parsed.document())
        else {
            return Err("wrong error category".into());
        };
        assert_eq!(path, "/blocks/0/inlines/1/inlines/1");
        assert_eq!(construct, Some("inline image"));
        let location = location.ok_or("image source")?;
        assert_eq!(
            source.get(location.absolute_start..=location.absolute_end),
            Some("image:photo.png[Alt]")
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
    }
    Ok(())
}

#[test]
fn unknown_inline_source_does_not_borrow_the_parents_coordinates() {
    let mut document = Document::default();
    document.location.start.line = 1;
    document.location.end.line = 10;
    let mut paragraph = Paragraph::new(
        vec![acdc_parser::InlineNode::VerbatimText(
            acdc_parser::Verbatim {
                content: "code",
                location: Location::default(),
            },
        )],
        document.location.clone(),
    );
    paragraph.location.end.line = 2;
    document.blocks.push(Block::Paragraph(paragraph));
    assert!(matches!(
        asg::document((&document).into()),
        Err(asg::Error::Unsupported {
            construct: Some("verbatim text"),
            location: None,
            ..
        })
    ));
}

#[test]
fn invalid_locations_are_distinct_from_unsupported_values() {
    let mut location = Location::default();
    location.start.line = 1;
    let mut document = Document::default();
    document
        .blocks
        .push(Block::Paragraph(Paragraph::new(Vec::new(), location)));
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::InvalidLocation {
        path, construct: Some("paragraph"), location: None, ..
    }) if path == "/blocks/0/location/1")
    );
}

#[test]
fn omitted_comments_do_not_shift_error_paths() -> Result<(), Error> {
    let parsed = acdc_parser::parse(
        "// Source comment\n\nimage::photo.png[Alt]",
        &acdc_parser::Options::default(),
    )?;
    assert!(
        matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported {
        path, construct: Some("block image"), location: Some(_), ..
    }) if path == "/blocks/0/alt")
    );
    Ok(())
}

#[test]
fn unsupported_source_keeps_include_chains() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let included = directory.path().join("included.adoc");
    std::fs::write(&included, "é *Before image:photo.png[Alt] after*.\n")?;
    let source = directory.path().join("main.adoc");
    std::fs::write(&source, "include::included.adoc[]\n")?;
    let parsed = acdc_parser::parse_file(
        &source,
        &acdc_parser::Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .build()?,
    )?;
    let Err(asg::Error::Unsupported {
        construct,
        location,
        ..
    }) = asg::document(parsed.document())
    else {
        return Err("wrong error category".into());
    };
    assert_eq!(construct, Some("inline image"));
    let location = location.ok_or("included image location")?;
    assert_eq!(location.start.line, 1);
    assert!(
        location
            .start
            .file
            .as_ref()
            .is_some_and(|chain| chain.iter().any(|part| part.ends_with("included.adoc")))
    );
    Ok(())
}

#[test]
fn protected_document_values_have_no_invented_declaration_range() -> Result<(), Error> {
    let parsed = acdc_parser::parse(
        "= Guide\n:protected: pass:q[*Bold*]\n\nText.",
        &acdc_parser::Options::default(),
    )?;
    assert!(
        matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported {
        path, construct: Some("document attribute"), location: None, ..
    }) if path == "/attributes/protected")
    );
    Ok(())
}

#[test]
fn section_policy_errors_identify_the_native_property() {
    for (kind, numbered, expected) in [
        (acdc_parser::SectionKind::Appendix, false, "kind"),
        (acdc_parser::SectionKind::Normal, true, "numbering"),
    ] {
        let mut section =
            acdc_parser::Section::new(Title::new(Vec::new()), 1, Vec::new(), Location::default())
                .with_numbering(numbered);
        section.kind = kind;
        let mut document = Document::default();
        document.blocks.push(Block::Section(section));
        assert!(
            matches!(asg::document((&document).into()), Err(asg::Error::Unsupported {
            path, construct: Some("section"), ..
        }) if path == format!("/blocks/0/{expected}"))
        );
    }
}
