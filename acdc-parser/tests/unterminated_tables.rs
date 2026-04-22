//! Integration tests for unterminated table warning detection.
//!
//! Each test loads a hand-written fixture from `fixtures/unterminated-tables/`,
//! parses through the public `parse_file` entry point (so the preprocessor
//! runs), and asserts on both the *count* and the *content* of the emitted
//! warnings — kind, delimiter string, source line, and `Display` rendering.

use std::{error::Error, path::Path};

use acdc_parser::{Options, Positioning, Warning, WarningKind, parse_file};

type TestResult = Result<(), Box<dyn Error>>;

const FIXTURE_DIR: &str = "fixtures/unterminated-tables";

fn parse_fixture(name: &str) -> Result<Vec<Warning>, Box<dyn Error>> {
    let path = Path::new(FIXTURE_DIR).join(name);
    if !path.exists() {
        return Err(format!("fixture missing: {}", path.display()).into());
    }
    let opts = Options::default();
    let mut result = parse_file(&path, &opts)?;
    Ok(result.take_warnings())
}

fn find_single_unterminated(warnings: &[Warning]) -> Result<&Warning, Box<dyn Error>> {
    let unterm: Vec<&Warning> = warnings
        .iter()
        .filter(|w| matches!(&w.kind, WarningKind::UnterminatedTable { .. }))
        .collect();

    match unterm.as_slice() {
        [w] => Ok(*w),
        other => Err(format!(
            "expected exactly 1 unterminated-table warning, got {} (all warnings: {:?})",
            other.len(),
            warnings,
        )
        .into()),
    }
}

fn warning_line(warning: &Warning) -> Result<usize, Box<dyn Error>> {
    let loc = warning
        .source_location()
        .ok_or("warning should carry a source location")?;
    Ok(match &loc.positioning {
        Positioning::Location(l) => l.start.line,
        Positioning::Position(p) => p.line,
    })
}

fn unterminated_delimiter(warning: &Warning) -> Result<&str, Box<dyn Error>> {
    if let WarningKind::UnterminatedTable { delimiter } = &warning.kind {
        Ok(delimiter)
    } else {
        Err(format!("expected UnterminatedTable, got {:?}", warning.kind).into())
    }
}

/// 01 — the motivating case: a plain pipe table is opened on line 9 and
/// never closed. Exactly one warning, `|===`.
#[test]
fn basic_unterminated_pipe_table() -> TestResult {
    let warnings = parse_fixture("01-basic-unterminated.adoc")?;
    let w = find_single_unterminated(&warnings)?;
    assert_eq!(unterminated_delimiter(w)?, "|===");
    assert_eq!(warning_line(w)?, 9);
    assert_eq!(
        format!("{}", w.kind),
        "unterminated table block (opened by `|===`)",
    );
    Ok(())
}

/// 02 — four separator variants in one document; only the final `:===`
/// table is unterminated. The others (pipe, exclamation-nested, comma
/// CSV) are properly closed and must not warn.
#[test]
fn only_the_last_separator_variant_is_unterminated() -> TestResult {
    let warnings = parse_fixture("02-all-four-separators.adoc")?;
    let w = find_single_unterminated(&warnings)?;
    assert_eq!(unterminated_delimiter(w)?, ":===");
    assert_eq!(
        format!("{}", w.kind),
        "unterminated table block (opened by `:===`)",
    );
    Ok(())
}

/// 03 — the grammar must preserve the exact `=` count from the opening
/// delimiter so the rendered token matches what the user wrote.
#[test]
fn longer_equals_run_is_preserved() -> TestResult {
    let warnings = parse_fixture("03-longer-equals-run.adoc")?;
    let w = find_single_unterminated(&warnings)?;
    assert_eq!(unterminated_delimiter(w)?, "|=======");
    assert_eq!(warning_line(w)?, 9);
    assert_eq!(
        format!("{}", w.kind),
        "unterminated table block (opened by `|=======`)",
    );
    Ok(())
}

/// 04 — regression guard: when a second `|===` appears later, the
/// terminated rule pairs them up and the unterminated fallback must not
/// fire. This mirrors asciidoctor's behavior (no warning, first table
/// closes at the second delim).
#[test]
fn second_delimiter_acts_as_close_no_warning() -> TestResult {
    let warnings = parse_fixture("04-second-open-is-actually-close.adoc")?;
    let any_unterminated = warnings
        .iter()
        .any(|w| matches!(&w.kind, WarningKind::UnterminatedTable { .. }));
    assert!(
        !any_unterminated,
        "unterminated fallback should not fire when a later `|===` acts as close, got: {warnings:?}",
    );
    Ok(())
}

/// 05 — unterminated `!===` inside an `a`-style cell. Outer closes
/// normally; the inner warning fires from the recursive cell parse.
///
/// Note: the reported line number is affected by a known cell-content
/// offset mapping bug (the recursive parse anchors at the `a|` prefix,
/// not the first content line). This test asserts on the delimiter
/// string only; add a line assertion once that offset is fixed.
#[test]
#[tracing_test::traced_test]
fn nested_inner_unterminated_in_a_cell() -> TestResult {
    let warnings = parse_fixture("05-nested-inner-unterminated.adoc")?;
    let w = find_single_unterminated(&warnings)?;
    assert_eq!(unterminated_delimiter(w)?, "!===");
    assert_eq!(
        format!("{}", w.kind),
        "unterminated table block (opened by `!===`)",
    );
    Ok(())
}

/// 06 — the preprocessor strips a single trailing newline (mirroring
/// `str::lines`). A document that is just `|===\n` arrives at the grammar
/// as `|===`. The unterminated rule accepts `eol() / ![_]` after the open
/// delimiter to handle both cases; this test would regress if that were
/// tightened back to a bare `eol()`.
#[test]
fn empty_open_delimiter_at_eof() -> TestResult {
    let warnings = parse_fixture("06-nothing-after-open.adoc")?;
    let w = find_single_unterminated(&warnings)?;
    assert_eq!(unterminated_delimiter(w)?, "|===");
    assert_eq!(warning_line(w)?, 10);
    assert_eq!(
        format!("{}", w.kind),
        "unterminated table block (opened by `|===`)",
    );
    Ok(())
}
