use crate::{Table, TableRow};

use super::{Error, pointer, unsupported};

struct Occupied {
    start: usize,
    end: usize,
    until: usize,
}

pub(super) fn validate(table: &Table<'_>, path: &str) -> Result<(), Error> {
    let count = table.rows.len()
        + usize::from(table.header.is_some())
        + usize::from(table.footer.is_some());
    let mut occupied = Vec::new();
    let rows = table
        .header
        .iter()
        .map(|row| (row, pointer(path, "header")))
        .chain(
            table
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| (row, pointer(&pointer(path, "rows"), &index.to_string()))),
        )
        .chain(
            table
                .footer
                .iter()
                .map(|row| (row, pointer(path, "footer"))),
        );
    for (index, (row, at)) in rows.enumerate() {
        validate_row(row, index, count, table.columns.len(), &mut occupied, &at)?;
    }
    Ok(())
}

fn validate_row(
    row: &TableRow<'_>,
    index: usize,
    count: usize,
    width: usize,
    occupied: &mut Vec<Occupied>,
    path: &str,
) -> Result<(), Error> {
    occupied.retain(|span| span.until > index);
    let mut column = 0;
    for (ordinal, cell) in row.columns.iter().enumerate() {
        let at = pointer(&pointer(path, "cells"), &ordinal.to_string());
        for (name, span) in [("colspan", cell.colspan), ("rowspan", cell.rowspan)] {
            if span == 0 {
                return Err(unsupported(
                    &pointer(&at, name),
                    "table spans must be positive",
                ));
            }
        }
        if cell.rowspan > count - index {
            return Err(unsupported(
                &pointer(&at, "rowspan"),
                "table span exceeds retained rows",
            ));
        }
        while let Some(span) = occupied
            .iter()
            .find(|span| span.start <= column && column < span.end)
        {
            column = span.end;
        }
        let end = column
            .checked_add(cell.colspan)
            .ok_or_else(|| unsupported(&pointer(&at, "colspan"), "table column span overflows"))?;
        if width != 0 && end > width {
            return Err(unsupported(
                &pointer(&at, "colspan"),
                "table cell exceeds declared width",
            ));
        }
        if occupied
            .iter()
            .any(|span| column < span.end && span.start < end)
        {
            return Err(unsupported(
                &pointer(&at, "colspan"),
                "table cell overlaps an earlier row span",
            ));
        }
        occupied.push(Occupied {
            start: column,
            end,
            until: index + cell.rowspan,
        });
        column = end;
    }
    if width != 0 && !row.columns.is_empty() {
        while let Some(span) = occupied
            .iter()
            .find(|span| span.start <= column && column < span.end)
        {
            column = span.end;
        }
        if column != width {
            return Err(unsupported(
                &pointer(path, "cells"),
                "table row does not fill declared width",
            ));
        }
    }
    Ok(())
}
