//! Typography context survives temporary inputs and retains sibling order.

use std::cell::Cell;

use acdc_converters_core::substitutions::{Replacements, TextBoundaries};
use acdc_parser::{
    InlineNode, Location, Options, Plain, Raw, Substitution,
    ast::{InlineKind, InlineRef},
    parse_inline,
};

type TestError = Box<dyn std::error::Error>;

#[test]
fn sibling_context_preserves_invisible_nodes_and_formatting_barriers() -> Result<(), TestError> {
    for (source, expected) in [
        ("before[[hidden]]--after", "—\u{200b}after"),
        ("beforeindexterm:[hidden]--after", "—\u{200b}after"),
        ("((before))--after", "—\u{200b}after"),
        ("*before*--after", "--after"),
        ("before[[a]] [[b]]--[[c]] after", "\u{2009}—\u{2009}"),
    ] {
        let parsed = parse_inline(source, &Options::default())?;
        let nodes = parsed.inlines();
        let (index, text) = nodes
            .iter()
            .enumerate()
            .find_map(|(index, node)| {
                if let InlineNode::PlainText(text) = node
                    && text.content.contains("--")
                {
                    Some((index, text.content))
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("missing dash fragment: {source}"))?;
        let siblings = parsed.content().inlines();
        let context = TextBoundaries::BOTH.for_inline_refs(
            siblings.clone().nth(index),
            siblings.clone().take(index).rev(),
            siblings.clone().skip(index + 1),
        );
        assert_eq!(context, TextBoundaries::BOTH.for_inline(nodes, index));
        assert_eq!(
            Replacements::unicode().apply(text, context),
            expected,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn raw_context_keeps_adjacent_spaces_and_outlives_its_inputs() {
    for (before, after, expected) in [
        ("before ", " after", "--"),
        ("before", "after", "—\u{200b}"),
    ] {
        let (text, context) = {
            let before = String::from(before);
            let text = String::from("--");
            let after = String::from(after);
            let nodes = [
                InlineNode::PlainText(Plain {
                    content: &before,
                    escaped: false,
                    location: Location::default(),
                }),
                InlineNode::RawText(Raw {
                    content: &text,
                    subs: vec![Substitution::Replacements],
                    location: Location::default(),
                }),
                InlineNode::PlainText(Plain {
                    content: &after,
                    escaped: false,
                    location: Location::default(),
                }),
            ];
            let siblings = nodes.iter().map(InlineRef::from);
            let context = TextBoundaries::BOTH.for_inline_refs(
                siblings.clone().nth(1),
                siblings.clone().take(1).rev(),
                siblings.clone().skip(2),
            );
            assert_eq!(context, TextBoundaries::BOTH.for_inline(&nodes, 1));
            (text, context)
        };
        assert_eq!(Replacements::unicode().apply(&text, context), expected);
    }
}

#[test]
fn invisible_fragments_do_not_scan_their_siblings() -> Result<(), TestError> {
    let parsed = parse_inline(
        "left[[id]]right indexterm:[hidden] end",
        &Options::default(),
    )?;
    let mut checked = 0;
    for node in parsed.content().inlines() {
        if matches!(
            node.kind(),
            InlineKind::Anchor(_) | InlineKind::IndexTerm(_)
        ) {
            let reads = Cell::new(0);
            let siblings = || {
                parsed.content().inlines().inspect(|_| {
                    reads.set(reads.get() + 1);
                })
            };
            let context = TextBoundaries::BOTH.for_inline_refs(Some(node), siblings(), siblings());
            assert_eq!(context, TextBoundaries::NONE);
            assert_eq!(reads.get(), 0);
            checked += 1;
        }
    }
    assert_eq!(checked, 2);
    Ok(())
}
