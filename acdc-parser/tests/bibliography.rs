use acdc_parser::{Block, InlineNode, Options, SectionKind, WarningKind, parse};

type Error = Box<dyn std::error::Error>;

fn unordered_list<'block, 'source>(
    block: &'block Block<'source>,
) -> Result<&'block acdc_parser::UnorderedList<'source>, Error> {
    let Block::UnorderedList(list) = block else {
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
    let Block::Section(section) = parsed.document().blocks.first().ok_or("missing section")? else {
        return Err("expected a section".into());
    };

    assert_eq!(section.kind, SectionKind::Bibliography);
    let direct = unordered_list(section.content.first().ok_or("missing direct list")?)?;
    assert_eq!(direct.metadata.style, Some("bibliography"));
    let first_item = direct.items.first().ok_or("missing first list item")?;
    let nested = unordered_list(first_item.blocks.first().ok_or("missing nested list")?)?;
    assert_eq!(nested.metadata.style, None);
    let explicit = unordered_list(section.content.get(2).ok_or("missing explicit list")?)?;
    assert_eq!(explicit.metadata.style, Some("square"));
    let second = unordered_list(section.content.last().ok_or("missing second direct list")?)?;
    assert_eq!(second.metadata.style, Some("bibliography"));
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

    let Block::Section(bibliography) = parsed
        .document()
        .blocks
        .first()
        .ok_or("missing bibliography")?
    else {
        return Err("expected a bibliography section".into());
    };
    assert_eq!(bibliography.kind, SectionKind::Bibliography);
    assert_eq!(bibliography.content.len(), 2);
    let Block::Section(first) = bibliography.content.first().ok_or("missing first child")? else {
        return Err("expected first child section".into());
    };
    assert!(matches!(first.content.first(), Some(Block::Section(_))));
    assert!(matches!(
        bibliography.content.get(1),
        Some(Block::Section(_))
    ));
    assert!(matches!(
        parsed.document().blocks.get(1),
        Some(Block::Section(_))
    ));
    Ok(())
}

#[cfg(feature = "setext")]
#[test]
fn bibliography_sections_warn_for_setext_child_sections() -> Result<(), Error> {
    let parsed = parse(
        "[bibliography]\nReferences\n----------\n\nChild\n~~~~~\n",
        &Options::builder().with_setext().build(),
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
    let Block::Section(bibliography) = parsed
        .document()
        .blocks
        .first()
        .ok_or("missing bibliography")?
    else {
        return Err("expected a bibliography section".into());
    };
    assert!(matches!(
        bibliography.content.as_slice(),
        [Block::Section(_)]
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
    let list = unordered_list(parsed.document().blocks.last().ok_or("missing list")?)?;
    let Some(first_item) = list.items.first() else {
        return Err("missing bibliography item".into());
    };
    let [InlineNode::InlineAnchor(anchor), ..] = first_item.principal.as_slice() else {
        return Err(format!("expected a bibliography anchor: {:?}", first_item.principal).into());
    };

    assert!(anchor.is_bibliography());
    let reference = parsed
        .document()
        .references
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
    let list = unordered_list(parsed.document().blocks.first().ok_or("missing list")?)?;
    let [
        valid_item,
        _,
        escaped_item,
        ordinary_item,
        late_item,
        numeric_item,
        empty_item,
    ] = list.items.as_slice()
    else {
        return Err("expected seven bibliography list items".into());
    };

    let [InlineNode::InlineAnchor(valid), ..] = valid_item.principal.as_slice() else {
        return Err(format!("expected a bibliography anchor: {:?}", valid_item.principal).into());
    };
    assert_eq!(valid.id, "id.with.dot");
    assert!(valid.is_bibliography());

    let numbered_label = parsed
        .document()
        .references
        .get("numbered")
        .ok_or("missing numbered bibliography reference")?
        .xreflabel
        .as_ref()
        .ok_or("missing numbered reference label")?;
    let [
        InlineNode::PlainText(open),
        InlineNode::PlainText(number),
        InlineNode::PlainText(close),
    ] = numbered_label.as_slice()
    else {
        return Err("expected a three-part numbered reference label".into());
    };
    assert_eq!(open.content, "[");
    assert_eq!(number.content, "1");
    assert_eq!(close.content, "]");

    assert!(
        escaped_item
            .principal
            .iter()
            .all(|inline| !matches!(inline, InlineNode::InlineAnchor(_)))
    );
    assert!(!parsed.document().references.contains_key("escaped"));

    let [InlineNode::InlineAnchor(ordinary), ..] = ordinary_item.principal.as_slice() else {
        return Err("expected an ordinary anchor".into());
    };
    assert!(!ordinary.is_bibliography());

    assert!(late_item.principal.iter().any(
        |inline| matches!(inline, InlineNode::InlineAnchor(anchor) if anchor.id == "late" && !anchor.is_bibliography())
    ));
    assert!(
        numeric_item
            .principal
            .iter()
            .all(|inline| !matches!(inline, InlineNode::InlineAnchor(_)))
    );
    assert!(
        empty_item
            .principal
            .iter()
            .all(|inline| !matches!(inline, InlineNode::InlineAnchor(_)))
    );

    let reference = parsed
        .document()
        .references
        .get("id.with.dot")
        .ok_or("missing dotted bibliography reference")?;
    assert!(reference.is_bibliography());
    let label = reference.xreflabel.as_ref().ok_or("missing label")?;
    assert!(matches!(label.first(), Some(InlineNode::PlainText(text)) if text.content == "["));
    assert!(matches!(label.last(), Some(InlineNode::PlainText(text)) if text.content == "]"));
    assert!(!parsed.document().references.contains_key("9numeric"));
    Ok(())
}
