use acdc_parser::{
    InlineNode, Location, Plain, Raw, Verbatim,
    ast::{AttributesRef, ContentRef, InlineKind, InlineRef},
};

use crate::{
    callouts,
    source_indent::{self, IndentedSource},
};

pub(crate) fn replace_text<'inline>(
    node: InlineRef<'inline>,
    content: &'inline str,
) -> Option<InlineNode<'inline>> {
    if let InlineKind::Plain(plain) = node.kind() {
        Some(InlineNode::PlainText(
            Plain::new(content, Location::from(node.location())).with_escaped(plain.is_escaped()),
        ))
    } else if let InlineKind::Verbatim(_) = node.kind() {
        Some(InlineNode::VerbatimText(Verbatim::new(
            content,
            Location::from(node.location()),
        )))
    } else if let InlineKind::Raw(raw) = node.kind() {
        let mut replacement = Raw::from(raw);
        replacement.replace_text(content);
        Some(InlineNode::RawText(replacement))
    } else {
        None
    }
}

pub(crate) fn replaced_inlines<'inline>(
    inlines: &'inline [InlineRef<'inline>],
    replacements: &'inline [Option<InlineNode<'inline>>],
) -> impl DoubleEndedIterator<Item = InlineRef<'inline>> + ExactSizeIterator + Clone {
    inlines
        .iter()
        .zip(replacements)
        .map(|(&node, replacement)| replacement.as_ref().map_or(node, InlineRef::from))
}

/// Keep temporary text alive while rendering; structured inlines stay in the source tree.
pub(crate) fn with_source_inlines<R>(
    content: ContentRef<'_>,
    attributes: Option<AttributesRef<'_>>,
    font_icons: bool,
    width: Option<u16>,
    render: impl FnOnce(&[InlineRef<'_>]) -> R,
) -> R {
    let original = content.inlines().collect::<Vec<_>>();
    let guards = callouts::strip_guards(&original, attributes, font_icons);
    let guarded = guards.as_ref().map(|nodes| {
        replaced_inlines(&original, nodes)
            .filter(|node| {
                !matches!(node.kind(), InlineKind::Plain(node) if node.text().is_empty())
                    && !matches!(node.kind(), InlineKind::Verbatim(node) if node.text().is_empty())
            })
            .collect::<Vec<_>>()
    });
    let inlines = guarded.as_deref().unwrap_or(&original);
    let indentation = IndentedSource::new(inlines.iter().copied(), width);
    let replacements = indentation
        .as_ref()
        .map(|source| source.replacements(inlines));
    let indented = replacements
        .as_ref()
        .map(|nodes| source_indent::indented_inlines(inlines, nodes).collect::<Vec<_>>());
    render(indented.as_deref().unwrap_or(inlines))
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::{CalloutRef, Paragraph, ast::ParagraphRef};

    #[test]
    fn source_guards_remove_empty_text_only_after_an_edit() {
        for (before, after, edited) in [("<!--", "-->", true), ("text", "tail", false)] {
            let paragraph = Paragraph::new(
                vec![
                    InlineNode::PlainText(Plain::new("", Location::default()).with_escaped(true)),
                    InlineNode::VerbatimText(Verbatim::new(before, Location::default())),
                    InlineNode::CalloutRef(CalloutRef::explicit(1, Location::default())),
                    InlineNode::PlainText(
                        Plain::new(after, Location::default()).with_escaped(true),
                    ),
                    InlineNode::VerbatimText(Verbatim::new("", Location::default())),
                    InlineNode::RawText(Raw::new("", vec![], Location::default())),
                ],
                Location::default(),
            );
            let content = ParagraphRef::from(&paragraph).content();
            for font_icons in [false, true] {
                with_source_inlines(content, None, font_icons, None, |inlines| {
                    if edited && font_icons {
                        let kinds = inlines.iter().map(|node| node.kind()).collect::<Vec<_>>();
                        assert!(matches!(
                            kinds.as_slice(),
                            [InlineKind::Callout(_), InlineKind::Raw(_)]
                        ));
                    } else {
                        assert_eq!(inlines.len(), 6);
                        let edits = callouts::strip_guards(inlines, None, font_icons);
                        assert_eq!(edits.is_some(), edited && font_icons);
                    }
                });
            }
            assert_eq!(content.inlines().count(), 6);
        }
    }
}
