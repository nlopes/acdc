use std::{fs, path::PathBuf};

use acdc_parser::{
    CompatibilityMode, Options, SafeMode, SymbolKind,
    ast::{ContentRef, InlineKind},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn symbols(content: ContentRef<'_>) -> Vec<acdc_parser::ast::SymbolRef<'_>> {
    let mut result = Vec::new();
    for node in content.inlines() {
        if let InlineKind::Symbol(symbol) = node.kind() {
            result.push(symbol);
        }
        for field in node.content_fields() {
            result.extend(symbols(field.content()));
        }
    }
    result
}

#[rstest::rstest]
#[case("[id=target,reftext='café (C)']\nBody.", false)]
#[case("[id=target,reftext=\"café (C)\"]\nBody.", true)]
#[case("[id=target,reftext=café (C)]\nBody.", true)]
#[case("[[target,café (C)]]\nBody.", false)]
#[case("[bibliography]\n* [[[target,café (C)]]] Citation.", false)]
#[case("Before [[target,café (C)]] after.", false)]
#[case("Before anchor:target[café (C)] after.", false)]
#[case("[id=target,reftext='café (C)',subs=none]\nBody.", false)]
fn reference_labels_resolve_their_own_profile_and_exact_source_span(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] source: &str,
    #[case] draft_literal: bool,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let label = parsed
        .document()
        .references
        .get("target")
        .and_then(|reference| reference.xreflabel.as_ref())
        .ok_or("reference label")?;
    let found = symbols(label.into());
    if draft_literal && mode == CompatibilityMode::Draft {
        assert_eq!(found.len(), 0);
    } else {
        let [symbol] = found.as_slice() else {
            return Err(format!("one symbol: {found:?}").into());
        };
        assert_eq!(symbol.kind(), SymbolKind::Copyright);
        assert_eq!(
            source.get(symbol.location().absolute_start()..=symbol.location().absolute_end()),
            Some("(C)")
        );
    }
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn restricted_labels_keep_macros_literal_and_do_not_register_them(
    #[values(
        "[id=target,reftext='*Bold (R)* link:https://example.org[Link] footnote:[Note]']\nBody.",
        "Before anchor:target[*Bold (R)* link:https://example.org[Link\\] footnote:[Note\\]] after."
    )]
    source: &str,
) -> Result<(), Error> {
    let parsed = parse(source, &Options::default())?;
    let label = parsed
        .document()
        .references
        .get("target")
        .and_then(|reference| reference.xreflabel.as_ref())
        .ok_or("reference label")?;
    assert_eq!(
        symbols(label.into())
            .iter()
            .map(|symbol| symbol.kind())
            .collect::<Vec<_>>(),
        [SymbolKind::RegisteredTrademark]
    );
    assert_eq!(parsed.document().footnotes.len(), 0);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn reference_typography_keeps_escaped_tokens_literal(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[id=target,reftext='café \\(C)']\nBody.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let label = parsed
        .document()
        .references
        .get("target")
        .and_then(|reference| reference.xreflabel.as_ref())
        .ok_or("reference label")?;
    assert_eq!(symbols(label.into()).len(), 0);
    Ok(())
}

#[rstest::rstest]
fn reference_labels_keep_declaration_snapshots_and_included_positions(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let included = root.join("includes/reference_typography_selected.adoc");
    let source = fs::read_to_string(&included)?;
    let parsed = parse_file(
        root.join("compatibility_reference_typography_contexts.adoc"),
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let references = &parsed.document().references;
    let label = references
        .get("included-label")
        .and_then(|r| r.xreflabel.as_ref())
        .ok_or("included label")?;
    let found = symbols(label.into());
    assert_eq!(
        found.iter().map(|symbol| symbol.kind()).collect::<Vec<_>>(),
        [SymbolKind::Copyright, SymbolKind::Copyright]
    );
    let authored = found.last().ok_or("authored copyright")?;
    let start = source.find("(C)").ok_or("authored token")?;
    let prefix = source.get(..start).ok_or("prefix")?;
    assert_eq!(
        authored.location().start().line() as usize,
        prefix.bytes().filter(|byte| *byte == b'\n').count() + 1
    );
    assert_eq!(
        authored.location().start().column() as usize,
        prefix.rsplit('\n').next().ok_or("line")?.chars().count() + 1
    );
    assert_eq!(
        parsed.resolve_location(authored.location()).file,
        Some(included.canonicalize()?)
    );
    let cell = references
        .get("cell-label")
        .and_then(|r| r.xreflabel.as_ref())
        .ok_or("cell label")?;
    assert_eq!(
        symbols(cell.into())
            .iter()
            .map(|symbol| symbol.kind())
            .collect::<Vec<_>>(),
        [SymbolKind::RegisteredTrademark, SymbolKind::Trademark]
    );
    assert_eq!(parsed.document().footnotes.len(), 0);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn natural_reference_targets_keep_their_authored_spelling(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[id=target,reftext='Label (C)']\nBody.\n\n<<Label (C)>>.",
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
