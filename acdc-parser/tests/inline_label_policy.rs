use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, BlockRef, ContentRef, FootnoteDefinitionRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn paragraph(parsed: &acdc_parser::ParseResult) -> Result<ContentRef<'_>, Error> {
    let Some(BlockKind::Paragraph(paragraph)) =
        parsed.document_ref().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected paragraph".into());
    };
    Ok(paragraph.content())
}

#[rstest::rstest]
#[case("   π label   ", "π label   ")]
#[case("\tπ label\t", "π label\t")]
#[case("\u{a0}π label\u{a0}", "\u{a0}π label\u{a0}")]
#[case("  \nπ label\n", "π label\n")]
#[case("\u{b}π label", "π label")]
#[case("\0π label", "π label")]
fn shorthand_labels_keep_trailing_and_unicode_whitespace_at_the_authored_span(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] label: &str,
    #[case] expected: &str,
) -> Result<(), Error> {
    let source = format!("See <<target,{label}>>.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let reference = paragraph(&parsed)?
        .inlines()
        .find_map(|node| {
            if let InlineKind::CrossReference(reference) = node.kind() {
                Some(reference)
            } else {
                None
            }
        })
        .ok_or("missing xref")?;
    let [InlineKind::Plain(text)] = &*reference
        .text()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err(format!("expected plain label: {:?}", reference.text()).into());
    };
    assert_eq!(text.text(), expected);
    // Normalization removes trailing spaces before a newline before assigning byte offsets.
    let start = parsed
        .source()
        .find(expected)
        .ok_or("missing authored label")?;
    let end = start
        + expected
            .char_indices()
            .last()
            .ok_or("empty expected label")?
            .0;
    assert_eq!(
        (
            text.location().absolute_start(),
            text.location().absolute_end()
        ),
        (start, end)
    );
    Ok(())
}

#[rstest::rstest]
fn shorthand_ascii_whitespace_is_automatic_but_unicode_whitespace_is_explicit(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("  ", "\t\n", "\u{a0}")] label: &str,
) -> Result<(), Error> {
    let source = format!("<<target,{label}>>");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [InlineKind::CrossReference(reference)] = &*paragraph(&parsed)?
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected xref".into());
    };
    if label == "\u{a0}" {
        assert!(
            matches!(&*reference.text().inlines().map(InlineRef::kind).collect::<Vec<_>>(), [InlineKind::Plain(text)] if text.text() == label)
        );
    } else {
        assert_eq!(reference.text().inlines().count(), 0);
    }
    Ok(())
}

#[rstest::rstest]
fn content_attribute_escapes_keep_their_full_authored_span(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "image:photo.png[Alt,title=",
        "icon:heart[alt=",
        "link:guide[Label,title=",
        "https://example.org[Label,title=",
        "mailto:team@example.org[Label,title="
    )]
    prefix: &str,
) -> Result<(), Error> {
    let source = format!("α {prefix}'π \\] end'] ω.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let content = paragraph(&parsed)?
        .inlines()
        .find_map(|node| {
            if let InlineKind::Image(image) = node.kind() {
                image.tooltip()
            } else if let InlineKind::Icon(icon) = node.kind() {
                icon.alt()
            } else if let InlineKind::Link(link) = node.kind() {
                link.tooltip()
            } else if let InlineKind::Url(url) = node.kind() {
                url.tooltip()
            } else if let InlineKind::Mailto(mailto) = node.kind() {
                mailto.tooltip()
            } else {
                None
            }
        })
        .ok_or("missing content attribute")?;
    let [InlineKind::Plain(text)] = &*content.inlines().map(InlineRef::kind).collect::<Vec<_>>()
    else {
        return Err("expected decoded attribute text".into());
    };
    assert_eq!(text.text(), "π ] end");
    let authored = "π \\] end";
    let start = source.find(authored).ok_or("missing authored value")?;
    assert_eq!(
        (
            text.location().absolute_start(),
            text.location().absolute_end()
        ),
        (start, start + authored.len() - 1)
    );
    Ok(())
}

#[rstest::rstest]
fn selected_tooltips_register_macros_once_and_controls_stay_scalar(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("link:guide", "https://example.org", "mailto:team@example.org")] prefix: &str,
) -> Result<(), Error> {
    let source = format!(
        "{prefix}[Label,title='footnote:hidden[Hidden]',title='*Tip* footnote:tip[Selected]',role='footnote:role[Scalar]']"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let ids = parsed
        .document_ref()
        .footnotes()
        .map(FootnoteDefinitionRef::id)
        .collect::<Vec<_>>();
    assert_eq!(ids, [Some("tip")]);
    Ok(())
}

#[rstest::rstest]
fn quoted_xref_footnotes_keep_their_mapped_catalog_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = r#"Before xref:target["π \"word\" footnote:note[Note]",role=quiet] after."#;
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let reference = paragraph(&parsed)?
        .inlines()
        .find_map(|node| {
            if let InlineKind::CrossReference(reference) = node.kind() {
                Some(reference)
            } else {
                None
            }
        })
        .ok_or("missing xref")?;
    let occurrence = reference
        .text()
        .inlines()
        .find_map(|node| {
            if let InlineKind::Footnote(note) = node.kind() {
                Some(note)
            } else {
                None
            }
        })
        .ok_or("missing footnote occurrence")?;
    let note = parsed
        .document_ref()
        .footnotes()
        .next()
        .ok_or("missing catalog footnote")?;
    assert_eq!(note.location(), occurrence.location());
    assert_eq!(note.content(), occurrence.content());
    assert_eq!(
        note.location().absolute_start(),
        source.find("footnote:").ok_or("missing authored note")?
    );
    Ok(())
}
