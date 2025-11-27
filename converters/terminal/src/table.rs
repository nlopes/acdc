use std::io::{self, BufWriter};

use acdc_converters_common::visitor::{Visitor, WritableVisitor};
use comfy_table::{Attribute, Cell, ContentArrangement, Table};

use crate::{Error, Processor, TerminalVisitor};

pub(crate) fn visit_table<V: WritableVisitor<Error = Error>>(
    tbl: &acdc_parser::Table,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    let mut table_widget = Table::new();
    table_widget
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(80)
        .load_preset(comfy_table::presets::UTF8_FULL)
        .apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS)
        .enforce_styling();

    if let Some(header) = &tbl.header {
        let header_cells = header
            .columns
            .iter()
            .map(|col| {
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
                Ok(Cell::new(String::from_utf8(buffer).unwrap_or_default())
                    .fg(processor.appearance.colors.table_header)
                    .add_attribute(Attribute::Bold))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        table_widget.set_header(header_cells);
    }

    for row in &tbl.rows {
        let cells = row
            .columns
            .iter()
            .map(|col| {
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
                Ok(Cell::new(String::from_utf8(buffer).unwrap_or_default()))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        table_widget.add_row(cells);
    }

    if let Some(footer) = &tbl.footer {
        let footer_cells = footer
            .columns
            .iter()
            .map(|col| {
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
                Ok(Cell::new(String::from_utf8(buffer).unwrap_or_default())
                    .fg(processor.appearance.colors.table_footer)
                    .add_attribute(Attribute::Bold))
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
    use acdc_converters_common::Options;
    use acdc_parser::{
        Block, BlockMetadata, DocumentAttributes, InlineNode, Location, Paragraph, Plain,
        TableColumn, TableRow,
    };

    /// Create simple plain text inline nodes for testing
    fn create_test_inlines(content: &str) -> Vec<InlineNode> {
        vec![InlineNode::PlainText(Plain {
            content: content.to_string(),
            location: Location::default(),
        })]
    }

    /// Create test processor with default options
    fn create_test_processor() -> Processor {
        use crate::Appearance;
        use std::{cell::Cell, rc::Rc};
        let options = Options::default();
        let document_attributes = DocumentAttributes::default();
        let appearance = Appearance::detect();
        Processor {
            options,
            document_attributes,
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
            appearance,
        }
    }

    #[test]
    fn test_table_with_footer() -> Result<(), Error> {
        let table = acdc_parser::Table {
            header: Some(TableRow {
                columns: vec![
                    TableColumn {
                        content: vec![Block::Paragraph(Paragraph {
                            metadata: BlockMetadata::default(),
                            title: Vec::new(),
                            content: create_test_inlines("Header 1"),
                            location: Location::default(),
                        })],
                    },
                    TableColumn {
                        content: vec![Block::Paragraph(Paragraph {
                            metadata: BlockMetadata::default(),
                            title: Vec::new(),
                            content: create_test_inlines("Header 2"),
                            location: Location::default(),
                        })],
                    },
                ],
            }),
            rows: vec![TableRow {
                columns: vec![
                    TableColumn {
                        content: vec![Block::Paragraph(Paragraph {
                            metadata: BlockMetadata::default(),
                            title: Vec::new(),
                            content: create_test_inlines("Cell 1"),
                            location: Location::default(),
                        })],
                    },
                    TableColumn {
                        content: vec![Block::Paragraph(Paragraph {
                            metadata: BlockMetadata::default(),
                            title: Vec::new(),
                            content: create_test_inlines("Cell 2"),
                            location: Location::default(),
                        })],
                    },
                ],
            }],
            footer: Some(TableRow {
                columns: vec![
                    TableColumn {
                        content: vec![Block::Paragraph(Paragraph {
                            metadata: BlockMetadata::default(),
                            title: Vec::new(),
                            content: create_test_inlines("Footer 1"),
                            location: Location::default(),
                        })],
                    },
                    TableColumn {
                        content: vec![Block::Paragraph(Paragraph {
                            metadata: BlockMetadata::default(),
                            title: Vec::new(),
                            content: create_test_inlines("Footer 2"),
                            location: Location::default(),
                        })],
                    },
                ],
            }),
            columns: Vec::new(),
            location: Location::default(),
        };

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
        let table = acdc_parser::Table {
            header: None,
            rows: vec![TableRow {
                columns: vec![TableColumn {
                    content: vec![Block::Paragraph(Paragraph {
                        metadata: BlockMetadata::default(),
                        title: Vec::new(),
                        content: create_test_inlines("Cell"),
                        location: Location::default(),
                    })],
                }],
            }],
            footer: None,
            columns: Vec::new(),
            location: Location::default(),
        };

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = crate::TerminalVisitor::new(buffer, processor.clone());

        visit_table(&table, &mut visitor, &processor)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Cell"), "Output should contain cell");

        Ok(())
    }
}
