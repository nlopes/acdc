use std::{
    error::Error,
    path::{Path, PathBuf},
};

use acdc_parser::{
    Block, DelimitedBlock, DelimitedBlockType, Options, Paragraph, ParseResult, SafeMode,
    WarningKind, parse, parse_file,
};

type TestResult = Result<(), Box<dyn Error>>;

fn listing(parsed: &ParseResult) -> Result<&DelimitedBlock<'_>, Box<dyn Error>> {
    parsed
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            let Block::DelimitedBlock(block) = block else {
                return None;
            };
            matches!(block.inner, DelimitedBlockType::DelimitedListing(_)).then_some(block)
        })
        .ok_or_else(|| "expected listing".into())
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/preprocessor")
}

#[rstest::rstest]
#[case("[source,bash]\n", "----")]
#[case("[source,bash]\n", "--")]
#[case("", "```bash")]
fn retained_listing_body_precedes_inline_substitutions(
    #[case] metadata: &str,
    #[case] delimiter: &str,
) -> TestResult {
    let body = "cat <<'EOF'\n<1>\n\\<2>\n<!--3-->\n{value}\nEOF\n\n";
    let closing = if delimiter.starts_with('`') {
        "```"
    } else {
        delimiter
    };
    let input = format!(":value: replaced\n\n{metadata}{delimiter}\n{body}{closing}\n");
    let parsed = parse(&input, &Options::default())?;
    assert_eq!(listing(&parsed)?.source_text(), Some(body));
    Ok(())
}

#[rstest::rstest]
#[case("normal")]
#[case("attributes")]
#[case("none")]
fn retained_source_paragraph_is_literal(#[case] substitutions: &str) -> TestResult {
    let body = "printf '%s' '{value} *bold* \\<1>'";
    let input = format!(":value: replaced\n\n[source,bash,subs={substitutions}]\n{body}");
    let parsed = parse(&input, &Options::default())?;
    let paragraph = parsed
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            Some(paragraph)
        })
        .ok_or("expected source paragraph")?;
    assert_eq!(paragraph.source_text(), Some(body));
    Ok(())
}

#[test]
fn retained_text_does_not_change_serialization_or_equality() -> TestResult {
    let parsed = parse(
        "[source,bash]\n----\necho hi <1>\n----\n\n[source,bash]\necho hi",
        &Options::default(),
    )?;
    let original = listing(&parsed)?;
    let mut synthetic = DelimitedBlock::new(
        original.inner.clone(),
        original.delimiter,
        original.location.clone(),
    )
    .with_metadata(original.metadata.clone())
    .with_title(original.title.clone());
    synthetic
        .open_delimiter_location
        .clone_from(&original.open_delimiter_location);
    synthetic
        .close_delimiter_location
        .clone_from(&original.close_delimiter_location);
    assert!(synthetic.source_text().is_none());
    assert_eq!(original, &synthetic);
    assert_eq!(
        serde_json::to_value(original)?,
        serde_json::to_value(synthetic)?
    );

    let Some(Block::Paragraph(original)) = parsed.document().blocks.last() else {
        return Err("expected final source paragraph".into());
    };
    let synthetic = Paragraph::new(original.content.clone(), original.location.clone())
        .with_metadata(original.metadata.clone())
        .with_title(original.title.clone());
    assert!(synthetic.source_text().is_none());
    assert_eq!(original, &synthetic);
    assert_eq!(
        serde_json::to_value(original)?,
        serde_json::to_value(synthetic)?
    );
    Ok(())
}

#[test]
fn retained_table_cell_source_uses_its_nested_input() -> TestResult {
    let parsed = parse(
        "[cols=a]\n|===\n|[source,bash]\n----\ncat <<'EOF'\n<1>\nEOF\n----\n|===",
        &Options::default(),
    )?;
    let Some(Block::DelimitedBlock(outer)) = parsed.document().blocks.first() else {
        return Err("expected table".into());
    };
    let DelimitedBlockType::DelimitedTable(table) = &outer.inner else {
        return Err("expected table content".into());
    };
    let inner = table
        .rows
        .iter()
        .flat_map(|row| &row.columns)
        .flat_map(|cell| &cell.content)
        .find_map(|block| {
            let Block::DelimitedBlock(inner) = block else {
                return None;
            };
            Some(inner)
        })
        .ok_or("expected nested listing")?;
    assert_eq!(inner.source_text(), Some("cat <<'EOF'\n<1>\nEOF\n"));
    Ok(())
}

