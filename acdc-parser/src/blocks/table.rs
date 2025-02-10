use pest::iterators::Pair;

use crate::{
    AttributeValue, BlockMetadata, DocumentAttributes, ElementAttributes, Error, Location, Options,
    Rule, Table, TableColumn, TableRow,
};

impl Table {
    pub(crate) fn parse(
        pair: &Pair<Rule>,
        options: &Options,
        metadata: &BlockMetadata,
        attributes: &ElementAttributes,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Self, Error> {
        let mut separator = "|".to_string();
        if let Some(AttributeValue::String(format)) = attributes.get("format") {
            separator = match format.as_str() {
                "csv" => ",".to_string(),
                "dsv" => ":".to_string(),
                "tsv" => "\t".to_string(),
                format => unimplemented!("unkown table format: {format}"),
            };
        }
        // override the separator if it is provided in the document
        separator = attributes
            .get("separator")
            .unwrap_or(&AttributeValue::String(separator.clone()))
            .to_string();

        let ncols = if let Some(AttributeValue::String(cols)) = attributes.get("cols") {
            Some(cols.split(',').count())
        } else {
            None
        };

        // Set this to true if the user mandates it!
        let mut has_header = metadata.options.contains(&String::from("header"));

        let raw_rows = Self::parse_rows(pair.as_str(), &separator, &mut has_header);

        // If the user forces a noheader, we should not have a header, so after we've
        // tried to figure out if there are any headers, we should set it to false one
        // last time.
        if metadata.options.contains(&String::from("noheader")) {
            has_header = false;
        }
        let has_footer = metadata.options.contains(&String::from("footer"));

        let mut header = None;
        let mut footer = None;
        let mut rows = Vec::new();

        for (i, row) in raw_rows.iter().enumerate() {
            let columns = row
                .iter()
                .filter(|cell| !cell.is_empty())
                .map(|cell| parse_table_cell(cell, options, parent_attributes))
                .collect::<Result<Vec<_>, _>>()?;

            // validate that if we have ncols we have the same number of columns in each row
            if let Some(ncols) = ncols {
                if columns.len() != ncols {
                    return Err(Error::Parse(format!(
                        "expected table row with {ncols} columns, found {} columns",
                        columns.len()
                    )));
                }
            }

            // if we have a header, we need to add the columns we have to the header
            if has_header {
                header = Some(TableRow { columns });
                has_header = false;
                continue;
            }

            // if we have a footer, we need to add the columns we have to the footer
            if has_footer && i == raw_rows.len() - 1 {
                footer = Some(TableRow { columns });
                continue;
            }

            // if we get here, these columns are a row
            rows.push(TableRow { columns });
        }

        Ok(Self {
            header,
            footer,
            rows,
            location: Location::default(),
        })
    }

    fn parse_rows(text: &str, separator: &str, has_header: &mut bool) -> Vec<Vec<String>> {
        let mut location = Location::default();

        let mut rows = Vec::new();
        let mut row_string = String::new();
        for (i, row) in text.lines().enumerate() {
            let row = row.trim();
            // If we are in the first row and it is empty, we should not have a header,
            // set it to false and move on.
            if i == 0 && row.is_empty() {
                *has_header = false;
                continue;
            }

            // If we're in the first row and it is empty, and we've already added
            // something to the rows, then we should have a header
            if i == 1 && row.is_empty() {
                *has_header = true;
            }

            if row.is_empty() && !row_string.is_empty() {
                let columns = row_string
                    .split(separator)
                    .map(str::trim)
                    .map(str::to_string)
                    .collect();
                row_string.clear();
                rows.push(columns);
            }

            // Adjust the location
            if row_string.is_empty() {
                location.start.line = i + 1;
                location.start.column = 1;
            }
            location.end.line = i + 1;
            location.end.column = row.len() + 1;

            // Add the row to the row string
            row_string.push_str(row);
        }
        if !row_string.is_empty() {
            let columns = row_string
                .split(separator)
                .map(str::trim)
                .map(str::to_string)
                .collect();
            rows.push(columns);
        }
        rows
    }
}

fn parse_table_cell(
    text: &str,
    options: &Options,
    parent_attributes: &mut DocumentAttributes,
) -> Result<TableColumn, Error> {
    use pest::Parser as _;

    let parse = crate::InnerPestParser::parse(Rule::block, text)
        .map_err(|e| Error::Parse(format!("error parsing table cell: {e}")))?;
    let content = crate::blocks::parse(
        parse,
        options,
        Some(&Location::default()),
        parent_attributes,
    )?;

    Ok(TableColumn { content })
}
