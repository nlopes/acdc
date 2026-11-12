use std::{
    error::Error,
    path::{Path, PathBuf},
};

use acdc_parser::{
    Options, Paragraph, ParseResult, SafeMode, WarningKind,
    ast::{
        AttributeValueRef, BlockKind, BlockRef, ListItemRef, ListingRef, TableCellRef, TableRowRef,
    },
    parse, parse_file,
};

type TestResult = Result<(), Box<dyn Error>>;

fn listing(parsed: &ParseResult) -> Result<ListingRef<'_>, Box<dyn Error>> {
    parsed
        .document()
        .blocks()
        .find_map(|block| {
            if let BlockKind::Listing(listing) = block.kind() {
                Some(listing)
            } else {
                None
            }
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
        .blocks()
        .find_map(|block| {
            let BlockKind::Paragraph(paragraph) = block.kind() else {
                return None;
            };
            Some(paragraph)
        })
        .ok_or("expected source paragraph")?;
    assert_eq!(paragraph.source_text(), Some(body));
    Ok(())
}

#[test]
fn retained_table_cell_source_uses_its_nested_input() -> TestResult {
    let parsed = parse(
        "[cols=a]\n|===\n|[source,bash]\n----\ncat <<'EOF'\n<1>\nEOF\n----\n|===",
        &Options::default(),
    )?;
    let Some(BlockKind::Table(table)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected table".into());
    };
    let inner = table
        .rows()
        .flat_map(TableRowRef::cells)
        .flat_map(TableCellRef::blocks)
        .find_map(|block| {
            if let BlockKind::Listing(inner) = block.kind() {
                Some(inner)
            } else {
                None
            }
        })
        .ok_or("expected nested listing")?;
    assert_eq!(inner.source_text(), Some("cat <<'EOF'\n<1>\nEOF\n"));
    Ok(())
}

#[test]
fn retained_include_body_contains_transformed_content() -> TestResult {
    let parsed = parse_file(
        fixtures().join("include_indent_main.adoc"),
        &Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .build()?,
    )?;
    assert_eq!(listing(&parsed)?.source_text(), Some("      TARGETLINE\n"));
    Ok(())
}

#[test]
fn retained_comment_in_list_continuation_includes_final_newline() -> TestResult {
    let parsed = parse("* item\n+\n////\ncomment body\n////\n", &Options::default())?;
    let Some(BlockKind::UnorderedList(list)) =
        parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected list".into());
    };
    let comment = list
        .items()
        .flat_map(ListItemRef::blocks)
        .find_map(|block| {
            if let BlockKind::DelimitedComment(comment) = block.kind() {
                Some(comment)
            } else {
                None
            }
        })
        .ok_or("expected attached comment")?;
    assert_eq!(comment.source_text(), Some("comment body\n"));
    Ok(())
}

#[test]
fn source_location_resolves_nested_selected_content() -> TestResult {
    let directory = fixtures();
    let primary = directory.join("include_tag_diagnostics_main.adoc");
    let parsed = parse_file(
        &primary,
        &Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .build()?,
    )?;
    let selected = parsed
        .document()
        .blocks()
        .find_map(|block| {
            let BlockKind::Paragraph(paragraph) = block.kind() else {
                return None;
            };
            (paragraph.source_text() == Some("Selected.")).then_some(paragraph)
        })
        .ok_or("expected selected paragraph")?;
    let source = parsed.resolve_location(selected.location());
    assert_eq!(
        source.file,
        Some(directory.join("include_tag_diagnostics_target.adoc"))
    );
    assert_eq!(source.location.start.line, 2);
    let Some(BlockKind::Paragraph(first)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected primary paragraph".into());
    };
    assert_eq!(
        parsed.resolve_location(first.location()).file,
        Some(primary)
    );
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
    let options = Options::builder()
        .with_safe_mode(acdc_parser::SafeMode::Unsafe)
        .with_base_dir(fixtures())
        .build()?;
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
fn malformed_include_line_selection_preserves_the_directive() -> TestResult {
    let options = Options::builder().with_base_dir(fixtures()).build()?;
    let source = "include::include_indent_target.rb[lines=not-a-number]";
    let parsed = parse(source, &options)?;
    assert_eq!(parsed.preprocessed_source(), source);
    let warning = parsed
        .source_recovery()
        .ok_or("expected selection warning")?;
    assert_eq!(
        warning
            .source_location()
            .ok_or("missing warning location")?
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
    let substitutions = match listing(&parsed)?.attributes().get("subs") {
        Some(AttributeValueRef::Substitutions(spec)) => spec.resolve(acdc_parser::VERBATIM),
        _ => acdc_parser::VERBATIM.to_vec(),
    };
    assert_eq!(
        substitutions.contains(&acdc_parser::Substitution::Attributes),
        attributes_enabled
    );
    Ok(())
}

#[test]
fn unsupported_substitutions_remain_source_recovery_after_warning_routing() -> TestResult {
    let mut parsed = parse(
        "[source,sh,subs=unknown]\n----\n{shell} --version\n----\n",
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
        .ok_or("lost unsupported substitutions")?;
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

#[rstest::rstest]
#[case(acdc_parser::CompatibilityMode::Asciidoctor)]
#[case(acdc_parser::CompatibilityMode::Draft)]
fn typed_paragraph_retains_literal_body(
    #[case] mode: acdc_parser::CompatibilityMode,
) -> TestResult {
    let body = "printf '%s' '{value} *bold* \\<1>'";
    let input = format!(":value: replaced\n\n[source,bash,subs=normal]\n{body}");
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let parsed = parse(&input, &options)?;
    let Some(block) = parsed.document().blocks().next() else {
        return Err("expected source paragraph".into());
    };
    let acdc_parser::ast::BlockKind::Paragraph(paragraph) = block.kind() else {
        return Err("expected source paragraph".into());
    };
    assert_eq!(paragraph.source_text(), Some(body));
    let constructed = Paragraph::new(Vec::new(), acdc_parser::Location::default());
    assert_eq!(
        acdc_parser::ast::ParagraphRef::from(&constructed).source_text(),
        None
    );
    Ok(())
}
