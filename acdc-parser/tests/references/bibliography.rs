use acdc_parser::{
    Options, SafeMode, SectionKind, WarningKind,
    ast::{
        BlockKind, BlockRef, ContentRef, InlineKind, InlineRef, ReferenceTargetRef,
        UnorderedListRef,
    },
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn bibliography_citation_keeps_attribute_escape() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:value: expanded\n\n[bibliography]\n* [[[ref,\\{value}]]] Entry.\n",
        &Options::default(),
    )?;
    let label = parsed
        .document()
        .references()
        .get("ref")
        .and_then(ReferenceTargetRef::xreflabel)
        .ok_or("missing label")?;
    let text: String = label
        .inlines()
        .filter_map(|node| {
            if let InlineKind::Plain(plain) = node.kind() {
                Some(plain.text())
            } else if let InlineKind::Raw(raw) = node.kind() {
                Some(raw.text())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(text, "[\\{value}]", "{label:?}");
    Ok(())
}

#[test]
fn bibliography_reference_warnings_retain_source_locations() -> Result<(), Error> {
    let source = "= T\n\nBefore.\n\n[bibliography]\n* [[[ref,<<missing>> +]]] Entry.\n";
    let parsed = parse(source, &Options::default())?;
    let warnings: Vec<_> = parsed
        .warnings()
        .iter()
        .filter(|warning| {
            warning
                .kind
                .to_string()
                .contains("invalid reference: missing")
        })
        .collect();
    assert_eq!(warnings.len(), 1, "{:?}", parsed.warnings());
    let location = &warnings
        .first()
        .ok_or("missing warning")?
        .source_location()
        .ok_or("missing warning source")?
        .location;
    assert_eq!(location.start.line, 6);
    Ok(())
}

#[test]
fn bibliography_labels_keep_parent_safe_mode_and_original_citation_text() -> Result<(), Error> {
    let source = "[bibliography]\n* [[[ref,pass:a[{safe-mode-name}]]]] Entry.\n";
    for mode in [
        SafeMode::Unsafe,
        SafeMode::Safe,
        SafeMode::Server,
        SafeMode::Secure,
    ] {
        let options = Options::builder().with_safe_mode(mode).build()?;
        let parsed = parse(source, &options)?;
        let list = unordered_list(parsed.document().blocks().next().ok_or("missing list")?)?;
        let [InlineKind::Anchor(anchor), ..] = &*list
            .items()
            .next()
            .ok_or("missing list item")?
            .principal()
            .inlines()
            .map(InlineRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("missing bibliography anchor".into());
        };
        assert_eq!(
            label_text(anchor.reftext()),
            Some("pass:a[{safe-mode-name}".to_owned())
        );
        let label = anchor.bibliography_label().ok_or("missing entry content")?;
        assert!(
            matches!(&*label.inlines().map(InlineRef::kind).collect::<Vec<_>>(), [InlineKind::Raw(raw)] if raw.text() == mode.name()),
            "{mode:?}: {label:?}"
        );
        assert!(label.inlines().all(|inline| {
            let location = inline.location();
            location.absolute_start() >= source.find("pass:").unwrap_or_default()
                && location.absolute_end() < source.len()
        }));
    }
    Ok(())
}

#[test]
fn bibliography_attribute_values_do_not_become_new_passthroughs() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:raw: +++<mark>raw</mark>+++\n\n[bibliography]\n* [[[ref,{raw}]]] Entry.\n",
        &Options::default(),
    )?;
    let list = unordered_list(parsed.document().blocks().next().ok_or("missing list")?)?;
    let [InlineKind::Anchor(anchor), ..] = &*list
        .items()
        .next()
        .ok_or("missing list item")?
        .principal()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("missing bibliography anchor".into());
    };
    assert_eq!(label_text(anchor.reftext()), Some("{raw}".to_owned()));
    let label = anchor.bibliography_label().ok_or("missing entry content")?;
    let mut text = String::new();
    for node in label.inlines() {
        if let InlineKind::Plain(plain) = node.kind() {
            text.push_str(plain.text());
        } else if let InlineKind::Raw(raw) = node.kind() {
            assert_eq!(
                raw.substitutions().collect::<Vec<_>>(),
                [acdc_parser::Substitution::SpecialChars]
            );
            text.push_str(raw.text());
        } else {
            return Err("bibliography text became markup".into());
        }
    }
    assert_eq!(text, "+++<mark>raw</mark>+++");
    Ok(())
}

