//! Block callbacks observe attribute events and stop on the first error.

use acdc_converters_core::{TraversalContext, visitor::BlockVisitor};
use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

const SOURCE: &str =
    "= Doc\n:value: header\n\nBefore.\n\n:value: first\n\nAfter.\n\n:value!:\n\nUnset.\n";

#[derive(Default)]
struct ConstructedTrace<'doc> {
    entries: Vec<(&'static str, Option<&'doc str>)>,
    fail_before: bool,
    fail_dispatch: bool,
}

impl<'doc> BlockVisitor<'doc> for ConstructedTrace<'doc> {
    type Error = &'static str;

    fn before_block(
        &mut self,
        context: &mut TraversalContext<'doc>,
        block: BlockRef<'doc>,
    ) -> Result<(), Self::Error> {
        self.entries.push((
            "before",
            context.get("value").and_then(|value| value.text()),
        ));
        if self.fail_before && matches!(block.kind(), BlockKind::DocumentAttribute(_)) {
            return Err("before");
        }
        Ok(())
    }

    fn visit_block(
        &mut self,
        context: &mut TraversalContext<'doc>,
        block: BlockRef<'doc>,
    ) -> Result<(), Self::Error> {
        let phase = if matches!(block.kind(), BlockKind::DocumentAttribute(_)) {
            "event"
        } else if matches!(block.kind(), BlockKind::Paragraph(_)) {
            "paragraph"
        } else {
            "container"
        };
        self.entries
            .push((phase, context.get("value").and_then(|value| value.text())));
        if self.fail_dispatch && phase == "event" {
            Err("dispatch")
        } else {
            Ok(())
        }
    }
}

#[test]
fn constructed_callbacks_see_events_after_before_hooks() -> Result<(), Error> {
    let parsed = parse(SOURCE, &Options::default())?;
    let parsed_blocks: Vec<_> = parsed
        .document()
        .blocks()
        .map(acdc_parser::Block::from)
        .collect();
    let mut context = TraversalContext::new(parsed.document().attributes());
    let mut trace = ConstructedTrace::default();
    context.visit_blocks(&mut trace, &parsed_blocks)?;
    assert_eq!(
        trace.entries,
        [
            ("before", Some("header")),
            ("paragraph", Some("header")),
            ("before", Some("header")),
            ("event", Some("first")),
            ("before", Some("first")),
            ("paragraph", Some("first")),
            ("before", Some("first")),
            ("event", None),
            ("before", None),
            ("paragraph", None),
        ]
    );
    Ok(())
}

#[test]
fn constructed_callback_errors_stop_before_later_blocks() -> Result<(), Error> {
    let parsed = parse(SOURCE, &Options::default())?;
    let parsed_blocks: Vec<_> = parsed
        .document()
        .blocks()
        .map(acdc_parser::Block::from)
        .collect();
    for fail_before in [false, true] {
        let mut context = TraversalContext::new(parsed.document().attributes());
        let mut trace = ConstructedTrace {
            fail_before,
            fail_dispatch: true,
            ..ConstructedTrace::default()
        };
        assert_eq!(
            context.visit_blocks(&mut trace, &parsed_blocks),
            Err(if fail_before { "before" } else { "dispatch" })
        );
        assert_eq!(
            context.get("value").and_then(|value| value.text()),
            Some(if fail_before { "header" } else { "first" })
        );
        assert_eq!(trace.entries.len(), if fail_before { 3 } else { 4 });
    }
    Ok(())
}

#[derive(Default)]
struct TypedTrace<'doc>(ConstructedTrace<'doc>);

impl<'doc> BlockVisitor<'doc> for TypedTrace<'doc> {
    type Error = &'static str;

    fn before_block(
        &mut self,
        context: &mut TraversalContext<'doc>,
        block: BlockRef<'doc>,
    ) -> Result<(), Self::Error> {
        self.0.entries.push((
            "before",
            context.get("value").and_then(|value| value.text()),
        ));
        if self.0.fail_before && matches!(block.kind(), BlockKind::DocumentAttribute(_)) {
            return Err("before");
        }
        Ok(())
    }

    fn visit_block(
        &mut self,
        context: &mut TraversalContext<'doc>,
        block: BlockRef<'doc>,
    ) -> Result<(), Self::Error> {
        let phase = if matches!(block.kind(), BlockKind::DocumentAttribute(_)) {
            "event"
        } else if matches!(block.kind(), BlockKind::Paragraph(_)) {
            "paragraph"
        } else {
            "container"
        };
        self.0
            .entries
            .push((phase, context.get("value").and_then(|value| value.text())));
        if self.0.fail_dispatch && phase == "event" {
            Err("dispatch")
        } else {
            Ok(())
        }
    }
}

