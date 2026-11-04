use acdc_parser::{
    Options, WarningKind,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef, LocationRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn source_span<'s, 'doc>(
    source: &'s str,
    location: impl Into<LocationRef<'doc>>,
) -> Result<&'s str, Error> {
    let location = location.into();
    let end_char = source
        .get(location.absolute_end()..)
        .and_then(|tail| tail.chars().next())
        .ok_or("invalid inclusive end")?;
    for (offset, position) in [
        (location.absolute_start(), location.start()),
        (location.absolute_end(), location.end()),
    ] {
        let prefix = source.get(..offset).ok_or("invalid UTF-8 boundary")?;
        assert_eq!(
            position.line() as usize,
            prefix.bytes().filter(|&b| b == b'\n').count() + 1
        );
        assert_eq!(
            position.column() as usize,
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
        .get(location.absolute_start()..location.absolute_end() + end_char.len_utf8())
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
        let [BlockKind::Paragraph(paragraph), _] = &*parsed
            .document_ref()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected paragraph".into());
        };
        for node in paragraph.content().inlines() {
            if let InlineKind::Plain(text) = node.kind() {
                assert_eq!(source_span(&source, text.location())?, text.text());
            }
        }
        let link = paragraph
            .content()
            .inlines()
            .find_map(|node| {
                if let InlineKind::Link(link) = node.kind() {
                    Some(link)
                } else {
                    None
                }
            })
            .ok_or("missing nested link")?;
        assert_eq!(
            source_span(&source, link.location())?,
            r"link:https://example.org[é\]"
        );
        let [InlineKind::Raw(label)] = &*link
            .text()
            .inlines()
            .map(InlineRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected link label".into());
        };
        assert_eq!(label.text(), "é");
        assert_eq!(source_span(&source, label.location())?, "é");
        let target = parsed
            .document_ref()
            .references()
            .get("target")
            .ok_or("missing anchor")?;
        assert_eq!(
            source_span(&source, target.location())?,
            r"anchor:target[Target\]"
        );
        let tail = paragraph
            .content()
            .inlines()
            .find_map(|node| {
                if let InlineKind::Raw(raw) = node.kind()
                    && raw.text() == "tail"
                {
                    Some(raw)
                } else {
                    None
                }
            })
            .ok_or("missing passthrough tail")?;
        assert_eq!(source_span(&source, tail.location())?, "tail");
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
        let [note] = &*parsed.document_ref().footnotes().collect::<Vec<_>>() else {
            return Err("expected one footnote".into());
        };
        assert_eq!(source_span(&source, note.location())?, r"footnote:note[é\]");
        let [InlineKind::Raw(body)] = &*note
            .content()
            .inlines()
            .map(InlineRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected footnote text".into());
        };
        assert_eq!(body.text(), "é");
        assert_eq!(source_span(&source, body.location())?, "é");
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
    let [BlockKind::Paragraph(paragraph)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected paragraph".into());
    };
    let term = paragraph
        .content()
        .inlines()
        .find_map(|node| {
            if let InlineKind::IndexTerm(term) = node.kind() {
                Some(term)
            } else {
                None
            }
        })
        .ok_or("missing index term")?;
    let [InlineKind::Raw(label)] = &*term
        .term()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected index label".into());
    };
    assert_eq!(label.text(), "Expanded");
    assert_eq!(source_span(source, label.location())?, "{name}");
    Ok(())
}