#[test]
fn retained_include_body_contains_transformed_content() -> TestResult {
    let parsed = parse_file(
        fixtures().join("include_indent_main.adoc"),
        &Options::default(),
    )?;
    assert_eq!(listing(&parsed)?.source_text(), Some("      TARGETLINE\n"));
    Ok(())
}

#[test]
fn retained_comment_in_list_continuation_includes_final_newline() -> TestResult {
    let parsed = parse("* item\n+\n////\ncomment body\n////\n", &Options::default())?;
    let Some(Block::UnorderedList(list)) = parsed.document().blocks.first() else {
        return Err("expected list".into());
    };
    let comment = list
        .items
        .iter()
        .flat_map(|item| &item.blocks)
        .find_map(|block| {
            let Block::DelimitedBlock(block) = block else {
                return None;
            };
            matches!(block.inner, DelimitedBlockType::DelimitedComment(_)).then_some(block)
        })
        .ok_or("expected attached comment")?;
    assert_eq!(comment.source_text(), Some("comment body\n"));
    Ok(())
}

#[test]
fn source_location_resolves_nested_selected_content() -> TestResult {
    let directory = fixtures();
    let primary = directory.join("include_tag_diagnostics_main.adoc");
    let parsed = parse_file(&primary, &Options::default())?;
    let selected = parsed
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            (paragraph.source_text() == Some("Selected.")).then_some(paragraph)
        })
        .ok_or("expected selected paragraph")?;
    let source = parsed.source_location(&selected.location);
    assert_eq!(
        source.file,
        Some(directory.join("include_tag_diagnostics_target.adoc"))
    );
    assert_eq!(source.location.start.line, 2);
    let Some(Block::Paragraph(first)) = parsed.document().blocks.first() else {
        return Err("expected primary paragraph".into());
    };
    assert_eq!(parsed.source_location(&first.location).file, Some(primary));
    Ok(())
}

#[test]
fn unknown_include_chain_does_not_resolve_to_the_primary_file() -> TestResult {
    let parsed = parse_file(
        fixtures().join("include_indent_main.adoc"),
        &Options::default(),
    )?;
    let mut location = acdc_parser::Location::default();
    location.start.file = Some(std::sync::Arc::new(vec!["unknown.adoc".into()]));
    assert!(parsed.source_location(&location).file.is_none());
    Ok(())
}

#[rstest::rstest]
#[case("include::__missing_recovery_test__.adoc[]", SafeMode::Unsafe)]
#[case("include::include_indent_target.rb[tag=missing]", SafeMode::Unsafe)]
#[case("include::include_indent_target.rb[lines=0]", SafeMode::Unsafe)]
#[case("include::include_indent_target.rb[]", SafeMode::Secure)]
#[case("include::https://example.invalid/script.adoc[]", SafeMode::Safe)]
fn source_loss_has_a_typed_located_warning(
    #[case] input: &str,
    #[case] safe_mode: SafeMode,
) -> TestResult {
    let options = Options::builder()
        .with_base_dir(fixtures())
        .with_safe_mode(safe_mode)
        .build()?;
    let parsed = parse(input, &options)?;
    let warning = parsed
        .warnings()
        .iter()
        .find(|warning| matches!(warning.kind, WarningKind::ContentRecovery { .. }))
        .ok_or("expected content recovery")?;
    assert_eq!(
        warning
            .source_location()
            .ok_or("expected source location")?
            .location
            .start
            .line,
        1
    );
    Ok(())
}

