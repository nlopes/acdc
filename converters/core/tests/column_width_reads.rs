//! Column widths retain ordering, automatic widths, and percentage normalization.

use acdc_converters_core::table::{calculate_column_widths, column_widths};
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
        (vec![Proportional(1)], vec![100.0]),
        (vec![Proportional(1), Proportional(3)], vec![25.0, 75.0]),
        (vec![Auto, Auto], vec![0.0, 0.0]),
        (vec![Proportional(0), Proportional(0)], vec![0.0, 0.0]),
        (vec![Percentage(0), Percentage(0)], vec![0.0, 0.0]),
        (vec![Percentage(20), Percentage(30)], vec![40.0, 60.0]),
        (vec![Percentage(75), Percentage(75)], vec![50.0, 50.0]),
        (
            vec![Auto, Proportional(0), Proportional(1), Percentage(25)],
            vec![0.0, 0.0, 80.0, 20.0],
        ),
        (
            vec![Proportional(1), Proportional(3), Percentage(100)],
            vec![12.5, 37.5, 50.0],
        ),
    ] {
        let columns = specifications
            .into_iter()
            .map(|width| ColumnFormat::new().with_width(width))
            .collect::<Vec<_>>();
        let actual = calculate_column_widths(&columns);
        assert_eq!(
            column_widths(columns.iter().map(ColumnFormatRef::from)),
            actual
        );
        assert_eq!(
            column_widths(columns.iter().filter(|_| true).map(ColumnFormatRef::from)),
            actual
        );
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(&expected) {
            assert!((actual - expected).abs() < 1e-10);
        }
    }
}

#[test]
fn typed_table_widths_outlive_temporary_input() -> Result<(), Box<dyn std::error::Error>> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let widths = {
            let source = String::from("[cols=\"1,3\"]\n|===\n|A |B\n|===\n");
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
        assert_eq!(widths, [25.0, 75.0]);
    }
    Ok(())
}
