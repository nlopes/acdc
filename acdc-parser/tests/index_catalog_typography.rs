use std::{fs, path::PathBuf};

use acdc_parser::{
    CompatibilityMode, InlineMacro, InlineNode, Options, ParseResult, SafeMode, SymbolKind,
    ast::{ContentRef, IndexTermRef, InlineKind},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn index(parsed: &ParseResult) -> Result<IndexTermRef<'_>, Error> {
    parsed
        .document_ref()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .find_map(|node| {
            if let InlineKind::IndexTerm(term) = node.kind() {
                Some(term)
            } else {
                None
            }
        })
        .ok_or_else(|| "index term".into())
}

fn symbols(content: ContentRef<'_>) -> Vec<SymbolKind> {
    let mut result = Vec::new();
    for node in content.inlines() {
        if let InlineKind::Symbol(symbol) = node.kind() {
            result.push(symbol.kind());
        }
        for field in node.content_fields() {
            result.extend(symbols(field.content()));
        }
    }
    result
}

#[rstest::rstest]
#[case("replacements,macros", vec![SymbolKind::SpacedEmDash])]
#[case("macros,replacements", vec![])]
fn catalog_typography_has_its_own_boundaries_and_registration_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] subs: &str,
    #[case] expected: Vec<SymbolKind>,
) -> Result<(), Error> {
    let source = format!("[subs=\"{subs}\"]\nleftindexterm2:[--]right");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let term = index(&parsed)?;
    assert_eq!(symbols(term.term()), [SymbolKind::WordBoundedEmDash]);
    assert_eq!(symbols(term.catalog().term()), expected);
    for node in term.catalog().term().inlines() {
        if let InlineKind::Symbol(symbol) = node.kind() {
            let location = symbol.location();
            assert_eq!(
                source.get(location.absolute_start()..=location.absolute_end()),
                Some("--")
            );
        }
    }
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
#[case(
    "indexterm:[Copyright (C),Secondary (R),Tertiary (TM),see=Other ...]",
    true
)]
#[case("indexterm2:[Copyright (C)]", true)]
#[case("indexterm2:[Copyright (C)]", false)]
fn catalog_typography_resolves_hierarchy_at_the_selected_stage(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] body: &str,
    #[case] enabled: bool,
) -> Result<(), Error> {
    let subs = if enabled {
        "replacements,macros"
    } else {
        "macros,replacements"
    };
    let source = format!("[subs=\"{subs}\"]\n{body}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let term = index(&parsed)?.catalog();
    assert_eq!(
        symbols(term.term()),
        if enabled {
            vec![SymbolKind::Copyright]
        } else {
            vec![]
        }
    );
    if let Some(content) = term.secondary() {
        assert_eq!(symbols(content), [SymbolKind::RegisteredTrademark]);
    }
    if let Some(content) = term.tertiary() {
        assert_eq!(symbols(content), [SymbolKind::Trademark]);
    }
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
#[case("Plain label")]
#[case("*Plain label*")]
fn ordinary_index_labels_do_not_need_an_additional_catalog_snapshot(
    #[case] label: &str,
) -> Result<(), Error> {
    let parsed = parse(&format!("indexterm2:[{label}]"), &Options::default())?;
    let owner = parsed
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            if let acdc_parser::Block::Paragraph(paragraph) = block {
                paragraph.content.iter().find_map(|node| {
                    if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
                        Some(term.as_ref())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .ok_or("index owner")?;
    assert!(std::ptr::eq(owner, owner.catalog_entry()));
    Ok(())
}

#[rstest::rstest]
fn catalog_typography_keeps_include_and_cell_snapshots_and_source_positions(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let primary = root.join("compatibility_index_catalog_contexts.adoc");
    let included_path = root.join("includes/index_catalog_selected.adoc");
    let source = fs::read_to_string(&included_path)?;
    let parsed = parse_file(
        &primary,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let terms = parsed
        .document_ref()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .filter_map(|node| {
            if let InlineKind::IndexTerm(term) = node.kind() {
                Some(term)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [included, _, cell, frozen] = terms.as_slice() else {
        return Err(format!("four index occurrences: {terms:?}").into());
    };
    assert_eq!(
        symbols(included.catalog().term()),
        [SymbolKind::Copyright, SymbolKind::Copyright]
    );
    assert_eq!(
        symbols(cell.catalog().term()),
        [SymbolKind::RegisteredTrademark]
    );
    assert_eq!(symbols(frozen.catalog().term()), []);
    let first = included
        .catalog()
        .term()
        .inlines()
        .find_map(|node| {
            if let InlineKind::Symbol(symbol) = node.kind() {
                Some(symbol)
            } else {
                None
            }
        })
        .ok_or("included copyright")?;
    let start = source.find("(C)").ok_or("source copyright")?;
    let prefix = source.get(..start).ok_or("source prefix")?;
    assert_eq!(
        first.location().start().line() as usize,
        prefix.bytes().filter(|byte| *byte == b'\n').count() + 1
    );
    assert_eq!(
        first.location().start().column() as usize,
        prefix
            .rsplit('\n')
            .next()
            .ok_or("source line")?
            .chars()
            .count()
            + 1
    );
    assert_eq!(
        parsed.resolve_location(first.location()).file,
        Some(included_path.canonicalize()?)
    );
    assert_eq!(parsed.document().footnotes.len(), 1);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn catalog_expands_protected_content_without_leaking_placeholders(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ":name: Expanded (C)\n\n[subs=\"specialcharacters,macros,attributes,quotes\"]\nindexterm2:[Start pass:attributes[{name}] *bold*]",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let catalog = index(&parsed)?.catalog();
    assert_eq!(
        catalog
            .term()
            .inlines()
            .map(|node| {
                if let InlineKind::Plain(text) = node.kind() {
                    text.text()
                } else if let InlineKind::Raw(raw) = node.kind() {
                    raw.text()
                } else {
                    "<unexpected>"
                }
            })
            .collect::<String>(),
        "Start Expanded (C) *bold*"
    );
    assert_eq!(symbols(catalog.term()), []);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}
