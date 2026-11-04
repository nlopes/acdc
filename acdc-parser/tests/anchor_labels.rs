use std::{fs, path::PathBuf};

use acdc_parser::{CompatibilityMode, Options, SafeMode, ast::InlineKind, parse, parse_file};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn inline_bracket_anchors_keep_inner_closing_brackets_in_the_label(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "Label [nested] tail",
        "link:https://example.org[Link] footnote:[Note] (C)",
        "*Bold [nested]* label",
        "λ🚀 [first] [second] tail",
        "Label, with comma [nested] tail"
    )]
    label: &str,
) -> Result<(), Error> {
    let source = format!("Earlier.\n\nBefore [[target,{label}]] after.\n\n<<target>>.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let reference = parsed
        .document_ref()
        .references()
        .get("target")
        .ok_or("recognized anchor")?;
    assert!(reference.xreflabel().is_some());
    assert_eq!(parsed.document_ref().footnotes().count(), 0);
    let anchor = parsed
        .document_ref()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .find_map(|node| {
            if let InlineKind::Anchor(anchor) = node.kind() {
                Some(anchor)
            } else {
                None
            }
        })
        .ok_or("anchor occurrence")?;
    assert_eq!(
        source.get(anchor.location().absolute_start()..=anchor.location().absolute_end()),
        Some(format!("[[target,{label}]]").as_str())
    );
    assert_eq!(parsed.document_ref().references().len(), 1);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn escaped_or_incomplete_anchors_do_not_create_targets(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "Before \\[[target,Label [nested] tail]] after.",
        "Before [[target,Label [nested] tail"
    )]
    source: &str,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document_ref().references().len(), 0);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn nested_anchor_labels_keep_source_coordinates_in_includes_and_cells(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let included = root.join("includes/anchor_labels_selected.adoc");
    let source = fs::read_to_string(&included)?;
    let parsed = parse_file(
        root.join("compatibility_anchor_label_contexts.adoc"),
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let references = parsed.document_ref().references();
    assert!(references.contains("included-label"));
    assert!(references.contains("cell-label"));
    let anchor = parsed
        .document_ref()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .find_map(|node| {
            if let InlineKind::Anchor(anchor) = node.kind() {
                (anchor.id() == "included-label").then_some(anchor)
            } else {
                None
            }
        })
        .ok_or("included anchor")?;
    let start = source.find("[[included-label").ok_or("authored ID")?;
    let prefix = source.get(..start).ok_or("prefix")?;
    assert_eq!(
        anchor.location().start().line() as usize,
        prefix.bytes().filter(|byte| *byte == b'\n').count() + 1
    );
    assert_eq!(
        anchor.location().start().column() as usize,
        prefix.rsplit('\n').next().ok_or("line")?.chars().count() + 1
    );
    assert_eq!(
        parsed.resolve_location(anchor.location()).file,
        Some(included.canonicalize()?)
    );
    assert_eq!(parsed.document_ref().footnotes().count(), 0);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn nested_anchor_labels_support_natural_reference_targets(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "Before [[target,Label [nested] tail]] after.\n\n<<Label [nested] tail>>.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let target = parsed
        .document_ref()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .find_map(|node| {
            if let InlineKind::CrossReference(reference) = node.kind() {
                Some(reference.target())
            } else {
                None
            }
        })
        .ok_or("cross-reference")?;
    assert_eq!(target, "target");
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}
