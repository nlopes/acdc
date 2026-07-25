use std::error::Error as StdError;

use acdc_parser::{Error, Options, parse};

type TestResult = Result<(), Box<dyn StdError>>;

fn assert_parses(input: &str) -> TestResult {
    let result = parse(input, &Options::default())?;
    assert!(
        !result.document().blocks.is_empty(),
        "expected the table to produce a document block",
    );
    Ok(())
}

fn assert_table_limit(input: &str, expected_message: &str, expected_line: u32) -> TestResult {
    let Err(error) = parse(input, &Options::default()) else {
        return Err("expected table resource limit to reject the document".into());
    };
    let Error::Parse(location, message) = error else {
        return Err(format!("unexpected parse error: {error:?}").into());
    };
    assert_eq!(message, expected_message);
    assert_eq!(location.location.start.line, expected_line);
    Ok(())
}

fn psv_table_with_rows(rows: usize, columns: usize, row: &str) -> String {
    let mut input = format!("[cols=\"{columns}*\"]\n|===\n");
    for _ in 0..rows {
        input.push_str(row);
        input.push('\n');
    }
    input.push_str("|===\n");
    input
}

fn csv_table_with_columns(columns: usize) -> String {
    let row = std::iter::repeat_n("cell", columns)
        .collect::<Vec<_>>()
        .join(",");
    format!(",===\n{row}\n,===\n")
}

#[test]
fn column_multiplier_accepts_one_hundred_and_rejects_the_next_column() -> TestResult {
    assert_parses("[cols=\"100*\"]\n|===\n| A\n|===\n")?;
    assert_table_limit(
        "[cols=\"101*\"]\n|===\n| A\n|===\n",
        "table column count request of 101 exceeds the maximum of 100",
        2,
    )
}

#[test]
fn overflowing_column_multiplier_is_rejected_instead_of_falling_back() -> TestResult {
    let oversized = format!("{}0", usize::MAX);
    let input = format!("[cols=\"{oversized}*\"]\n|===\n| A\n|===\n");

    assert_table_limit(
        &input,
        "table column count request of 101 exceeds the maximum of 100",
        2,
    )
}

#[test]
fn cell_duplication_accepts_one_hundred_and_rejects_the_next_copy() -> TestResult {
    assert_parses("|===\n100*| Same\n|===\n")?;
    assert_table_limit(
        "|===\n101*| Same\n|===\n",
        "table cell duplication request of 101 exceeds the maximum of 100",
        2,
    )
}

#[test]
fn column_span_accepts_one_hundred_and_rejects_the_next_column() -> TestResult {
    assert_parses("|===\n100+| Spans\n|===\n")?;
    assert_table_limit(
        "|===\n101+| Too wide\n|===\n",
        "table column span request of 101 exceeds the maximum of 100",
        2,
    )
}

#[test]
fn row_span_accepts_one_thousand_and_rejects_the_next_row() -> TestResult {
    assert_parses("|===\n.1000+| Spans\n|===\n")?;
    assert_table_limit(
        "|===\n.1001+| Too tall\n|===\n",
        "table row span request of 1001 exceeds the maximum of 1000",
        2,
    )
}

#[test]
fn table_accepts_one_thousand_rows_and_rejects_the_next_row() -> TestResult {
    assert_parses(&psv_table_with_rows(1_000, 1, "| Cell"))?;
    assert_table_limit(
        &psv_table_with_rows(1_001, 1, "| Cell"),
        "table row count request of 1001 exceeds the maximum of 1000",
        1003,
    )
}

#[test]
fn csv_row_obeys_the_same_one_hundred_column_limit() -> TestResult {
    assert_parses(&csv_table_with_columns(100))?;
    assert_table_limit(
        &csv_table_with_columns(101),
        "table column count request of 101 exceeds the maximum of 100",
        2,
    )
}

#[test]
fn table_accepts_the_full_one_hundred_thousand_cell_boundary() -> TestResult {
    assert_parses(&psv_table_with_rows(1_000, 100, "100*| Same"))
}
