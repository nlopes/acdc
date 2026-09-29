use acdc_parser::{Block, InlineMacro, InlineNode, Location, Options, WarningKind, parse};

type Error = Box<dyn std::error::Error>;

fn source_span<'s>(source: &'s str, location: &Location) -> Result<&'s str, Error> {
    let end_char = source
        .get(location.absolute_end..)
        .and_then(|tail| tail.chars().next())
        .ok_or("invalid inclusive end")?;
    for (offset, position) in [
        (location.absolute_start, &location.start),
        (location.absolute_end, &location.end),
    ] {
        let prefix = source.get(..offset).ok_or("invalid UTF-8 boundary")?;
        assert_eq!(
            position.line as usize,
            prefix.bytes().filter(|&b| b == b'\n').count() + 1
        );
        assert_eq!(
            position.column as usize,
            prefix
                .rsplit('\n')
                .next()
                .ok_or("missing line")?
                .chars()
                .count()
                + 1
        );
    }
    source
        .get(location.absolute_start..location.absolute_end + end_char.len_utf8())
        .ok_or_else(|| "invalid source span".into())
}

#[test]
fn passthrough_brackets_preserve_nested_macro_locations() -> Result<(), Error> {
    for separator in [" / ", "\n"] {
        let source = format!(
            "α pass:m[link:https://example.org[é\\]{separator}anchor:target[Target\\]tail] ω.\n\n<<target>>.\n"
        );
        let parsed = parse(&source, &Options::default())?;
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
        let [Block::Paragraph(paragraph), _] = parsed.document().blocks.as_slice() else {
            return Err("expected paragraph".into());
        };
        for node in &paragraph.content {
            if let InlineNode::PlainText(text) = node {
                assert_eq!(source_span(&source, &text.location)?, text.content);
            }
        }
        let link = paragraph
            .content
            .iter()
            .find_map(|node| {
                if let InlineNode::Macro(InlineMacro::Link(link)) = node {
                    Some(link)
                } else {
                    None
                }
            })
            .ok_or("missing nested link")?;
        assert_eq!(
            source_span(&source, &link.location)?,
            r"link:https://example.org[é\]"
        );
        let [InlineNode::RawText(label)] = link.text.as_slice() else {
            return Err("expected link label".into());
        };
        assert_eq!(label.content, "é");
        assert_eq!(source_span(&source, &label.location)?, "é");
        let target = parsed
            .document()
            .references
            .get("target")
            .ok_or("missing anchor")?;
        assert_eq!(
            source_span(&source, &target.location)?,
            r"anchor:target[Target\]"
        );
        let tail = paragraph
            .content
            .iter()
            .find_map(|node| {
                if let InlineNode::RawText(raw) = node
                    && raw.content == "tail"
                {
                    Some(raw)
                } else {
                    None
                }
            })
            .ok_or("missing passthrough tail")?;
        assert_eq!(source_span(&source, &tail.location)?, "tail");
    }
    Ok(())
}

#[test]
fn passthrough_brackets_register_footnotes_once() -> Result<(), Error> {
    for wrapper in ["{}", "*{}*", "link:https://example.org[Before {} after]"] {
        let wrapped = wrapper.replace("{}", r"pass:m[footnote:note[é\] tail]");
        let source = format!("α {wrapped} ω.\n\nReuse footnote:note[].\n");
        let parsed = parse(&source, &Options::default())?;
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
        let [note] = parsed.document().footnotes.as_slice() else {
            return Err("expected one footnote".into());
        };
        assert_eq!(source_span(&source, &note.location)?, r"footnote:note[é\]");
        let [InlineNode::RawText(body)] = note.content.as_slice() else {
            return Err("expected footnote text".into());
        };
        assert_eq!(body.content, "é");
        assert_eq!(source_span(&source, &body.location)?, "é");
    }
    Ok(())
}

#[test]
fn passthrough_brackets_report_conflicting_footnote_source_spans() -> Result<(), Error> {
    let source = "α pass:m[footnote:note[First\\]] ω.\n\nβ pass:m[footnote:note[Second\\]] γ.\n";
    let parsed = parse(source, &Options::default())?;
    let [warning] = parsed.warnings() else {
        return Err("expected one conflict warning".into());
    };
    let WarningKind::ConflictingFootnote { id, first } = &warning.kind else {
        return Err(format!("unexpected warning: {warning}").into());
    };
    assert_eq!(id, "note");
    assert_eq!(
        source_span(source, &first.location)?,
        r"footnote:note[First\]"
    );
    let duplicate = &warning
        .source_location()
        .ok_or("missing conflict location")?
        .location;
    assert_eq!(source_span(source, duplicate)?, r"footnote:note[Second\]");
    Ok(())
}

#[test]
fn passthrough_brackets_keep_later_attribute_source_spans() -> Result<(), Error> {
    let source = ":name: Expanded\n\nα pass:a,m[one\\]two (({name}))] ω.\n";
    let parsed = parse(source, &Options::default())?;
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected paragraph".into());
    };
    let term = paragraph
        .content
        .iter()
        .find_map(|node| {
            if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
                Some(term)
            } else {
                None
            }
        })
        .ok_or("missing index term")?;
    let [InlineNode::RawText(label)] = term.term() else {
        return Err("expected index label".into());
    };
    assert_eq!(label.content, "Expanded");
    assert_eq!(source_span(source, &label.location)?, "{name}");
    Ok(())
}
