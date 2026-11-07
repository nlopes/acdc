//! Public typed attribute inputs preserve replay, borrowing, and nested scopes.

use std::ptr;

use acdc_converters_core::TraversalContext;
use acdc_parser::{
    CompatibilityMode, DocumentAttributeValue, Options, TableFrame, TableGrid, TablePresentation,
    ast::{BlockKind, DocumentAttributesRef, TableCellRef, TableRowRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn typed_attribute_replay_preserves_assignment_identity_and_header_values() -> Result<(), Error> {
    let options = Options::builder()
        .with_attribute("max-include-depth", "03")
        .build()?;
    let parsed = parse(
        "= Header\n:name: header\n:present:\n\nBody.\n\n:name: first\n:name!:\n:name: final\n",
        &options,
    )?;
    let document = parsed.document();
    let mut context = TraversalContext::new(document.attributes());
    let header: DocumentAttributesRef<'_> = context.header();
    let depth = context.get("max-include-depth").ok_or("depth")?;
    assert_eq!(depth.as_integer(), Some(3));
    assert_eq!(depth.text(), Some("03"));
    assert!(ptr::eq(
        depth,
        header.get("max-include-depth").ok_or("header depth")?
    ));
    assert!(
        context
            .get("present")
            .is_some_and(DocumentAttributeValue::is_presence)
    );
    let mut values = Vec::new();
    for block in document.blocks() {
        if let BlockKind::DocumentAttribute(event) = block.kind() {
            context.apply(event);
            assert!(ptr::eq(
                context
                    .attribute_assignment(event.name())
                    .ok_or("assignment")?,
                event.assignment()
            ));
            values.push(context.get("name").and_then(DocumentAttributeValue::text));
        }
    }
    assert_eq!(values, [Some("first"), None, Some("final")]);
    assert_eq!(
        context.get("name").and_then(DocumentAttributeValue::text),
        Some("final")
    );
    assert_eq!(
        header.get("name").and_then(DocumentAttributeValue::text),
        Some("header")
    );
    assert!(ptr::eq(
        header.get("name").ok_or("header name")?,
        document.attributes().get("name").ok_or("document name")?
    ));
    Ok(())
}

fn replay_cell<'doc>(context: &mut TraversalContext<'doc>, cell: TableCellRef<'doc>) {
    for block in cell.blocks() {
        if let BlockKind::DocumentAttribute(event) = block.kind() {
            context.apply(event);
        }
    }
}

#[test]
fn typed_nested_cells_restore_parent_state_after_success_and_error() -> Result<(), Error> {
    let parsed = parse(
        "= Doc\n:doctype: book\n:toc: left\n:imagesdir: root\n\nBody.\n\n:imagesdir: parent\n\n[cols=\"a,a\"]\n|===\n|\nOne.\n\n:cell-value: one\n\nLater.\n|\nTwo.\n\n:cell-value: two\n\nLater.\n|===\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let table = document
        .blocks()
        .find_map(|block| {
            if let BlockKind::Table(table) = block.kind() {
                Some(table)
            } else {
                None
            }
        })
        .ok_or("table")?;
    let mut cells = table.rows().flat_map(TableRowRef::cells);
    let first = cells.next().ok_or("first cell")?;
    let second = cells.next().ok_or("second cell")?;
    for fail in [false, true] {
        let mut context = TraversalContext::new(document.attributes());
        for block in document.blocks() {
            if let BlockKind::DocumentAttribute(event) = block.kind() {
                context.apply(event);
            }
        }
        let result = context.with_table_cell(first, |context| -> Result<(), &str> {
            replay_cell(context, first);
            assert!(context.is_nested_document());
            assert_eq!(
                context
                    .get("doctype")
                    .and_then(DocumentAttributeValue::as_str),
                Some("article")
            );
            assert!(!context.contains_key("toc"));
            assert_eq!(context.substitute_attributes("{cell-value}"), "one");
            let nested = context.with_table_cell(second, |context| -> Result<(), &str> {
                replay_cell(context, second);
                assert_eq!(context.substitute_attributes("{cell-value}"), "two");
                if fail {
                    Err("nested conversion failed")
                } else {
                    Ok(())
                }
            });
            assert_eq!(nested.is_err(), fail);
            assert_eq!(context.substitute_attributes("{cell-value}"), "one");
            if fail {
                Err("conversion failed")
            } else {
                Ok(())
            }
        });
        assert_eq!(result.is_err(), fail);
        assert!(!context.is_nested_document());
        assert!(!context.contains_key("cell-value"));
        assert_eq!(context.substitute_attributes("{imagesdir}"), "parent");
        assert_eq!(
            context
                .get("doctype")
                .and_then(DocumentAttributeValue::as_str),
            Some("book")
        );
        assert!(
            context
                .get("toc")
                .is_some_and(DocumentAttributeValue::is_presence)
        );
        assert_eq!(
            context
                .header()
                .get("imagesdir")
                .and_then(DocumentAttributeValue::as_str),
            Some("root")
        );
    }
    Ok(())
}

#[test]
fn table_decoration_lookup_follows_nested_scopes_and_restores_body_defaults() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "= Doc\n:table-frame: all\n\nBody.\n\n:table-frame: sides\n\n[cols=\"a,a\"]\n|===\n|\nOne.\n\n:table-grid: cols\n\nLater.\n|\nTwo.\n\n:table-grid: rows\n\nLater.\n|===\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let document = parsed.document();
        let table = document
            .blocks()
            .find_map(|block| {
                if let BlockKind::Table(table) = block.kind() {
                    Some(table)
                } else {
                    None
                }
            })
            .ok_or("table")?;
        let mut cells = table.rows().flat_map(TableRowRef::cells);
        let first = cells.next().ok_or("first cell")?;
        let second = cells.next().ok_or("second cell")?;
        let mut context = TraversalContext::new(document.attributes());
        for block in document.blocks() {
            if let BlockKind::DocumentAttribute(event) = block.kind() {
                context.apply(event);
            }
        }
        let grid = |context: &TraversalContext<'_>| {
            TablePresentation::resolve_with(None, |name| context.get(name)).grid()
        };
        assert_eq!(
            TablePresentation::resolve_with(None, |name| context.get(name)).frame(),
            TableFrame::Sides
        );
        assert_eq!(grid(&context), TableGrid::All);
        let result = context.with_table_cell(first, |context| {
            replay_cell(context, first);
            assert_eq!(grid(context), TableGrid::Columns);
            let nested = context.with_table_cell(second, |context| {
                replay_cell(context, second);
                assert_eq!(grid(context), TableGrid::Rows);
                Err::<(), _>("nested conversion failed")
            });
            assert!(nested.is_err());
            assert_eq!(grid(context), TableGrid::Columns);
            Err::<(), _>("conversion failed")
        });
        assert!(result.is_err());
        assert_eq!(grid(&context), TableGrid::All);
        assert_eq!(
            TablePresentation::resolve(None, context.header()).grid(),
            TableGrid::All
        );
        assert_eq!(
            TablePresentation::resolve(None, context.header()).frame(),
            TableFrame::All
        );
    }
    Ok(())
}
