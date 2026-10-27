use acdc_parser::{Block, CompatibilityMode, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn named_reference_text_has_one_parsed_owner(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[id=target,reftext='*Label*']\nBody.\n\n<<target>>.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let metadata = parsed
        .document()
        .blocks
        .first()
        .and_then(Block::metadata)
        .ok_or("missing metadata")?;
    let json = serde_json::to_value(metadata)?;
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
        .document()
        .references
        .get("target")
        .and_then(|reference| reference.xreflabel.as_ref())
        .ok_or("missing label")?;
    let bold = label
        .iter()
        .find_map(|node| {
            if let InlineNode::BoldText(bold) = node {
                Some(bold)
            } else {
                None
            }
        })
        .ok_or("missing bold text")?;
    assert_eq!(
        source.get(bold.location.absolute_start..=bold.location.absolute_end),
        Some("*Label*"),
        "{:?}",
        bold.location
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
        .document()
        .references
        .get("target")
        .and_then(|reference| reference.xreflabel.as_ref())
        .ok_or("missing label")?;
    assert_eq!(
        matches!(label.first(), Some(InlineNode::BoldText(_))),
        mode == CompatibilityMode::Asciidoctor
    );
    if mode == CompatibilityMode::Draft {
        assert!(
            matches!(label.first(), Some(InlineNode::RawText(raw)) if raw.content == "*Label*")
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
    let notes = &parsed.document().footnotes;
    let expected = if mode == CompatibilityMode::Draft {
        vec![Some("selected"), Some("body")]
    } else {
        vec![Some("body")]
    };
    assert_eq!(
        notes.iter().map(|note| note.id).collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        notes.iter().map(|note| note.number).collect::<Vec<_>>(),
        if mode == CompatibilityMode::Draft {
            vec![1, 2]
        } else {
            vec![1]
        }
    );
    let metadata = parsed
        .document()
        .blocks
        .first()
        .and_then(Block::metadata)
        .ok_or("missing metadata")?;
    assert_eq!(
        metadata.reftext.as_ref(),
        parsed
            .document()
            .references
            .get("target")
            .and_then(|reference| reference.xreflabel.as_ref())
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
    let Block::Paragraph(last) = parsed.document().blocks.last().ok_or("missing paragraph")? else {
        return Err("expected paragraph".into());
    };
    let targets = last
        .content
        .iter()
        .filter_map(|node| {
            if let InlineNode::Macro(acdc_parser::InlineMacro::CrossReference(xref)) = node {
                Some(xref.target)
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
    let Block::Section(section) = parsed.document().blocks.first().ok_or("missing section")? else {
        return Err("expected section".into());
    };
    assert!(
        section
            .metadata
            .reftext
            .as_ref()
            .is_some_and(|label| label.is_empty())
    );
    assert!(
        parsed
            .document()
            .references
            .get("target")
            .is_some_and(|reference| reference.xreflabel.is_none() && reference.title.is_some())
    );
    assert!(
        parsed
            .document()
            .toc_entries
            .first()
            .and_then(|entry| entry.xreflabel.as_ref())
            .is_some_and(|label| label.is_empty())
    );
    Ok(())
}

#[rstest::rstest]
fn inline_entry_point_uses_the_same_parsed_label(
    #[values("[[target,λ *Label*]]", "anchor:target[λ *Label*]")] source: &str,
) -> Result<(), Error> {
    let parsed = acdc_parser::parse_inline(source, &Options::default())?;
    let InlineNode::InlineAnchor(anchor) = parsed.inlines().first().ok_or("missing anchor")? else {
        return Err("expected anchor".into());
    };
    let label = anchor.xreflabel.as_ref().ok_or("missing label")?;
    let bold = label
        .iter()
        .find_map(|node| {
            if let InlineNode::BoldText(bold) = node {
                Some(bold)
            } else {
                None
            }
        })
        .ok_or("missing bold")?;
    assert_eq!(
        source.get(bold.location.absolute_start..=bold.location.absolute_end),
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
    let json = serde_json::to_value(parsed.document())?;
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
        .document()
        .references
        .get("target")
        .and_then(|reference| reference.xreflabel.as_ref())
        .ok_or("missing label")?;
    let bold = label
        .iter()
        .find_map(|node| {
            if let InlineNode::BoldText(bold) = node {
                Some(bold)
            } else {
                None
            }
        })
        .ok_or("missing bold")?;
    for location in
        std::iter::once(&bold.location).chain(bold.content.iter().map(InlineNode::location))
    {
        assert_eq!(
            source.get(location.absolute_start..=location.absolute_end),
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
        .document()
        .references
        .get("target")
        .and_then(|reference| reference.xreflabel.as_ref())
        .ok_or("missing label")?;
    assert!(
        matches!(label.first(), Some(InlineNode::PlainText(plain)) if plain.content == "At anchor"),
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
        .document()
        .references
        .get("cell")
        .and_then(|reference| reference.xreflabel.as_ref())
        .ok_or("missing label")?;
    let bold = label
        .iter()
        .find_map(|node| {
            if let InlineNode::BoldText(bold) = node {
                Some(bold)
            } else {
                None
            }
        })
        .ok_or("missing bold")?;
    assert_eq!(
        source.get(bold.location.absolute_start..=bold.location.absolute_end),
        Some("*label*")
    );
    let Block::Paragraph(last) = parsed.document().blocks.last().ok_or("missing reference")? else {
        return Err("expected reference paragraph".into());
    };
    assert!(
        matches!(last.content.first(), Some(InlineNode::Macro(acdc_parser::InlineMacro::CrossReference(xref))) if xref.target == "cell")
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
        .document()
        .references
        .get("included")
        .and_then(|reference| reference.xreflabel.as_ref())
        .ok_or("missing label")?;
    let bold = label
        .iter()
        .find_map(|node| {
            if let InlineNode::BoldText(bold) = node {
                Some(bold)
            } else {
                None
            }
        })
        .ok_or("missing bold")?;
    let location = parsed.source_location(&bold.location);
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
        .document()
        .blocks
        .iter()
        .filter_map(Block::metadata)
        .find_map(|metadata| metadata.reftext.as_ref())
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
        .document()
        .references
        .get("target")
        .and_then(|reference| reference.xreflabel.as_ref())
        .ok_or("missing target label")?;
    let first = label.first().ok_or("missing label content")?;
    let location = first.location();
    assert_eq!(
        source.get(location.absolute_start..=location.absolute_end),
        Some("Label"),
        "{label:?}"
    );
    Ok(())
}
