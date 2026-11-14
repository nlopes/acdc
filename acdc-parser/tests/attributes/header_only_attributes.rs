//! A body entry for an attribute that only takes effect in the document header
//! warns, so a header ended early by an empty line doesn't fail silently.
//! Asciidoctor doesn't warn.

use acdc_parser::{
    CompatibilityMode, Options, ParseResult, WarningKind,
    ast::{BlockKind, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn options(mode: CompatibilityMode) -> Result<Options<'static>, Error> {
    Ok(Options::builder().with_compatibility_mode(mode).build()?)
}

/// The header-only warnings as `(name, line)`.
fn header_only_warnings(parsed: &ParseResult) -> Vec<(String, Option<u32>)> {
    parsed
        .warnings()
        .iter()
        .filter_map(|warning| {
            if let WarningKind::HeaderOnlyAttribute { name } = &warning.kind {
                let line = warning
                    .source_location()
                    .map(|source| source.location.start.line);
                Some((name.clone(), line))
            } else {
                None
            }
        })
        .collect()
}

fn warned(entries: &[(&str, u32)]) -> Vec<(String, Option<u32>)> {
    entries
        .iter()
        .map(|(name, line)| ((*name).to_owned(), Some(*line)))
        .collect()
}

const GROUPED: &str = "= User Guide\n:description: How to install the product.\n\n:doctype: book\n:toc: left\n:sectnums:\n\n:icons: font\n:source-highlighter: rouge\n\n== Install\n\nText.\n";

#[rstest::rstest]
fn grouped_header_attributes_warn_for_header_only_names(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(GROUPED, &options(mode)?)?;
    assert_eq!(
        header_only_warnings(&parsed),
        warned(&[("doctype", 4), ("toc", 5), ("source-highlighter", 9)])
    );
    let commented = GROUPED.replace(".\n\n:doctype:", ".\n// layout\n:doctype:");
    let commented = commented.replace(":sectnums:\n\n", ":sectnums:\n// layout\n");
    let parsed = parse(&commented, &options(mode)?)?;
    assert_eq!(header_only_warnings(&parsed), []);
    Ok(())
}

#[rstest::rstest]
fn unset_in_the_body_warns_like_a_set(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Title\n:toc:\n\n:!toc:\n:doctype!:\n\nText.\n",
        &options(mode)?,
    )?;
    assert_eq!(
        header_only_warnings(&parsed),
        warned(&[("toc", 4), ("doctype", 5)])
    );
    Ok(())
}

/// acdc's own HTML settings are read from the header too.
#[rstest::rstest]
fn acdc_header_settings_in_the_body_warn(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Title\n\n:dark-mode:\n:highlight-style: github-dark\n:highlight-css: class\n:csp: default-src 'self'\n\nText.\n",
        &options(mode)?,
    )?;
    assert_eq!(
        header_only_warnings(&parsed),
        warned(&[
            ("dark-mode", 3),
            ("highlight-style", 4),
            ("highlight-css", 5),
            ("csp", 6)
        ])
    );
    Ok(())
}

#[rstest::rstest]
fn body_entries_acdc_applies_in_the_body_do_not_warn(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Title\n\n:icons: font\n:stem:\n:sectnums:\n:nofooter:\n:imagesdir: img\n:custom: value\n\nText.\n",
        &options(mode)?,
    )?;
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
fn header_entries_do_not_warn(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("= Title\n", "")] title: &str,
) -> Result<(), Error> {
    let source = format!(
        "{title}:doctype: book\n:toc: left\n:!source-highlighter:\n:description: Desc\n\nText.\n"
    );
    let parsed = parse(&source, &options(mode)?)?;
    assert_eq!(header_only_warnings(&parsed), [], "{source:?}");
    Ok(())
}

#[rstest::rstest]
fn caller_attributes_never_warn(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    // A caller value alone, and a body entry that the caller's lock rejects.
    let locked = Options::builder()
        .with_compatibility_mode(mode)
        .with_attribute("toc", "left")
        .build()?;
    let parsed = parse("= Title\n\n:toc: right\n\nText.\n", &locked)?;
    assert_eq!(header_only_warnings(&parsed), []);
    // A caller default doesn't lock the attribute, so the body entry applies
    // and warns.
    let defaulted = Options::builder()
        .with_compatibility_mode(mode)
        .with_default_attribute("doctype", "article")
        .build()?;
    let parsed = parse("= Title\n\n:doctype: book\n\nText.\n", &defaulted)?;
    assert_eq!(header_only_warnings(&parsed), warned(&[("doctype", 3)]));
    Ok(())
}

#[test]
fn untitled_header_boundary_follows_the_mode() -> Result<(), Error> {
    let source = ":a: 1\n\n:toc:\n\nText.\n";
    let parsed = parse(source, &options(CompatibilityMode::Asciidoctor)?)?;
    assert_eq!(header_only_warnings(&parsed), []);
    let parsed = parse(source, &options(CompatibilityMode::Draft)?)?;
    assert_eq!(header_only_warnings(&parsed), warned(&[("toc", 3)]));
    Ok(())
}

#[rstest::rstest]
fn block_metadata_entry_warns_and_still_applies_for_references(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Title\n\n[.lead]\n:description: Body text\n{description}\n",
        &options(mode)?,
    )?;
    assert_eq!(header_only_warnings(&parsed), warned(&[("description", 4)]));
    let paragraph = parsed
        .document()
        .blocks()
        .find_map(|block| {
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                Some(paragraph)
            } else {
                None
            }
        })
        .ok_or("paragraph")?;
    let text: String = paragraph
        .content()
        .inlines()
        .filter_map(|inline| {
            if let InlineKind::Plain(node) = inline.kind() {
                Some(node.text().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(text, "Body text");
    Ok(())
}
