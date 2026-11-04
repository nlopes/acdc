use std::{fs, path::PathBuf};

use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{BlockKind, StructuralRef},
    parse, parse_file,
};

type TestError = Box<dyn std::error::Error>;

#[rstest::rstest]
fn complete_page_break_lines_keep_marker_ranges(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("<<<", "<<<<<")] marker: &str,
    #[values("", "\n", "\r\n", "  ", "\t\n", "\n\n", "\nAfter.\n")] ending: &str,
) -> Result<(), TestError> {
    let source = format!("Before café 😀.\n\n{marker}{ending}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut blocks = parsed.document().blocks();
    assert!(matches!(
        blocks.next().ok_or("leading paragraph")?.kind(),
        BlockKind::Paragraph(_)
    ));
    let page = blocks.next().ok_or("page break")?;
    assert!(matches!(page.kind(), BlockKind::PageBreak(_)));
    let location = page.location();
    let start = source.find(marker).ok_or("marker offset")?;
    assert_eq!(location.absolute_start(), start);
    assert_eq!(location.absolute_end(), start + marker.len() - 1);
    assert_eq!((location.start().line(), location.start().column()), (3, 1));
    assert_eq!(location.end().line(), 3);
    assert_eq!(location.end().column() as usize, marker.len());
    assert_eq!(blocks.count(), usize::from(ending.contains("After.")));
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn page_break_markers_require_an_entire_unindented_line(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("<<", "<<< after", "before <<<", "<<<x", "<<< <", " <<<", "\t<<<")] source: &str,
) -> Result<(), TestError> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(!parsed.document().descendants().any(|node| {
        matches!(node, StructuralRef::Block(block) if matches!(block.kind(), BlockKind::PageBreak(_)))
    }));
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn final_page_break_keeps_selected_metadata(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), TestError> {
    let source = ".Last page\n[#last.print,options=custom]\n<<<";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut blocks = parsed.document().blocks();
    let BlockKind::PageBreak(page) = blocks.next().ok_or("page break")?.kind() else {
        return Err("final marker must be a page break".into());
    };
    assert_eq!(page.id().ok_or("page ID")?.as_str(), "last");
    assert!(page.title().is_some());
    assert!(page.roles().contains("print"));
    assert!(page.options().contains("custom"));
    assert!(blocks.next().is_none());
    Ok(())
}

#[rstest::rstest]
fn page_break_contexts_keep_original_source_ranges(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), TestError> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let child = root.join("includes/page_break_selected.adoc");
    let source = fs::read_to_string(&child)?;
    let parsed = parse_file(
        root.join("page_break_contexts.adoc"),
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let pages = parsed
        .document()
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::Block(block) = node {
                if matches!(block.kind(), BlockKind::PageBreak(_)) {
                    return Some(block);
                }
            }
            None
        })
        .collect::<Vec<_>>();
    assert_eq!(pages.len(), 3);
    let included = pages.first().ok_or("included page break")?;
    let location = included.location();
    assert_eq!((location.start().line(), location.start().column()), (3, 5));
    assert_eq!((location.end().line(), location.end().column()), (3, 8));
    assert_eq!(
        parsed.resolve_location(location).file,
        Some(child.canonicalize()?)
    );
    assert!(source.contains("    <<<<"));
    let cell = pages.get(1).ok_or("cell page break")?.location();
    assert_eq!((cell.start().line(), cell.start().column()), (7, 2));
    assert_eq!((cell.end().line(), cell.end().column()), (7, 4));
    let final_page = pages.last().ok_or("final page break")?.location();
    assert_eq!(
        (final_page.start().line(), final_page.start().column()),
        (10, 1)
    );
    assert_eq!(
        (final_page.end().line(), final_page.end().column()),
        (10, 3)
    );
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}
