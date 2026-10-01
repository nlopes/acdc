use acdc_parser::{Block, InlineNode, Options, Substitution, parse};

type Error = Box<dyn std::error::Error>;

// Fixture JSON omits raw substitution flags; check them alongside source spans.
#[test]
fn passthrough_character_references_preserve_complete_source_spans() -> Result<(), Error> {
    for (source, token, expected, escaped) in [
        ("é pass:c,r[&#169;] Ω", "&#169;", "&#169;", false),
        (r"é pass:c,r[\&#169;] Ω", r"\&#169;", "&#169;", true),
        ("é pass:c,r[&#9;] Ω", "&#9;", "&#9;", true),
        ("é pass:r[&amp;#169;] Ω", "&amp;#169;", "&#169;", false),
        ("é pass:c,r[&lt;] Ω", "&lt;", "<", true),
        ("é pass:r,c[&lt;] Ω", "&lt;", "&lt;", true),
    ] {
        let parsed = parse(source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected one paragraph".into());
        };
        let raw = paragraph
            .content
            .iter()
            .find_map(|node| {
                if let InlineNode::RawText(raw) = node
                    && raw.content == expected
                {
                    Some(raw)
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("missing expected fragment in {source:?}"))?;
        assert_eq!(
            raw.subs.contains(&Substitution::SpecialChars),
            escaped,
            "{source}"
        );
        assert_eq!(
            &source[raw.location.absolute_start..=raw.location.absolute_end],
            token,
            "{source}"
        );
        assert_eq!(raw.location.start.line, 1);
        assert_eq!(raw.location.end.line, 1);
        assert_eq!(
            raw.location.start.column,
            u32::try_from(source[..raw.location.absolute_start].chars().count() + 1)?,
        );
        assert_eq!(
            raw.location.end.column,
            u32::try_from(source[..=raw.location.absolute_end].chars().count())?,
        );
    }
    Ok(())
}

#[test]
fn passthrough_character_references_map_generated_values_to_the_reference() -> Result<(), Error> {
    let source = "é pass:a,r[{value}] Ω";
    let options = Options::with_attributes([("value", "&#169;&#174;")])?;
    let parsed = parse(source, &options)?;
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected one paragraph".into());
    };
    let fragments: Vec<_> = paragraph
        .content
        .iter()
        .filter_map(|node| {
            if let InlineNode::RawText(raw) = node {
                Some(raw)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(fragments.len(), 2);
    for raw in fragments {
        assert!(raw.subs.is_empty());
        assert_eq!(
            &source[raw.location.absolute_start..=raw.location.absolute_end],
            "{value}"
        );
    }
    Ok(())
}
