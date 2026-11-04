use acdc_parser::{
    Options, Substitution,
    ast::{BlockKind, BlockRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

// Fixture JSON omits raw substitution flags; check them alongside source spans.
#[test]
fn passthrough_character_references_preserve_complete_source_spans() -> Result<(), Error> {
    for (source, token, expected, escaped) in [
        ("é pass:c,r[&#169;] Ω", "&#169;", "&#169;", false),
        (r"é pass:c,r[\&#169;] Ω", r"\&#169;", "&#169;", true),
        ("é pass:c,r[&#9;] Ω", "&#9;", "&#9;", true),
        ("é pass:r[&amp;#169;] Ω", "&amp;#169;", "&#169;", false),
        ("é pass:c,r[&lt;] Ω", "&lt;", "&lt;", false),
        ("é pass:r,c[&lt;] Ω", "&lt;", "&lt;", true),
    ] {
        let parsed = parse(source, &Options::default())?;
        let [BlockKind::Paragraph(paragraph)] = &*parsed
            .document_ref()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected one paragraph".into());
        };
        let (location, actual_escaping) = paragraph
            .content()
            .inlines()
            .find_map(|node| {
                if let InlineKind::Raw(raw) = node.kind()
                    && raw.text() == expected
                {
                    Some((
                        raw.location(),
                        raw.substitutions()
                            .any(|substitution| substitution == Substitution::SpecialChars),
                    ))
                } else if let InlineKind::CharacterReference(reference) = node.kind()
                    && reference.kind().to_string() == expected
                {
                    Some((reference.location(), false))
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("missing expected fragment in {source:?}"))?;
        assert_eq!(actual_escaping, escaped, "{source}");
        assert_eq!(
            &source[location.absolute_start()..=location.absolute_end()],
            token,
            "{source}"
        );
        assert_eq!(location.start().line(), 1);
        assert_eq!(location.end().line(), 1);
        assert_eq!(
            location.start().column(),
            u32::try_from(source[..location.absolute_start()].chars().count() + 1)?,
        );
        assert_eq!(
            location.end().column(),
            u32::try_from(source[..=location.absolute_end()].chars().count())?,
        );
    }
    Ok(())
}

#[test]
fn passthrough_character_references_map_generated_values_to_the_reference() -> Result<(), Error> {
    let source = "é pass:a,r[{value}] Ω";
    let options = Options::with_attributes([("value", "&#169;&#174;")])?;
    let parsed = parse(source, &options)?;
    let [BlockKind::Paragraph(paragraph)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected one paragraph".into());
    };
    let fragments: Vec<_> = paragraph
        .content()
        .inlines()
        .filter_map(|node| {
            if let InlineKind::CharacterReference(reference) = node.kind() {
                Some(reference)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(fragments.len(), 2);
    for reference in fragments {
        assert_eq!(
            &source[reference.location().absolute_start()..=reference.location().absolute_end()],
            "{value}"
        );
    }
    Ok(())
}
