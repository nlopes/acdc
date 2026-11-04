//! Header controls use semantic values without another quote pass.

use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, BlockRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn author_values_preserve_authored_quotes(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Authors\n:author: \"Jane Doe\"\n:email: 'jane@example.org'\n\nBody.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let header = parsed.document_ref().header().ok_or("header")?;
    let [author] = &*header.authors().collect::<Vec<_>>() else {
        return Err("one author".into());
    };
    assert_eq!(author.first_name(), "\"Jane");
    assert_eq!(author.last_name(), "Doe\"");
    assert_eq!(author.email(), Some("'jane@example.org'"));
    Ok(())
}

#[rstest::rstest]
fn manpage_values_preserve_authored_quotes(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= tool(1)\n:doctype: manpage\n:manname: 'authored-name'\n:manpurpose: \"Quoted purpose\"\n\n== NAME\n\ntool - fallback purpose\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(
        parsed
            .document_ref()
            .attributes()
            .get("manname")
            .and_then(|value| value.as_str()),
        Some("'authored-name'")
    );
    assert_eq!(
        parsed
            .document_ref()
            .attributes()
            .get("manpurpose")
            .and_then(|value| value.as_str()),
        Some("\"Quoted purpose\"")
    );
    assert_eq!(
        parsed
            .document_ref()
            .attributes()
            .get("manvolnum")
            .and_then(|value| value.as_str()),
        Some("1")
    );
    Ok(())
}

#[rstest::rstest]
fn quoted_document_offsets_are_not_include_attribute_delimiters(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Offset\n:leveloffset: \"+1\"\n\n== One\n\nBody.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(BlockKind::Section(section)) =
        parsed.document_ref().blocks().next().map(BlockRef::kind)
    else {
        return Err("section".into());
    };
    assert_eq!(section.level(), 1);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn string_filetype_flags_retain_the_callers_literal_value(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "Body.",
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_attribute("filetype", "'custom'")
            .build()?,
    )?;
    assert_eq!(
        parsed
            .document_ref()
            .attributes()
            .get("filetype")
            .and_then(|value| value.as_str()),
        Some("'custom'")
    );
    assert!(
        parsed
            .document_ref()
            .attributes()
            .contains("filetype-'custom'")
    );
    assert!(
        !parsed
            .document_ref()
            .attributes()
            .contains("filetype-custom")
    );
    Ok(())
}