#[test]
fn typed_and_constructed_inputs_share_hook_order_and_error_behavior() -> Result<(), Error> {
    let parsed = parse(SOURCE, &Options::default())?;
    let parsed_blocks: Vec<_> = parsed
        .document()
        .blocks()
        .map(acdc_parser::Block::from)
        .collect();
    for (fail_before, fail_dispatch) in [(false, false), (true, false), (false, true)] {
        let mut owner = ConstructedTrace {
            fail_before,
            fail_dispatch,
            ..ConstructedTrace::default()
        };
        let mut typed = TypedTrace(ConstructedTrace {
            fail_before,
            fail_dispatch,
            ..ConstructedTrace::default()
        });
        let mut owner_context = TraversalContext::new(parsed.document().attributes());
        let mut typed_context = TraversalContext::new(parsed.document().attributes());
        let expected = owner_context.visit_blocks(&mut owner, &parsed_blocks);
        let actual = typed_context
            .visit_blocks::<_, _>(&mut typed, parsed_blocks.iter().map(BlockRef::from));
        assert_eq!(actual, expected);
        assert_eq!(typed.0.entries, owner.entries);
        assert!(std::ptr::eq(
            typed_context
                .attribute_assignment("value")
                .ok_or("typed assignment")?,
            owner_context
                .attribute_assignment("value")
                .ok_or("owner assignment")?
        ));
    }
    Ok(())
}

#[test]
fn typed_callbacks_control_recursion_and_support_trait_objects() -> Result<(), Error> {
    let parsed = parse(
        "= Doc\n:value: header\n\n== Section\n\n:value: nested\n\nBody.\n",
        &Options::default(),
    )?;
    let mut context = TraversalContext::new(parsed.document().attributes());
    let mut trace = TypedTrace::default();
    let visitor: &mut dyn BlockVisitor<'_, Error = &'static str> = &mut trace;
    let block = parsed.document().blocks().next().ok_or("section")?;
    context.visit_block::<_, _>(visitor, block)?;
    assert_eq!(
        trace.0.entries,
        [("before", Some("header")), ("container", Some("header"))]
    );
    let BlockKind::Section(section) = block.kind() else {
        return Err("section kind".into());
    };
    context.visit_blocks(&mut trace, section.blocks())?;
    assert_eq!(
        context.get("value").and_then(|value| value.text()),
        Some("nested")
    );
    assert_eq!(trace.0.entries.len(), 6);
    Ok(())
}

#[test]
fn typed_cell_callback_errors_restore_the_parent_scope() -> Result<(), Error> {
    let parsed = parse(
        "= Doc\n\nOutside.\n\n[cols=a]\n|===\n|\n:value: local\n\nInside.\n|===\n",
        &Options::default(),
    )?;
    let table = parsed
        .document()
        .blocks()
        .find_map(|block| {
            if let BlockKind::Table(table) = block.kind() {
                Some(table)
            } else {
                None
            }
        })
        .ok_or("table")?;
    let cell = table
        .rows()
        .next()
        .and_then(|row| row.cells().next())
        .ok_or("cell")?;
    let mut context = TraversalContext::new(parsed.document().attributes());
    let mut trace = TypedTrace(ConstructedTrace {
        fail_dispatch: true,
        ..ConstructedTrace::default()
    });
    let result = context.with_table_cell(cell, |context| {
        context.visit_blocks(&mut trace, cell.blocks())
    });
    assert_eq!(result, Err("dispatch"));
    assert!(trace.0.entries.contains(&("event", Some("local"))));
    assert!(context.get("value").is_none());
    assert!(!context.is_nested_document());
    Ok(())
}
