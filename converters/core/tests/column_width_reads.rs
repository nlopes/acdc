//! Column widths retain numeric zero, automatic columns and one shared scale.

use acdc_converters_core::table::column_widths;
use acdc_parser::{
    ColumnFormat,
    ColumnWidth::{Auto, Percentage, Proportional},
    CompatibilityMode, Options,
    ast::{BlockKind, BlockRef, ColumnFormatRef},
    parse,
};

#[test]
fn width_rules_preserve_empty_zero_mixed_and_normalized_values() {
    for (specifications, expected) in [
        (vec![], vec![]),
        (vec![Proportional(1)], vec![Some(100.0)]),
        (
            vec![Proportional(1), Proportional(3)],
            vec![Some(25.0), Some(75.0)],
        ),
        (vec![Auto, Auto], vec![None, None]),
        (
            vec![Proportional(0), Proportional(0)],
            vec![Some(50.0), Some(50.0)],
        ),
        (
            vec![Percentage(0), Percentage(0)],
            vec![Some(50.0), Some(50.0)],
        ),
        (
            vec![Percentage(20), Percentage(30)],
            vec![Some(40.0), Some(60.0)],
        ),
        (
            vec![Percentage(75), Percentage(75)],
            vec![Some(50.0), Some(50.0)],
        ),
        (
            vec![Proportional(3), Proportional(3), Auto],
            vec![Some(3.0), Some(3.0), None],
        ),
        (vec![Proportional(0), Auto], vec![Some(0.0), None]),
        (
            vec![Auto, Proportional(0), Proportional(1), Percentage(25)],
            vec![None, Some(0.0), Some(1.0), Some(25.0)],
        ),
        (
            vec![Proportional(1), Proportional(3), Percentage(100)],
            vec![
                Some(100.0 / 104.0),
                Some(300.0 / 104.0),
                Some(10_000.0 / 104.0),
            ],
        ),
        (
            vec![Proportional(u32::MAX), Proportional(u32::MAX)],
            vec![Some(50.0), Some(50.0)],
        ),
        (
            vec![Proportional(u32::MAX), Percentage(u32::MAX), Auto],
            vec![Some(50.0), Some(50.0), None],
        ),
    ] {
        let columns = specifications
            .into_iter()
            .map(|width| ColumnFormat::new().with_width(width))
            .collect::<Vec<_>>();
        let actual = column_widths(columns.iter().map(ColumnFormatRef::from));
        let mut reversed = actual.clone();
        reversed.reverse();
        assert_eq!(
            column_widths(columns.iter().rev().map(ColumnFormatRef::from)),
            reversed
        );
        assert_eq!(
            column_widths(columns.iter().filter(|_| true).map(ColumnFormatRef::from)),
            actual
        );
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(&expected) {
            assert_eq!(actual.is_some(), expected.is_some());
            if let (Some(actual), Some(expected)) = (actual, expected) {
                assert!((actual - expected).abs() < 1e-10);
                assert!(actual.is_finite() && (0.0..=100.0).contains(actual));
            }
        }
    }
}

#[test]
fn typed_table_widths_outlive_temporary_input() -> Result<(), Box<dyn std::error::Error>> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for (columns, expected) in [
            ("1,3", vec![Some(25.0), Some(75.0)]),
            ("2*3,~", vec![Some(3.0), Some(3.0), None]),
            ("2*4294967295,~", vec![Some(50.0), Some(50.0), None]),
            ("0,0", vec![Some(50.0), Some(50.0)]),
        ] {
            let widths = {
                let source = format!("[cols=\"{columns}\"]\n|===\n|A |B |C\n|===\n");
                let parsed = parse(
                    &source,
                    &Options::builder().with_compatibility_mode(mode).build()?,
                )?;
                let Some(BlockKind::Table(table)) =
                    parsed.document().blocks().next().map(BlockRef::kind)
                else {
                    return Err("expected table".into());
                };
                column_widths(table.columns())
            };
            assert_eq!(widths, expected);
        }
    }
    Ok(())
}

#[test]
fn long_owned_column_lists_do_not_overflow_the_authored_integer_range() {
    let columns = vec![ColumnFormat::new().with_width(Proportional(u32::MAX)); 10_000];
    let widths = column_widths(columns.iter().map(ColumnFormatRef::from));
    assert_eq!(widths.len(), columns.len());
    assert!(
        widths
            .iter()
            .all(|width| width
                .is_some_and(|width| width.is_finite() && (width - 0.01).abs() < 1e-12))
    );
}
