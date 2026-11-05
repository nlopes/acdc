mod grid;

use serde_json::{Value, json};

use crate::{
    Block, ColumnFormat, ColumnStyle, ColumnWidth, DelimitedBlock, Table, TableColumn, TableFrame,
    TableGrid, TablePresentation, TableRow, TableStripes,
};

use super::{Error, Export, Node, Profile, diagnostics, metadata, node, pointer, unsupported};

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
        let formats = table
            .columns
            .iter()
            .enumerate()
            .map(|(index, format)| {
                column_format(
                    format,
                    &pointer(&pointer(path, "columns"), &index.to_string()),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
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
        if !cell.initial_attributes.is_empty() {
            return Err(unsupported(
                &pointer(path, "initial_attributes"),
                "nested cell attribute scopes have no supported mapping",
            ));
        }
        let mut result = Node::new();
        for (name, span) in [("colspan", cell.colspan), ("rowspan", cell.rowspan)] {
            if span != 1 {
                result.insert(name.into(), json!(span));
            }
        }
        if let Some(style) = cell.style {
            result.insert(
                "style".into(),
                style_name(style, &pointer(path, "style"))?.into(),
            );
        }
        if let Some(halign) = cell.halign {
            result.insert("halign".into(), json!(halign));
        }
        if let Some(valign) = cell.valign {
            result.insert("valign".into(), json!(valign));
        }
        let literal =
            cell.style.or_else(|| format.map(|format| format.style)) == Some(ColumnStyle::Literal);
        let blocks = cell
            .content
            .iter()
            .enumerate()
            .map(|(index, child)| {
                let at = pointer(&pointer(path, "blocks"), &index.to_string());
                let Block::Paragraph(paragraph) = child else {
                    return Err(
                        unsupported(&at, "nested cell blocks have no supported mapping")
                            .with_context(diagnostics::block_construct(child), child.location()),
                    );
                };
                let mut value = self.paragraph(paragraph, &at, literal)?;
                metadata::location(&mut value, child.location(), &at)?;
                Ok(Value::Object(value))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        result.insert("blocks".into(), Value::Array(blocks));
        Ok(Value::Object(result))
    }
}

fn column_format(value: &ColumnFormat, path: &str) -> Result<Value, Error> {
    let width = match value.width {
        ColumnWidth::Proportional(value) => json!({"kind":"proportional","value":value}),
        ColumnWidth::Percentage(value) => json!({"kind":"percentage","value":value}),
        ColumnWidth::Auto => json!({"kind":"auto"}),
    };
    let style = style_name(value.style, &pointer(path, "style"))?;
    Ok(json!({"halign":value.halign,"valign":value.valign,"width":width,"style":style}))
}

fn style_name(style: ColumnStyle, path: &str) -> Result<&'static str, Error> {
    match style {
        ColumnStyle::Default => Ok("default"),
        ColumnStyle::Emphasis => Ok("emphasis"),
        ColumnStyle::Header => Ok("header"),
        ColumnStyle::Literal => Ok("literal"),
        ColumnStyle::Monospace => Ok("monospace"),
        ColumnStyle::Strong => Ok("strong"),
        ColumnStyle::AsciiDoc => Err(unsupported(
            path,
            "AsciiDoc cell scopes have no supported mapping",
        )),
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