fn unordered_list(block: BlockRef<'_>) -> Result<UnorderedListRef<'_>, Error> {
    let BlockKind::UnorderedList(list) = block.kind() else {
        return Err("expected an unordered list".into());
    };
    Ok(list)
}

#[test]
fn bibliography_sections_promote_only_direct_unstyled_lists() -> Result<(), Error> {
    let parsed = parse(
        "[bibliography]\n== References\n\n* Direct\n** Nested\n\nBetween lists.\n\n[square]\n* Explicit\n\nAnother separator.\n\n* Second direct\n",
        &Options::default(),
    )?;
    let BlockKind::Section(section) = parsed
        .document()
        .blocks()
        .next()
        .ok_or("missing section")?
        .kind()
    else {
        return Err("expected a section".into());
    };

    assert_eq!(section.kind(), SectionKind::Bibliography);
    let direct = unordered_list(section.blocks().next().ok_or("missing direct list")?)?;
    assert_eq!(direct.style(), Some("bibliography"));
    let first_item = direct.items().next().ok_or("missing first list item")?;
    let nested = unordered_list(first_item.blocks().next().ok_or("missing nested list")?)?;
    assert_eq!(nested.style(), None);
    let explicit = unordered_list(section.blocks().nth(2).ok_or("missing explicit list")?)?;
    assert_eq!(explicit.style(), Some("square"));
    let second = unordered_list(
        section
            .blocks()
            .last()
            .ok_or("missing second direct list")?,
    )?;
    assert_eq!(second.style(), Some("bibliography"));
    Ok(())
}

#[test]
fn bibliography_sections_warn_for_each_direct_child_section() -> Result<(), Error> {
    let parsed = parse(
        "= Document\n\n[bibliography]\n== References\n\n[#first]\n=== First\n\n==== Grandchild\n\nText.\n\n=== Second\n\nText.\n\n== Following\n\n=== Allowed child\n",
        &Options::default(),
    )?;
    let warnings = parsed
        .warnings()
        .iter()
        .filter(|warning| matches!(warning.kind, WarningKind::NestedSectionInBibliography))
        .collect::<Vec<_>>();
    let [first_warning, second_warning] = warnings.as_slice() else {
        return Err(format!(
            "expected two nested bibliography warnings, got {}",
            warnings.len()
        )
        .into());
    };

    assert_eq!(
        first_warning
            .source_location()
            .ok_or("missing location")?
            .location
            .start
            .line,
        7
    );
    assert_eq!(
        second_warning
            .source_location()
            .ok_or("missing location")?
            .location
            .start
            .line,
        13
    );
    assert_eq!(
        first_warning.kind.to_string(),
        "bibliography sections do not support nested sections"
    );

    let BlockKind::Section(bibliography) = parsed
        .document()
        .blocks()
        .next()
        .ok_or("missing bibliography")?
        .kind()
    else {
        return Err("expected a bibliography section".into());
    };
    assert_eq!(bibliography.kind(), SectionKind::Bibliography);
    assert_eq!(bibliography.blocks().count(), 2);
    let BlockKind::Section(first) = bibliography
        .blocks()
        .next()
        .ok_or("missing first child")?
        .kind()
    else {
        return Err("expected first child section".into());
    };
    assert!(matches!(
        first.blocks().next().map(BlockRef::kind),
        Some(BlockKind::Section(_))
    ));
    assert!(matches!(
        bibliography.blocks().nth(1).map(BlockRef::kind),
        Some(BlockKind::Section(_))
    ));
    assert!(matches!(
        parsed.document().blocks().nth(1).map(BlockRef::kind),
        Some(BlockKind::Section(_))
    ));
    Ok(())
}

#[test]
fn bibliography_sections_warn_for_setext_child_sections() -> Result<(), Error> {
    let parsed = parse(
        "[bibliography]\nReferences\n----------\n\nChild\n~~~~~\n",
        &Options::builder().build()?,
    )?;
    let warning = parsed
        .warnings()
        .iter()
        .find(|warning| matches!(warning.kind, WarningKind::NestedSectionInBibliography))
        .ok_or("missing nested bibliography warning")?;

    assert_eq!(
        warning
            .source_location()
            .ok_or("missing location")?
            .location
            .start
            .line,
        5
    );
    let BlockKind::Section(bibliography) = parsed
        .document()
        .blocks()
        .next()
        .ok_or("missing bibliography")?
        .kind()
    else {
        return Err("expected a bibliography section".into());
    };
    assert!(matches!(
        &*bibliography
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>(),
        [BlockKind::Section(_)]
    ));
    Ok(())
}

#[test]
fn bibliography_sections_allow_headings_inside_compound_blocks() -> Result<(), Error> {
    let parsed = parse(
        "[bibliography]\n== References\n\n--\n=== Heading in open block\n\nText.\n--\n",
        &Options::default(),
    )?;

    assert!(
        !parsed
            .warnings()
            .iter()
            .any(|warning| matches!(warning.kind, WarningKind::NestedSectionInBibliography))
    );
    Ok(())
}

#[test]
fn explicit_bibliography_lists_work_outside_bibliography_sections() -> Result<(), Error> {
    let parsed = parse(
        "See <<ref>>.\n\n[bibliography]\n* [[[ref]]] Reference\n",
        &Options::default(),
    )?;
    let list = unordered_list(parsed.document().blocks().last().ok_or("missing list")?)?;
    let Some(first_item) = list.items().next() else {
        return Err("missing bibliography item".into());
    };
    let [InlineKind::Anchor(anchor), ..] = &*kinds(first_item.principal()) else {
        return Err(format!(
            "expected a bibliography anchor: {:?}",
            first_item.principal()
        )
        .into());
    };

    assert!(anchor.is_bibliography());
    let reference = parsed
        .document()
        .references()
        .get("ref")
        .ok_or("missing bibliography reference")?;
    assert!(reference.is_bibliography());
    assert!(reference.has_automatic_citation());
    Ok(())
}

#[test]
fn bibliography_anchors_require_valid_leading_triple_syntax() -> Result<(), Error> {
    let parsed = parse(
        "[bibliography]\n* [[[id.with.dot,Short label]]] Valid\n* [[[numbered,1]]] Numbered label\n* [\\[[escaped]]] Escaped\n* [[ordinary]] Ordinary\n* Before [[[late]]] Late\n* [[[9numeric]]] Numeric ID\n* [[[]]] Empty\n",
        &Options::default(),
    )?;
    let list = unordered_list(parsed.document().blocks().next().ok_or("missing list")?)?;
    let [
        valid_item,
        _,
        escaped_item,
        ordinary_item,
        late_item,
        numeric_item,
        empty_item,
    ] = &*list.items().collect::<Vec<_>>()
    else {
        return Err("expected seven bibliography list items".into());
    };

    let [InlineKind::Anchor(valid), ..] = &*kinds(valid_item.principal()) else {
        return Err(format!(
            "expected a bibliography anchor: {:?}",
            valid_item.principal()
        )
        .into());
    };
    assert_eq!(valid.id(), "id.with.dot");
    assert!(valid.is_bibliography());

    let numbered_label = parsed
        .document()
        .references()
        .get("numbered")
        .ok_or("missing numbered bibliography reference")?
        .xreflabel()
        .ok_or("missing numbered reference label")?;
    let [
        InlineKind::Plain(open),
        InlineKind::Plain(number),
        InlineKind::Plain(close),
    ] = &*numbered_label
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected a three-part numbered reference label".into());
    };
    assert_eq!(open.text(), "[");
    assert_eq!(number.text(), "1");
    assert_eq!(close.text(), "]");

    assert!(
        escaped_item
            .principal()
            .inlines()
            .all(|inline| !matches!(inline.kind(), InlineKind::Anchor(_)))
    );
    assert!(!parsed.document().references().contains("escaped"));

    let [InlineKind::Anchor(ordinary), ..] = &*kinds(ordinary_item.principal()) else {
        return Err("expected an ordinary anchor".into());
    };
    assert!(!ordinary.is_bibliography());

    assert!(late_item.principal().inlines().any(
        |inline| matches!(inline.kind(), InlineKind::Anchor(anchor) if anchor.id() == "late" && !anchor.is_bibliography())
    ));
    assert!(
        numeric_item
            .principal()
            .inlines()
            .all(|inline| !matches!(inline.kind(), InlineKind::Anchor(_)))
    );
    assert!(
        empty_item
            .principal()
            .inlines()
            .all(|inline| !matches!(inline.kind(), InlineKind::Anchor(_)))
    );

    let reference = parsed
        .document()
        .references()
        .get("id.with.dot")
        .ok_or("missing dotted bibliography reference")?;
    assert!(reference.is_bibliography());
    let label = reference.xreflabel().ok_or("missing label")?;
    assert!(
        matches!(label.inlines().next().map(InlineRef::kind), Some(InlineKind::Plain(text)) if text.text() == "[")
    );
    assert!(
        matches!(label.inlines().next_back().map(InlineRef::kind), Some(InlineKind::Plain(text)) if text.text() == "]")
    );
    assert!(!parsed.document().references().contains("9numeric"));
    Ok(())
}

fn kinds(content: ContentRef<'_>) -> Vec<InlineKind<'_>> {
    content.inlines().map(InlineRef::kind).collect()
}

fn label_text(label: Option<ContentRef<'_>>) -> Option<String> {
    label.map(|label| {
        label
            .inlines()
            .filter_map(|node| {
                if let InlineKind::Plain(plain) = node.kind() {
                    Some(plain.text())
                } else if let InlineKind::Raw(raw) = node.kind() {
                    Some(raw.text())
                } else {
                    None
                }
            })
            .collect()
    })
}
