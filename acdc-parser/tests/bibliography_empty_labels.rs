use std::{fs, path::PathBuf};

use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{ContentRef, InlineKind},
    parse, parse_file,
};

type TestError = Box<dyn std::error::Error>;

fn plain_text(content: ContentRef<'_>) -> Result<String, TestError> {
    content.inlines().try_fold(String::new(), |mut text, node| {
        if let InlineKind::Plain(plain) = node.kind() {
            text.push_str(plain.text());
            Ok(text)
        } else {
            Err("expected plain label".into())
        }
    })
}

#[rstest::rstest]
#[case("", "")]
#[case(" ", " ")]
#[case("  ", " ")]
#[case("\t", "\t")]
#[case(" café ", "café ")]
fn bibliography_labels_preserve_empty_presence_and_authored_whitespace(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] supplied: &str,
    #[case] expected: &str,
) -> Result<(), TestError> {
    let source = format!("[bibliography]\n* [[[target,{supplied}]]] Entry.\n\nSee <<target>>.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
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
        .ok_or("bibliography anchor")?;
    assert!(anchor.is_bibliography());
    let label = anchor.bibliography_label().ok_or("present label")?;
    assert_eq!(plain_text(label)?, expected);
    let citation = anchor.reftext().ok_or("present citation")?;
    assert_eq!(plain_text(citation)?, expected);
    if supplied.is_empty() {
        assert_eq!(label.inlines().count(), 0);
    }
    for node in label.inlines().chain(citation.inlines()) {
        let InlineKind::Plain(plain) = node.kind() else {
            return Err("plain label".into());
        };
        let location = node.location();
        assert_eq!(
            source.get(location.absolute_start()..=location.absolute_end()),
            Some(plain.text())
        );
    }
    assert!(parsed.document_ref().references().contains("target"));
    assert_eq!(parsed.document_ref().footnotes().count(), 0);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn empty_bibliography_labels_survive_includes_and_table_cells(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), TestError> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let included = root.join("includes/empty_bibliography_selected.adoc");
    let authored = fs::read_to_string(&included)?;
    let parsed = parse_file(
        root.join("compatibility_empty_bibliography_contexts.adoc"),
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let mut labels = Vec::new();
    for field in parsed.document_ref().content_occurrences() {
        for node in field.content().inlines() {
            if let InlineKind::Anchor(anchor) = node.kind() {
                let label = anchor.bibliography_label().ok_or("present label")?;
                labels.push((anchor.id(), plain_text(label)?));
                if anchor.id() == "included-spaces" {
                    let node = label.inlines().next().ok_or("whitespace node")?;
                    let location = node.location();
                    let offset = authored.find(" ]]]").ok_or("authored spaces")?;
                    let prefix = authored.get(..offset).ok_or("prefix")?;
                    assert_eq!(
                        location.start().line() as usize,
                        prefix.bytes().filter(|byte| *byte == b'\n').count() + 1
                    );
                    assert_eq!(
                        location.start().column() as usize,
                        prefix.rsplit('\n').next().ok_or("line")?.chars().count() + 1
                    );
                    assert_eq!(
                        parsed.resolve_location(location).file,
                        Some(included.canonicalize()?)
                    );
                }
            }
        }
    }
    assert_eq!(
        labels,
        [
            ("included-empty", String::new()),
            ("included-spaces", " ".into()),
            ("cell-empty", String::new()),
            ("cell-unicode", "café ".into())
        ]
    );
    assert_eq!(parsed.document_ref().footnotes().count(), 0);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}
