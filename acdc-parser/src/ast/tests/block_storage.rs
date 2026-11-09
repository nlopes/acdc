use crate::{
    Block, CompatibilityMode, Location, Options, Section, SectionKind, Title, ast::SectionRef,
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn typed_section_numbers_read_stored_values_without_renumbering(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let number = {
        let source = String::from("= Manual\n:sectnums:\n\n== First\n");
        let parsed = parse(&source, &options)?;
        let Some(Block::Section(section)) = parsed.document_storage().blocks.first() else {
            return Err("expected section".into());
        };
        let mut section = section.clone();
        section.level = 3;
        section.kind = SectionKind::Appendix;
        assert_eq!(SectionRef::from(&section).number(), section.number());
        SectionRef::from(&section).number().map(str::to_owned)
    };
    assert_eq!(number.as_deref(), Some("1"));
    let section = Section::new(Title::default(), 1, Vec::new(), Location::default());
    assert_eq!(SectionRef::from(&section).number(), None);
    let section = section.with_numbering(true);
    assert_eq!(SectionRef::from(&section).number(), None);
    Ok(())
}

#[rstest::rstest]
fn typed_heading_ids_preserve_assigned_collisions_and_owner_edits(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let parsed = parse("= Manual\n\n== Same\n\n== Same\n", &options)?;
    let before = serde_json::to_value(parsed.document_storage())?;
    let mut ids = Vec::new();
    for block in &parsed.document_storage().blocks {
        let Block::Section(section) = block else {
            return Err("expected section".into());
        };
        let read = SectionRef::from(section);
        let id = read.resolve_id();
        assert!(matches!(id, std::borrow::Cow::Borrowed(_)));
        assert_eq!(id, section.id());
        assert_eq!(
            Some(id.as_ref()),
            read.id().map(crate::ast::ElementIdRef::as_str)
        );
        ids.push(id.into_owned());

        let mut edited = section.clone();
        edited.title = Title::default();
        assert_eq!(SectionRef::from(&edited).resolve_id(), section.id());
        edited.metadata.id = Some(crate::Anchor::new("", Location::default()));
        assert_eq!(SectionRef::from(&edited).resolve_id(), "");
    }
    let [first, second] = ids.as_slice() else {
        return Err("expected two section IDs".into());
    };
    assert_ne!(first, second);
    assert_eq!(serde_json::to_value(parsed.document_storage())?, before);
    Ok(())
}

#[rstest::rstest]
fn typed_heading_ids_keep_constructed_fallbacks_and_heading_precedence(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let parsed = parse("[discrete]\n== Heading\n", &options)?;
    let Some(Block::DiscreteHeading(original)) = parsed.document_storage().blocks.first() else {
        return Err("expected discrete heading".into());
    };
    for (text, expected) in [("", ""), ("!!!", ""), ("*Héading* _text_", "_héading_text")] {
        let inlines = (!text.is_empty())
            .then(|| crate::parse_inline(text, &options))
            .transpose()?;
        let title = Title::from(
            inlines
                .as_ref()
                .map(|parsed| parsed.inlines())
                .unwrap_or_default()
                .to_vec(),
        );
        for (primary, anchors, expected_id) in [
            (None, false, expected),
            (None, true, "last"),
            (Some("primary"), true, "primary"),
            (Some(""), true, ""),
        ] {
            let mut section = Section::new(title.clone(), 1, Vec::new(), Location::default());
            section.metadata.id = primary.map(|id| crate::Anchor::new(id, Location::default()));
            if anchors {
                section.metadata.anchors = vec![
                    crate::Anchor::new("first", Location::default()),
                    crate::Anchor::new("last", Location::default()),
                ];
            }
            let mut header = original.clone();
            header.title = title.clone();
            header.metadata = section.metadata.clone();
            let before = serde_json::to_value((&section, &header))?;
            let section_id = SectionRef::from(&section).resolve_id();
            let header_id = crate::ast::DiscreteHeadingRef::from(&header).resolve_id();
            assert_eq!(section_id, expected_id);
            assert_eq!(header_id, expected_id);
            assert_eq!(section_id, section.id());
            assert_eq!(
                header_id,
                Section::generate_id_string(&header.metadata, header.title.as_inlines())
            );
            assert_eq!(
                matches!(section_id, std::borrow::Cow::Borrowed(_)),
                primary.is_some() || anchors
            );
            assert_eq!(
                matches!(header_id, std::borrow::Cow::Borrowed(_)),
                primary.is_some() || anchors
            );
            assert_eq!(serde_json::to_value((&section, &header))?, before);
            if primary.is_none() && !anchors {
                assert!(SectionRef::from(&section).id().is_none());
                assert!(crate::ast::DiscreteHeadingRef::from(&header).id().is_none());
            }
        }
    }
    Ok(())
}
