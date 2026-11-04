use acdc_parser::{
    CompatibilityMode, Options,
    ast::{
        AttributeValueRef, AttributesRef, BlockKind, BlockRef, ContentRef, FootnoteDefinitionRef,
        InlineKind, InlineRef, ReferenceRef, TocEntryRef,
    },
    parse,
};

type Error = Box<dyn std::error::Error>;

fn reftext(attributes: AttributesRef<'_>) -> Option<ContentRef<'_>> {
    if let Some(AttributeValueRef::Content(content)) = attributes.get("reftext") {
        Some(content)
    } else {
        None
    }
}

#[rstest::rstest]
fn named_reference_text_has_one_parsed_owner(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[id=target,reftext='*Label*']\nBody.\n\n<<target>>.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document_json = serde_json::to_value(parsed.document_ref())?;
    let json = document_json
        .pointer("/blocks/0/metadata")
        .ok_or("missing metadata")?;
    assert!(json.pointer("/attributes/reftext").is_none(), "{json}");
    assert_eq!(
        json.pointer("/reftext/0/name")
            .and_then(serde_json::Value::as_str),
        Some("span"),
        "{json}"
    );
    assert_eq!(
        json.pointer("/reftext/0/variant")
            .and_then(serde_json::Value::as_str),
        Some("strong"),
        "{json}"
    );
    Ok(())
}

#[rstest::rstest]
fn reference_label_nodes_point_to_the_authored_value(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "[id=target,reftext='λ *Label*']\nBody.",
        "[[target,λ *Label*]]\nBody.",
        "Before [[target,λ *Label*]] after.",
        "Before anchor:target[λ *Label*] after."
    )]
    source: &str,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let label = parsed
        .document_ref()
        .references()
        .get("target")
        .and_then(ReferenceRef::xreflabel)
        .ok_or("missing label")?;
    let bold = label
        .inlines()
        .find_map(|node| {
            if let InlineKind::Bold(bold) = node.kind() {
                Some(bold)
            } else {
                None
            }
        })
        .ok_or("missing bold text")?;
    assert_eq!(
        source.get(bold.location().absolute_start()..=bold.location().absolute_end()),
        Some("*Label*"),
        "{:?}",
        bold.location()
    );
    Ok(())
}

