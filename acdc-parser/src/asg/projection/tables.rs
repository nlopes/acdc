mod grid;

use serde_json::{Value, json};

use crate::{
    Block, ColumnFormat, ColumnStyle, ColumnWidth, DelimitedBlock, Table, TableColumn, TableFrame,
    TableGrid, TablePresentation, TableRow, TableStripes,
};

use super::{
    Error, Export, Node, Profile, attributes, diagnostics, metadata, node, pointer, unsupported,
};

impl<'a, 'doc> Export<'a, 'doc> {
    pub(super) fn table(
        &mut self,
        block: &'a DelimitedBlock<'doc>,
        table: &'a Table<'doc>,
        path: &str,
    ) -> Result<Node, Error> {
        if self.profile == Profile::Upstream {
            return Err(unsupported(
                &pointer(path, "inner"),
                "tables need the named acdc profile",
            ));
        }
        grid::validate(table, path)?;
        let mut result = node("table", "block");
        result.insert("form".into(), "delimited".into());
        result.insert("delimiter".into(), block.delimiter.into());
        let formats = table.columns.iter().map(column_format).collect();
        result.insert("columns".into(), Value::Array(formats));
        if let Some(header) = &table.header {
            result.insert(
                "header".into(),
                self.table_row(header, &table.columns, &pointer(path, "header"))?,
            );
        }
        let rows = table
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                self.table_row(
                    row,
                    &table.columns,
                    &pointer(&pointer(path, "rows"), &index.to_string()),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        result.insert("rows".into(), Value::Array(rows));
        if let Some(footer) = &table.footer {
            result.insert(
                "footer".into(),
                self.table_row(footer, &table.columns, &pointer(path, "footer"))?,
            );
        }
        if let Some(presentation) = table.presentation() {
            result.insert("presentation".into(), table_presentation(presentation));
        }
        metadata::location_property(&mut result, "delimited_location", &table.location, path)?;
        self.block_metadata(&mut result, &block.metadata, Some(&block.title), path)?;
        Ok(result)
    }

    fn table_row(
        &mut self,
        row: &'a TableRow<'doc>,
        formats: &[ColumnFormat],
        path: &str,
    ) -> Result<Value, Error> {
        let cells = row
            .columns
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let at = pointer(&pointer(path, "cells"), &index.to_string());
                self.table_cell(cell, formats.get(index), &at)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({"cells":cells}))
    }

    fn table_cell(
        &mut self,
        cell: &'a TableColumn<'doc>,
        format: Option<&ColumnFormat>,
        path: &str,
    ) -> Result<Value, Error> {
        let mut result = Node::new();
        if let Some(notes) = &cell.footnotes {
            let at = pointer(path, "footnotes");
            super::super::footnotes::validate_scope(
                crate::ast::StructuralRef::TableCell(crate::ast::TableCellRef::from(cell)),
                notes,
                &at,
            )?;
            result.insert(
                "footnotes".into(),
                Value::Array(super::super::footnotes::project_definitions(
                    notes,
                    &at,
                    self.profile,
                )?),
            );
        }
        for (name, span) in [("colspan", cell.colspan), ("rowspan", cell.rowspan)] {
            if span != 1 {
                result.insert(name.into(), json!(span));
            }
        }
        if let Some(style) = cell.style {
            result.insert("style".into(), style_name(style).into());
        }
        if let Some(halign) = cell.halign {
            result.insert("halign".into(), json!(halign));
        }
        if let Some(valign) = cell.valign {
            result.insert("valign".into(), json!(valign));
        }
        let literal =
            cell.style.or_else(|| format.map(|format| format.style)) == Some(ColumnStyle::Literal);
        let ascii_doc =
            cell.style.or_else(|| format.map(|format| format.style)) == Some(ColumnStyle::AsciiDoc);
        if !cell.initial_attributes.is_empty() {
            if !ascii_doc {
                return Err(unsupported(
                    &pointer(path, "initial_attributes"),
                    "cell initial attributes require AsciiDoc style",
                ));
            }
            let initial = cell.initial_attributes.iter().enumerate().map(|(index, (name, value))| {
                let at = pointer(&pointer(path, "initial_attributes"), &index.to_string());
                Ok(json!({"attribute":name.as_ref(), "assignment":attributes::assignment(value, &pointer(&at, "assignment"))?}))
            }).collect::<Result<Vec<_>, Error>>()?;
            result.insert("initial_attributes".into(), Value::Array(initial));
        }
        let blocks = if ascii_doc {
            self.blocks(&cell.content, &pointer(path, "blocks"))?
        } else {
            let blocks = cell
                .content
                .iter()
                .enumerate()
                .map(|(index, child)| {
                    let at = pointer(&pointer(path, "blocks"), &index.to_string());
                    let Block::Paragraph(paragraph) = child else {
                        return Err(unsupported(
                            &at,
                            "nested cell blocks have no supported mapping",
                        )
                        .with_context(diagnostics::block_construct(child), child.location()));
                    };
                    let mut value = self.paragraph(paragraph, &at, literal)?;
                    metadata::location(&mut value, child.location(), &at)?;
                    Ok(Value::Object(value))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            Value::Array(blocks)
        };
        result.insert("blocks".into(), blocks);
        Ok(Value::Object(result))
    }
}

fn column_format(value: &ColumnFormat) -> Value {
    let width = match value.width {
        ColumnWidth::Proportional(value) => json!({"kind":"proportional","value":value}),
        ColumnWidth::Percentage(value) => json!({"kind":"percentage","value":value}),
        ColumnWidth::Auto => json!({"kind":"auto"}),
    };
    let style = style_name(value.style);
    json!({"halign":value.halign,"valign":value.valign,"width":width,"style":style})
}

fn style_name(style: ColumnStyle) -> &'static str {
    match style {
        ColumnStyle::Default => "default",
        ColumnStyle::Emphasis => "emphasis",
        ColumnStyle::Header => "header",
        ColumnStyle::Literal => "literal",
        ColumnStyle::Monospace => "monospace",
        ColumnStyle::Strong => "strong",
        ColumnStyle::AsciiDoc => "asciidoc",
    }
}

fn table_presentation(value: TablePresentation) -> Value {
    let frame = match value.frame() {
        TableFrame::All => "all",
        TableFrame::Ends => "ends",
        TableFrame::Sides => "sides",
        TableFrame::None => "none",
    };
    let grid = match value.grid() {
        TableGrid::All => "all",
        TableGrid::Rows => "rows",
        TableGrid::Columns => "columns",
        TableGrid::None => "none",
    };
    let stripes = match value.stripes() {
        TableStripes::None => "none",
        TableStripes::All => "all",
        TableStripes::Odd => "odd",
        TableStripes::Even => "even",
        TableStripes::Hover => "hover",
    };
    json!({"frame":frame,"grid":grid,"stripes":stripes})
}
