use acdc_parser::{
    Block, InlineNode, Location, Options, WarningKind, parse, parse_file, parse_inline,
};

type Error = Box<dyn std::error::Error>;

fn assert_span(source: &str, location: &Location, expected: &str) -> Result<(), Error> {
    let start = source.find(expected).ok_or("expected source text")?;
    let end = start + expected.len() - 1;
    assert_eq!(
        (location.absolute_start, location.absolute_end),
        (start, end)
    );
    for (offset, position) in [(start, &location.start), (end, &location.end)] {
        let prefix = source.get(..offset).ok_or("invalid source boundary")?;
        assert_eq!(
            position.line as usize,
            prefix.bytes().filter(|&b| b == b'\n').count() + 1
        );
        assert_eq!(
            position.column as usize,
            prefix
                .rsplit('\n')
                .next()
                .unwrap_or_default()
                .chars()
                .count()
                + 1
        );
    }
    Ok(())
}

#[test]
fn formatted_inline_locations_map_anchor_catalogs_to_source() -> Result<(), Error> {
    for (open, close) in [
        ("*", "*"),
        ("**", "**"),
        ("_", "_"),
        ("__", "__"),
        ("`", "`"),
        ("``", "``"),
        ("#", "#"),
        ("##", "##"),
        ("~", "~"),
        ("^", "^"),
        ("\"`", "`\""),
        ("'`", "`'"),
        ("*_", "_*"),
    ] {
        for anchor in [
            "anchor:target[Label]",
            "[[target,Label]]",
            "anchor:{id}[Label]",
        ] {
            for newline in ["\n", "\r\n"] {
                let source = format!(
                    ":id: target{newline}:word: café{newline}{newline}Before.{newline}{newline}α {open}{{word}}{anchor}tail{close}.{newline}"
                );
                let parsed = parse(&source, &Options::default())?;
                let target = parsed
                    .document()
                    .references
                    .get("target")
                    .ok_or("missing target")?;
                assert_span(parsed.source(), &target.location, anchor)?;
            }
        }
    }
    Ok(())
}

#[test]
fn formatted_inline_locations_map_breaks_and_apostrophes() -> Result<(), Error> {
    for marker in ["*", "_", "`", "#"] {
        for newline in ["\n", "\r\n"] {
            let source =
                format!("Before.{newline}{newline}α {marker}Writer`'s +{newline}text{marker}.");
            let parsed = parse(&source, &Options::default())?;
            let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.get(1) else {
                return Err("expected paragraph".into());
            };
            let mut found = 0;
            for node in &paragraph.content {
                let children = if let InlineNode::BoldText(text) = node {
                    &text.content
                } else if let InlineNode::ItalicText(text) = node {
                    &text.content
                } else if let InlineNode::MonospaceText(text) = node {
                    &text.content
                } else if let InlineNode::HighlightText(text) = node {
                    &text.content
                } else {
                    continue;
                };
                for child in children {
                    if let InlineNode::StandaloneCurvedApostrophe(apostrophe) = child {
                        assert_span(parsed.source(), &apostrophe.location, "`'")?;
                        found += 1;
                    } else if let InlineNode::LineBreak(line_break) = child {
                        assert_span(parsed.source(), &line_break.location, " +")?;
                        found += 1;
                    }
                }
            }
            assert_eq!(found, 2, "{source:?}");
        }
    }
    Ok(())
}

#[test]
fn formatted_inline_locations_keep_generated_break_at_newline() -> Result<(), Error> {
    let source = "Before.\n\n[%hardbreaks]\nα *first\nsecond*.";
    let parsed = parse(source, &Options::default())?;
    let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.get(1) else {
        return Err("expected paragraph".into());
    };
    let bold = paragraph
        .content
        .iter()
        .find_map(|node| {
            if let InlineNode::BoldText(bold) = node {
                Some(bold)
            } else {
                None
            }
        })
        .ok_or("missing bold text")?;
    let line_break = bold
        .content
        .iter()
        .find_map(|node| {
            if let InlineNode::LineBreak(line_break) = node {
                Some(line_break)
            } else {
                None
            }
        })
        .ok_or("missing generated line break")?;
    let offset = source.find("\nsecond").ok_or("missing newline")?;
    assert_eq!(line_break.location.absolute_start, offset);
    assert_eq!(line_break.location.absolute_end, offset);
    assert_eq!(
        (
            line_break.location.start.line,
            line_break.location.start.column
        ),
        (4, 9)
    );
    assert_eq!(line_break.location.start, line_break.location.end);
    Ok(())
}

#[test]
fn formatted_inline_locations_preserve_duplicate_diagnostics() -> Result<(), Error> {
    let source = "Before.\n\n*anchor:target[First]One*.\n\n`anchor:target[Second]Two`.\n";
    let parsed = parse(source, &Options::default())?;
    let [warning] = parsed.warnings() else {
        return Err("expected one duplicate warning".into());
    };
    let WarningKind::DuplicateId { first, .. } = &warning.kind else {
        return Err("expected duplicate ID".into());
    };
    assert_span(source, &first.location, "anchor:target[First]")?;
    assert_span(
        source,
        &warning
            .source_location()
            .ok_or("missing warning location")?
            .location,
        "anchor:target[Second]",
    )?;
    Ok(())
}

#[test]
fn formatted_inline_locations_preserve_include_origins() -> Result<(), Error> {
    let parsed = parse_file(
        "fixtures/tests/formatted_inline_locations.adoc",
        &Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .build()?,
    )?;
    let target = parsed
        .document()
        .references
        .get("included")
        .ok_or("missing included anchor")?;
    let source = include_str!("../fixtures/tests/includes/formatted_inline_locations.adoc");
    assert_span(source, &target.location, "anchor:included[Included target]")?;
    assert!(
        target
            .location
            .start
            .file
            .as_ref()
            .and_then(|files| files.last())
            .is_some_and(|file| file.ends_with("includes/formatted_inline_locations.adoc"))
    );
    assert_eq!(target.location.start.file, target.location.end.file);
    let following = parsed
        .document()
        .references
        .get("following")
        .ok_or("missing following anchor")?;
    assert_span(
        include_str!("../fixtures/tests/formatted_inline_locations.adoc"),
        &following.location,
        "anchor:following[Following target]",
    )?;
    Ok(())
}

#[test]
fn formatted_inline_locations_map_standalone_inline_parses() -> Result<(), Error> {
    for anchor in ["anchor:target[]", "[[target]]"] {
        let source = format!("First line.\nα *Before {anchor} after*.");
        let parsed = parse_inline(&source, &Options::default())?;
        let bold = parsed
            .inlines()
            .iter()
            .find_map(|node| {
                if let InlineNode::BoldText(text) = node {
                    Some(text)
                } else {
                    None
                }
            })
            .ok_or("missing bold text")?;
        let target = bold
            .content
            .iter()
            .find_map(|node| {
                if let InlineNode::InlineAnchor(anchor) = node {
                    Some(anchor)
                } else {
                    None
                }
            })
            .ok_or("missing anchor")?;
        assert_span(&source, &target.location, anchor)?;
    }
    Ok(())
}