#[rstest::rstest]
fn draft_literal_reference_text_is_distinct_from_formatted_text(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[id=target,reftext=\"*Label*\"]\nBody.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let label = parsed
        .document_ref()
        .references()
        .get("target")
        .and_then(ReferenceRef::xreflabel)
        .ok_or("missing label")?;
    assert_eq!(
        matches!(
            label.inlines().next().map(InlineRef::kind),
            Some(InlineKind::Bold(_))
        ),
        mode == CompatibilityMode::Asciidoctor
    );
    if mode == CompatibilityMode::Draft {
        assert!(
            matches!(label.inlines().next().map(InlineRef::kind), Some(InlineKind::Raw(raw)) if raw.text() == "*Label*")
        );
    }
    Ok(())
}

#[rstest::rstest]
fn only_the_selected_draft_label_registers_content_before_the_body(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "[id=target,reftext='Discarded footnote:discarded[Discarded]']\n[reftext='Selected footnote:selected[Selected]']\nBody footnote:body[Body].\n\n<<target>>.\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let notes = parsed.document_ref().footnotes().collect::<Vec<_>>();
    let expected = if mode == CompatibilityMode::Draft {
        vec![Some("selected"), Some("body")]
    } else {
        vec![Some("body")]
    };
    assert_eq!(
        notes
            .iter()
            .copied()
            .map(FootnoteDefinitionRef::id)
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        notes
            .iter()
            .copied()
            .map(FootnoteDefinitionRef::number)
            .collect::<Vec<_>>(),
        if mode == CompatibilityMode::Draft {
            vec![1, 2]
        } else {
            vec![1]
        }
    );
    let metadata = parsed
        .document_ref()
        .blocks()
        .next()
        .and_then(BlockRef::attributes)
        .ok_or("missing metadata")?;
    assert_eq!(
        reftext(metadata),
        parsed
            .document_ref()
            .references()
            .get("target")
            .and_then(ReferenceRef::xreflabel)
    );
    Ok(())
}

#[rstest::rstest]
fn natural_reference_targets_keep_markup_and_the_first_id_owner(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "[id=target,reftext='Custom *Label*']\nFirst.\n\n[id=target,reftext='Ignored label']\nSecond.\n\n<<Custom *Label*>> and <<Ignored label>>.\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::Paragraph(last) = parsed
        .document_ref()
        .blocks()
        .last()
        .map(BlockRef::kind)
        .ok_or("missing paragraph")?
    else {
        return Err("expected paragraph".into());
    };
    let targets = last
        .content()
        .inlines()
        .filter_map(|node| {
            if let InlineKind::CrossReference(xref) = node.kind() {
                Some(xref.target())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(targets, ["target", "Ignored label"]);
    assert_eq!(
        parsed
            .warnings()
            .iter()
            .filter(|warning| matches!(warning.kind, acdc_parser::WarningKind::DuplicateId { .. }))
            .count(),
        1
    );
    Ok(())
}

#[rstest::rstest]
fn explicit_empty_reference_text_stays_present_but_uses_title_fallback(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[id=target,reftext=\"\"]\n== Title\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::Section(section) = parsed
        .document_ref()
        .blocks()
        .next()
        .map(BlockRef::kind)
        .ok_or("missing section")?
    else {
        return Err("expected section".into());
    };
    assert!(reftext(section.attributes()).is_some_and(ContentRef::is_empty));
    assert!(
        parsed
            .document_ref()
            .references()
            .get("target")
            .is_some_and(|reference| reference.xreflabel().is_none() && reference.title().is_some())
    );
    assert!(
        parsed
            .document_ref()
            .toc_entries()
            .next()
            .and_then(TocEntryRef::xreflabel)
            .is_some_and(ContentRef::is_empty)
    );
    Ok(())
}

#[rstest::rstest]
fn inline_entry_point_uses_the_same_parsed_label(
    #[values("[[target,λ *Label*]]", "anchor:target[λ *Label*]")] source: &str,
) -> Result<(), Error> {
    let parsed = acdc_parser::parse_inline(source, &Options::default())?;
    let InlineKind::Anchor(anchor) = parsed
        .content()
        .inlines()
        .next()
        .map(InlineRef::kind)
        .ok_or("missing anchor")?
    else {
        return Err("expected anchor".into());
    };
    let label = anchor.reftext().ok_or("missing label")?;
    let bold = label
        .inlines()
        .find_map(|node| {
            if let InlineKind::Bold(bold) = node.kind() {
                Some(bold)
            } else {
                None
            }
        })
        .ok_or("missing bold")?;
    assert_eq!(
        source.get(bold.location().absolute_start()..=bold.location().absolute_end()),
        Some("*Label*")
    );
    Ok(())
}

#[test]
fn reference_content_preserves_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<acdc_parser::Document<'_>>();
}

#[rstest::rstest]
fn labels_on_generated_sections_remain_in_native_json(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("", "Label")] value: &str,
) -> Result<(), Error> {
    let source = format!("[reftext='{value}']\n== Title\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let json = serde_json::to_value(parsed.document_ref())?;
    assert!(
        json.pointer("/blocks/0/metadata/reftext")
            .is_some_and(serde_json::Value::is_array),
        "{json}"
    );
    Ok(())
}

#[rstest::rstest]
fn expanded_reference_labels_map_to_the_full_authored_reference(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":value: *Éclair λ*\n\n[id=target,reftext='Prefix {value} suffix']\nBody.\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let label = parsed
        .document_ref()
        .references()
        .get("target")
        .and_then(ReferenceRef::xreflabel)
        .ok_or("missing label")?;
    let bold = label
        .inlines()
        .find_map(|node| {
            if let InlineKind::Bold(bold) = node.kind() {
                Some(bold)
            } else {
                None
            }
        })
        .ok_or("missing bold")?;
    for location in
        std::iter::once(bold.location()).chain(bold.content().inlines().map(InlineRef::location))
    {
        assert_eq!(
            source.get(location.absolute_start()..=location.absolute_end()),
            Some("{value}")
        );
    }
    Ok(())
}

#[rstest::rstest]
fn a_block_anchor_uses_its_declaration_snapshot(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":word: Before\n\n[role=example]\n:word: At anchor\n[[target,{word}]]\n:word: After anchor\nBody.\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let label = parsed
        .document_ref()
        .references()
        .get("target")
        .and_then(ReferenceRef::xreflabel)
        .ok_or("missing label")?;
    assert!(
        matches!(label.inlines().next().map(InlineRef::kind), Some(InlineKind::Plain(plain)) if plain.text() == "At anchor"),
        "{label:?}"
    );
    Ok(())
}

#[rstest::rstest]
fn nested_blocks_keep_reference_label_locations_and_natural_lookup(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("[cols=\"a\"]\n|===\n|\n", "====\n")] opening: &str,
) -> Result<(), Error> {
    let closing = if opening.starts_with('[') {
        "|==="
    } else {
        "===="
    };
    let source = format!(
        "Before.\n\n{opening}[[cell,Cell *label*]]\nBody.\n{closing}\n\n<<Cell *label*>>.\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let label = parsed
        .document_ref()
        .references()
        .get("cell")
        .and_then(ReferenceRef::xreflabel)
        .ok_or("missing label")?;
    let bold = label
        .inlines()
        .find_map(|node| {
            if let InlineKind::Bold(bold) = node.kind() {
                Some(bold)
            } else {
                None
            }
        })
        .ok_or("missing bold")?;
    assert_eq!(
        source.get(bold.location().absolute_start()..=bold.location().absolute_end()),
        Some("*label*")
    );
    let BlockKind::Paragraph(last) = parsed
        .document_ref()
        .blocks()
        .last()
        .map(BlockRef::kind)
        .ok_or("missing reference")?
    else {
        return Err("expected reference paragraph".into());
    };
    assert!(
        matches!(last.content().inlines().next().map(InlineRef::kind), Some(InlineKind::CrossReference(xref)) if xref.target() == "cell")
    );
    Ok(())
}

#[rstest::rstest]
fn included_reference_labels_keep_the_selected_file_and_line(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let include = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/tests/includes/compatibility_reference_locations.adoc");
    let source = format!(
        "Before.\n\ninclude::{}[tag=selected]\n\n<<included>>.\n",
        include.display()
    );
    let parsed = parse(
        &source,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .build()?,
    )?;
    let label = parsed
        .document_ref()
        .references()
        .get("included")
        .and_then(ReferenceRef::xreflabel)
        .ok_or("missing label")?;
    let bold = label
        .inlines()
        .find_map(|node| {
            if let InlineKind::Bold(bold) = node.kind() {
                Some(bold)
            } else {
                None
            }
        })
        .ok_or("missing bold")?;
    let location = parsed.resolve_location(bold.location());
    assert_eq!(location.file.as_deref(), Some(include.as_path()));
    assert_eq!(
        (location.location.start.line, location.location.end.line),
        (4, 4)
    );
    assert_eq!(
        (location.location.start.column, location.location.end.column),
        (32, 34)
    );
    let owner = parsed
        .document_ref()
        .blocks()
        .filter_map(BlockRef::attributes)
        .find_map(reftext)
        .ok_or("missing owner")?;
    assert_eq!(label, owner);
    Ok(())
}

#[rstest::rstest]
fn nested_inline_labels_follow_the_same_location_mapping_as_their_anchor(
    #[values(
        "Before *[[target,Label]]* after.",
        "[title='anchor:target[Label]']\nBody.",
        "Before ((anchor:target[Label])) after.",
        "pass:n[anchor:target[Label\\]]]",
        "footnote:[anchor:target[Label]]"
    )]
    source: &str,
) -> Result<(), Error> {
    let source = format!("Before.\n\n{source}");
    let parsed = parse(&source, &Options::default())?;
    let label = parsed
        .document_ref()
        .references()
        .get("target")
        .and_then(ReferenceRef::xreflabel)
        .ok_or("missing target label")?;
    let first = label.inlines().next().ok_or("missing label content")?;
    let location = first.location();
    assert_eq!(
        source.get(location.absolute_start()..=location.absolute_end()),
        Some("Label"),
        "{label:?}"
    );
    Ok(())
}
