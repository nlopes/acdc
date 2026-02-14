use std::io::{self, BufWriter};

use acdc_converters_core::table::calculate_column_widths;
use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{ColumnStyle, HorizontalAlignment};
use comfy_table::{
    Attribute, Cell, CellAlignment, ColumnConstraint, ContentArrangement, Table, Width,
};

use crate::{Error, Processor, TerminalVisitor};

/// Map `HorizontalAlignment` to comfy-table `CellAlignment`.
fn map_alignment(align: HorizontalAlignment) -> CellAlignment {
    match align {
        HorizontalAlignment::Left => CellAlignment::Left,
        HorizontalAlignment::Center => CellAlignment::Center,
        HorizontalAlignment::Right => CellAlignment::Right,
    }
}

/// Render a cell's content to a string, applying column style formatting.
fn render_cell_content(
    col: &acdc_parser::TableColumn,
    col_index: usize,
    columns: &[acdc_parser::ColumnFormat],
    processor: &Processor,
) -> Result<Cell, Error> {
    let buffer = Vec::new();
    let inner = BufWriter::new(buffer);
    let mut temp_visitor = TerminalVisitor::new(inner, processor.clone());
    col.content
        .iter()
        .try_for_each(|block| temp_visitor.visit_block(block))?;
    let buffer = temp_visitor
        .into_writer()
        .into_inner()
        .map_err(io::IntoInnerError::into_error)?;
    let text = String::from_utf8(buffer).unwrap_or_default();

    let mut cell = Cell::new(text);

    // Determine effective alignment: cell override > column default
    let effective_halign = col
        .halign
        .or_else(|| columns.get(col_index).map(|c| c.halign));
    if let Some(align) = effective_halign {
        cell = cell.set_alignment(map_alignment(align));
    }

    // Determine effective style: cell override > column default
    let effective_style = col
        .style
        .or_else(|| columns.get(col_index).map(|c| c.style));
    if let Some(style) = effective_style {
        match style {
            ColumnStyle::Strong => {
                cell = cell.add_attribute(Attribute::Bold);
            }
            ColumnStyle::Emphasis => {
                cell = cell.add_attribute(Attribute::Italic);
            }
            ColumnStyle::Header => {
                cell = cell
                    .fg(processor.appearance.colors.table_header)
                    .add_attribute(Attribute::Bold);
            }
            // Default, AsciiDoc, Literal, Monospace: no extra styling
            // (terminal is already monospace, literal just means no inline processing)
            ColumnStyle::Default
            | ColumnStyle::AsciiDoc
            | ColumnStyle::Literal
            | ColumnStyle::Monospace
            | _ => {}
        }
    }

    Ok(cell)
}

