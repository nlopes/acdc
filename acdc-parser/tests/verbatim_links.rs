#![cfg(feature = "pre-spec-subs")]

use acdc_parser::{Block, DelimitedBlockType, InlineMacro, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn verbatim_link_locations_retain_the_original_macro_spans() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/subs_verbatim_links.adoc");
    let parsed = parse(source, &Options::default())?;
    let mut spans = Vec::new();
    for block in &parsed.document().blocks {
        if let Block::DelimitedBlock(block) = block
            && let DelimitedBlockType::DelimitedListing(nodes) = &block.inner
        {
            for node in nodes {
                if matches!(node, InlineNode::Macro(InlineMacro::Link(_))) {
                    let location = node.location();
                    let span = source
                        .get(location.absolute_start..=location.absolute_end)
                        .ok_or("invalid macro span")?;
                    assert!(
                        (span.starts_with("link:") && span.ends_with(']')) || span == "{linkmacro}",
                        "{span:?}"
                    );
                    let prefix = source
                        .get(..location.absolute_start)
                        .ok_or("invalid start")?;
                    assert_eq!(
                        prefix.lines().count() + usize::from(prefix.ends_with('\n')),
                        location.start.line as usize
                    );
                    spans.push(span);
                }
            }
        }
    }
    assert_eq!(
        spans
            .iter()
            .filter(|span| **span == "link:{site}[{label}]")
            .count(),
        2
    );
    assert!(spans.contains(&"link:https://example.org/path?q=one&x=two[Web & label]"));
    assert!(spans.contains(&"link:https://example.org/unicode[Été & 日本語]"));
    Ok(())
}