#[test]
fn absent_optional_include_does_not_recover_content() -> TestResult {
    let options = Options::builder().with_base_dir(fixtures()).build()?;
    let parsed = parse(
        "include::__missing_recovery_test__.adoc[opts=optional]",
        &options,
    )?;
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[test]
fn disabled_includes_record_content_recovery() -> TestResult {
    let options = Options::builder()
        .with_base_dir(fixtures())
        .with_attribute("max-include-depth", "0")
        .build()?;
    let parsed = parse("include::include_indent_target.rb[]", &options)?;
    assert!(
        parsed
            .warnings()
            .iter()
            .any(|warning| matches!(warning.kind, WarningKind::ContentRecovery { .. }))
    );
    Ok(())
}

#[test]
fn malformed_include_line_selection_is_a_parse_error() -> TestResult {
    let options = Options::builder().with_base_dir(fixtures()).build()?;
    let Err(error) = parse(
        "include::include_indent_target.rb[lines=not-a-number]",
        &options,
    ) else {
        return Err("malformed selection must not widen the include".into());
    };
    assert!(matches!(error, acdc_parser::Error::InvalidLineRange(_, _)));
    assert_eq!(
        error
            .source_location()
            .ok_or("missing error location")?
            .location
            .start
            .line,
        1
    );
    Ok(())
}

#[rstest::rstest]
#[case("ifdef::missing[]")]
#[case("ifndef::missing[]")]
fn unclosed_conditionals_record_content_recovery(#[case] directive: &str) -> TestResult {
    let parsed = parse(
        &format!("intro\n\n{directive}\nremaining content"),
        &Options::default(),
    )?;
    let warning = parsed
        .warnings()
        .iter()
        .find(|warning| matches!(warning.kind, WarningKind::ContentRecovery { .. }))
        .ok_or("expected content recovery")?;
    assert_eq!(
        warning
            .source_location()
            .ok_or("expected source location")?
            .location
            .start
            .line,
        3
    );
    Ok(())
}

#[rstest::rstest]
#[case("----\nunterminated")]
#[case("[cols=\"1,1\"]\n|===\n|incomplete row\n|===")]
#[case("ifdef::missing[]\nremaining content")]
fn source_recovery_remains_available_after_warning_routing(#[case] input: &str) -> TestResult {
    let mut parsed = parse(input, &Options::default())?;
    let kind = parsed
        .source_recovery()
        .ok_or("expected source recovery")?
        .kind
        .clone();
    let warnings = parsed.take_warnings();
    assert_ne!(warnings, []);
    assert_eq!(parsed.warnings(), []);
    assert_eq!(
        parsed.source_recovery().ok_or("lost source recovery")?.kind,
        kind
    );
    Ok(())
}

#[test]
fn presentation_warning_is_not_a_source_recovery() -> TestResult {
    let mut parsed = parse("See <<missing>>.", &Options::default())?;
    assert_ne!(parsed.take_warnings(), []);
    assert!(parsed.source_recovery().is_none());
    Ok(())
}

#[rstest::rstest]
#[case(None, false)]
#[case(Some("attributes"), true)]
#[case(Some("+attributes"), true)]
#[case(Some("attributes+"), true)]
#[case(Some("none"), false)]
#[case(Some("-attributes"), false)]
#[case(Some("+normal,-attributes"), false)]
#[case(Some("-normal,+attributes"), true)]
#[case(Some("+attributes,-normal"), false)]
#[case(Some("-attributes,attributes+"), true)]
fn block_substitutions_use_the_supplied_baseline(
    #[case] subs: Option<&str>,
    #[case] attributes_enabled: bool,
) -> TestResult {
    let attribute = subs
        .map(|value| format!(",subs=\"{value}\""))
        .unwrap_or_default();
    let input = format!("[source,sh{attribute}]\n----\n{{shell}} --version\n----\n");
    let parsed = parse(&input, &Options::default())?;
    let metadata = &listing(&parsed)?.metadata;
    assert_eq!(
        metadata.uses_substitution(
            &acdc_parser::Substitution::Attributes,
            acdc_parser::VERBATIM
        ),
        cfg!(feature = "pre-spec-subs") && attributes_enabled,
    );
    Ok(())
}

#[cfg(not(feature = "pre-spec-subs"))]
#[test]
fn ignored_substitutions_remain_source_recovery_after_warning_routing() -> TestResult {
    let mut parsed = parse(
        "[source,sh,subs=attributes]\n----\n{shell} --version\n----\n",
        &Options::default(),
    )?;
    let warnings = parsed.take_warnings();
    assert!(
        warnings
            .iter()
            .any(|warning| matches!(warning.kind, WarningKind::ContentRecovery { .. }))
    );
    let recovery = parsed
        .source_recovery()
        .ok_or("lost ignored substitutions")?;
    assert_eq!(
        recovery
            .source_location()
            .ok_or("missing source position")?
            .location
            .start
            .line,
        1
    );
    Ok(())
}
