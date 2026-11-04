use std::{fs, path::PathBuf};

use acdc_parser::{
    CompatibilityMode, Options, SafeMode, Substitution, SymbolKind,
    ast::{BlockKind, ContentRef, DocumentRef, InlineKind, InlineRef, StructuralRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn body(document: DocumentRef<'_>) -> Result<ContentRef<'_>, Error> {
    let block = document.blocks().next().ok_or("missing block")?;
    if let BlockKind::Paragraph(paragraph) = block.kind() {
        return Ok(paragraph.content());
    }
    if let BlockKind::Passthrough(block) = block.kind() {
        return Ok(block.content());
    }
    Err("expected passthrough content".into())
}

fn source(body: &str, settings: &str, paragraph: bool) -> String {
    let content = if paragraph {
        format!("[pass{settings}]\n{body}\n")
    } else {
        format!("[{settings}]\n++++\n{body}\n++++\n")
    };
    format!(":word: *Declared*\n\n{content}")
}

#[rstest::rstest]
#[case("subs=normal", 1)]
#[case("subs=\"attributes,quotes,specialchars,replacements,macros\"", 2)]
fn block_passthroughs_resolve_order_and_register_selected_macros_once(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(false, true)] paragraph: bool,
    #[case] settings: &str,
    #[case] bold_count: usize,
) -> Result<(), Error> {
    let settings = if paragraph {
        format!(",{settings}")
    } else {
        settings.to_owned()
    };
    let source = source(
        "café *Authored* {word} (C) footnote:one[Note.] indexterm:[Term] anchor:target[]",
        &settings,
        paragraph,
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let nodes = body(parsed.document_ref())?;
    let bold = nodes
        .inlines()
        .filter(|node| matches!(node.kind(), InlineKind::Bold(_)))
        .collect::<Vec<_>>();
    assert_eq!(bold.len(), bold_count, "{nodes:?}");
    for node in bold {
        let location = node.location();
        let selected = source
            .get(location.absolute_start()..=location.absolute_end())
            .ok_or("invalid bold location")?;
        assert!(matches!(selected, "*Authored*" | "{word}"), "{selected}");
    }
    let copyright = nodes.inlines().find(|node| matches!(node.kind(), InlineKind::Symbol(symbol) if symbol.kind() == SymbolKind::Copyright)).ok_or("missing copyright")?;
    let location = copyright.location();
    assert_eq!(
        source.get(location.absolute_start()..=location.absolute_end()),
        Some("(C)")
    );
    assert_eq!(parsed.document_ref().footnotes().count(), 1);
    let note = parsed
        .document_ref()
        .footnotes()
        .next()
        .ok_or("missing footnote")?;
    assert_eq!(
        source.get(note.location().absolute_start()..=note.location().absolute_end()),
        Some("footnote:one[Note.]")
    );
    assert!(parsed.document_ref().references().contains("target"));
    assert_eq!(
        nodes
            .inlines()
            .filter(|node| matches!(node.kind(), InlineKind::IndexTerm(_)))
            .count(),
        1
    );
    for node in nodes.inlines() {
        if let InlineKind::Raw(raw) = node.kind() {
            assert!(
                raw.substitutions()
                    .all(|substitution| substitution == Substitution::SpecialChars)
            );
        }
    }
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn block_passthroughs_keep_default_and_none_content_raw(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(false, true)] paragraph: bool,
    #[values("", "subs=none", "subs=\"\"")] settings: &str,
) -> Result<(), Error> {
    let settings = if paragraph && !settings.is_empty() {
        format!(",{settings}")
    } else {
        settings.to_owned()
    };
    let input = "*Raw* {word} (C) footnote:unused[Note.] anchor:unused[]";
    let source = source(input, &settings, paragraph);
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [InlineKind::Raw(raw)] = &*body(parsed.document_ref())?
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected untouched raw content".into());
    };
    assert_eq!(raw.text().trim_end_matches('\n'), input);
    assert_eq!(raw.substitutions().collect::<Vec<_>>(), []);
    assert!(parsed.document_ref().footnotes().next().is_none());
    assert!(!parsed.document_ref().references().contains("unused"));
    Ok(())
}

#[rstest::rstest]
fn block_passthroughs_keep_snapshots_and_included_or_cell_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let primary = root.join("compatibility_block_passthrough_contexts.adoc");
    let child = root.join("includes/block_passthrough_selected.adoc");
    let source = fs::read_to_string(&primary)?;
    let included = fs::read_to_string(&child)?;
    let parsed = parse_file(
        &primary,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let mut selected = Vec::new();
    for node in parsed.document_ref().descendants() {
        let StructuralRef::Block(block) = node else {
            continue;
        };
        let content = if let BlockKind::Passthrough(pass) = block.kind() {
            pass.content()
        } else if let BlockKind::Paragraph(paragraph) = block.kind()
            && paragraph.style() == Some("pass")
        {
            paragraph.content()
        } else {
            continue;
        };
        let bold = content
            .inlines()
            .find_map(|node| {
                if let InlineKind::Bold(bold) = node.kind() {
                    Some(bold)
                } else {
                    None
                }
            })
            .ok_or("selected bold content")?;
        let label = bold
            .content()
            .inlines()
            .find_map(|node| {
                if let InlineKind::Raw(raw) = node.kind() {
                    Some(raw.text())
                } else {
                    None
                }
            })
            .ok_or("bold text")?;
        let (authored, spelling) = match label {
            "Included" => (&included, "*Included*"),
            "Cell" => (&source, "*Cell*"),
            "Later" => (&source, "{word}"),
            _ => return Err(format!("unexpected selected label: {label}").into()),
        };
        let start = if label == "Later" {
            authored.rfind(spelling)
        } else {
            authored.find(spelling)
        }
        .ok_or("authored content")?;
        let end = start + spelling.len() - 1;
        for (offset, position) in [
            (start, bold.location().start()),
            (end, bold.location().end()),
        ] {
            let prefix = authored.get(..offset).ok_or("source position")?;
            assert_eq!(
                position.line() as usize,
                prefix.bytes().filter(|byte| *byte == b'\n').count() + 1
            );
            assert_eq!(
                position.column() as usize,
                prefix.rsplit('\n').next().ok_or("line")?.chars().count() + 1
            );
        }
        if label == "Included" {
            assert_eq!(
                parsed.resolve_location(bold.location()).file,
                Some(child.canonicalize()?)
            );
            assert!(content.inlines().any(
                |node| matches!(node.kind(), InlineKind::Raw(raw) if raw.text() == "*Initial*")
            ));
        } else if label == "Cell" {
            assert!(content.inlines().any(
                |node| matches!(node.kind(), InlineKind::Raw(raw) if raw.text() == "*Later*")
            ));
        }
        selected.push(label);
    }
    assert_eq!(selected, ["Included", "Cell", "Later"]);
    assert_eq!(parsed.document_ref().footnotes().count(), 2);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}
