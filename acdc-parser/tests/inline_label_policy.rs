use acdc_parser::{Block, CompatibilityMode, InlineMacro, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

fn paragraph(parsed: &acdc_parser::ParseResult) -> Result<&[InlineNode<'_>], Error> {
    let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
        return Err("expected paragraph".into());
    };
    Ok(&paragraph.content)
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
        .iter()
        .find_map(|node| {
            if let InlineNode::Macro(InlineMacro::CrossReference(reference)) = node {
                Some(reference)
            } else {
                None
            }
        })
        .ok_or("missing xref")?;
    let [InlineNode::PlainText(text)] = reference.text.as_slice() else {
        return Err(format!("expected plain label: {:?}", reference.text).into());
    };
    assert_eq!(text.content, expected);
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
        (text.location.absolute_start, text.location.absolute_end),
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
    let [InlineNode::Macro(InlineMacro::CrossReference(reference))] = paragraph(&parsed)? else {
        return Err("expected xref".into());
    };
    if label == "\u{a0}" {
        assert!(
            matches!(reference.text.as_slice(), [InlineNode::PlainText(text)] if text.content == label)
        );
    } else {
        assert_eq!(reference.text, []);
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
        .iter()
        .find_map(|node| {
            if let InlineNode::Macro(InlineMacro::Image(image)) = node {
                image.tooltip.as_ref()
            } else if let InlineNode::Macro(InlineMacro::Icon(icon)) = node {
                icon.alt.as_ref()
            } else if let InlineNode::Macro(InlineMacro::Link(link)) = node {
                link.tooltip.as_ref()
            } else if let InlineNode::Macro(InlineMacro::Url(url)) = node {
                url.tooltip.as_ref()
            } else if let InlineNode::Macro(InlineMacro::Mailto(mailto)) = node {
                mailto.tooltip.as_ref()
            } else {
                None
            }
        })
        .ok_or("missing content attribute")?;
    let [InlineNode::PlainText(text)] = content.as_ref() else {
        return Err("expected decoded attribute text".into());
    };
    assert_eq!(text.content, "π ] end");
    let authored = "π \\] end";
    let start = source.find(authored).ok_or("missing authored value")?;
    assert_eq!(
        (text.location.absolute_start, text.location.absolute_end),
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
        .document()
        .footnotes
        .iter()
        .map(|note| note.id)
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
        .iter()
        .find_map(|node| {
            if let InlineNode::Macro(InlineMacro::CrossReference(reference)) = node {
                Some(reference)
            } else {
                None
            }
        })
        .ok_or("missing xref")?;
    let occurrence = reference
        .text
        .iter()
        .find_map(|node| {
            if let InlineNode::Macro(InlineMacro::Footnote(note)) = node {
                Some(note)
            } else {
                None
            }
        })
        .ok_or("missing footnote occurrence")?;
    let note = parsed
        .document()
        .footnotes
        .first()
        .ok_or("missing catalog footnote")?;
    assert_eq!(note.location, occurrence.location);
    assert_eq!(note.content, occurrence.content);
    assert_eq!(
        note.location.absolute_start,
        source.find("footnote:").ok_or("missing authored note")?
    );
    Ok(())
}