pub(crate) fn visit_table<V: WritableVisitor<Error = Error>>(
    tbl: &acdc_parser::Table,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    let terminal_width = processor.terminal_width;

    let mut table_widget = Table::new();
    table_widget
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(u16::try_from(terminal_width).unwrap_or(80))
        .load_preset(comfy_table::presets::UTF8_FULL)
        .apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS)
        .enforce_styling();

    // Apply proportional column widths if specified
    if !tbl.columns.is_empty() {
        let widths = calculate_column_widths(&tbl.columns);
        let constraints: Vec<ColumnConstraint> = widths
            .iter()
            .map(|width| {
                let clamped = width.clamp(0.0, 100.0).round();
                format!("{clamped:.0}")
                    .parse::<u16>()
                    .ok()
                    .filter(|&p| p > 0)
                    .map_or(ColumnConstraint::ContentWidth, |percent| {
                        ColumnConstraint::Absolute(Width::Percentage(percent))
                    })
            })
            .collect();
        table_widget.set_constraints(constraints);
    }

    if let Some(header) = &tbl.header {
        let header_cells = header
            .columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let mut cell = render_cell_content(col, i, &tbl.columns, processor)?;
                // Headers always get bold + header color
                cell = cell
                    .fg(processor.appearance.colors.table_header)
                    .add_attribute(Attribute::Bold);
                Ok(cell)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        table_widget.set_header(header_cells);
    }

    for (row_idx, row) in tbl.rows.iter().enumerate() {
        let cells = row
            .columns
            .iter()
            .enumerate()
            .map(|(col_idx, col)| {
                let mut cell = render_cell_content(col, col_idx, &tbl.columns, processor)?;
                // Alternating row shading for readability
                if row_idx % 2 == 1 {
                    cell = cell.add_attribute(Attribute::Dim);
                }
                Ok(cell)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        table_widget.add_row(cells);
    }

    if let Some(footer) = &tbl.footer {
        let footer_cells = footer
            .columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let mut cell = render_cell_content(col, i, &tbl.columns, processor)?;
                // Footers always get bold + footer color
                cell = cell
                    .fg(processor.appearance.colors.table_footer)
                    .add_attribute(Attribute::Bold);
                Ok(cell)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        table_widget.add_row(footer_cells);
    }

    let w = visitor.writer_mut();
    writeln!(w, "{table_widget}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_converters_core::Options;
    use acdc_parser::{Block, DelimitedBlockType, DocumentAttributes};

    /// Parse an `AsciiDoc` string and extract the first table from the document.
    #[allow(clippy::expect_used)]
    fn parse_table(adoc: &str) -> acdc_parser::Table {
        let options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(adoc, &options).expect("Failed to parse AsciiDoc");
        doc.blocks
            .into_iter()
            .find_map(|block| {
                if let Block::DelimitedBlock(db) = block
                    && let DelimitedBlockType::DelimitedTable(table) = db.inner
                {
                    return Some(table);
                }
                None
            })
            .expect("No table found in document")
    }

    /// Create test processor with default options
    fn create_test_processor() -> Processor {
        use crate::Appearance;
        use acdc_converters_core::section::{
            AppendixTracker, PartNumberTracker, SectionNumberTracker,
        };
        use std::{cell::Cell, rc::Rc};
        let options = Options::default();
        let document_attributes = DocumentAttributes::default();
        let appearance = Appearance::detect();
        let section_number_tracker = SectionNumberTracker::new(&document_attributes);
        let part_number_tracker =
            PartNumberTracker::new(&document_attributes, section_number_tracker.clone());
        let appendix_tracker =
            AppendixTracker::new(&document_attributes, section_number_tracker.clone());
        Processor {
            options,
            document_attributes,
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
            appearance,
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            terminal_width: crate::FALLBACK_TERMINAL_WIDTH,
            index_entries: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            has_valid_index_section: false,
            list_indent: std::rc::Rc::new(std::cell::Cell::new(0)),
        }
    }

    #[test]
    fn test_table_with_footer() -> Result<(), Error> {
        let adoc = r"
[%header%footer]
|===
| Header 1 | Header 2

| Cell 1 | Cell 2

| Footer 1 | Footer 2
|===
";
        let table = parse_table(adoc);

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = crate::TerminalVisitor::new(buffer, processor.clone());

        visit_table(&table, &mut visitor, &processor)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Header 1"),
            "Output should contain header"
        );
        assert!(
            output_str.contains("Cell 1"),
            "Output should contain body cell"
        );
        assert!(
            output_str.contains("Footer 1"),
            "Output should contain footer"
        );

        Ok(())
    }

    #[test]
    fn test_table_without_footer() -> Result<(), Error> {
        let adoc = r"
|===
| Cell
|===
";
        let table = parse_table(adoc);

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = crate::TerminalVisitor::new(buffer, processor.clone());

        visit_table(&table, &mut visitor, &processor)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Cell"), "Output should contain cell");

        Ok(())
    }

    #[test]
    fn test_table_with_alignment() -> Result<(), Error> {
        let adoc = r#"
[cols="<,^,>"]
|===
| Left | Center | Right
|===
"#;
        let table = parse_table(adoc);

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = crate::TerminalVisitor::new(buffer, processor.clone());

        visit_table(&table, &mut visitor, &processor)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Left"), "Should contain left cell");
        assert!(output_str.contains("Center"), "Should contain center cell");
        assert!(output_str.contains("Right"), "Should contain right cell");

        Ok(())
    }

    #[test]
    fn test_table_with_column_styles() -> Result<(), Error> {
        let adoc = r#"
[cols="s,e"]
|===
| Strong Column | Emphasis Column
|===
"#;
        let table = parse_table(adoc);

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = crate::TerminalVisitor::new(buffer, processor.clone());

        visit_table(&table, &mut visitor, &processor)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Strong Column"),
            "Should contain strong cell"
        );
        assert!(
            output_str.contains("Emphasis Column"),
            "Should contain emphasis cell"
        );

        Ok(())
    }
}
