use std::borrow::Cow;

use acdc_converters_core::TraversalContext;
use acdc_parser::{BlockMetadata, InlineNode};

pub(crate) fn font_icons(attributes: &TraversalContext<'_>) -> bool {
    attributes.get("icons").and_then(|value| value.text()) == Some("font")
}

pub(crate) fn marker_html(number: usize, font_icons: bool) -> String {
    if font_icons {
        format!("<i class=\"conum\" data-value=\"{number}\"></i><b>({number})</b>")
    } else {
        format!("<b class=\"conum\">({number})</b>")
    }
}

/// Remove guards only beside parsed callouts, borrowing the remaining source.
pub(crate) fn strip_guards<'nodes, 'input>(
    inlines: &'nodes [InlineNode<'input>],
    metadata: &BlockMetadata<'_>,
    font_icons: bool,
) -> Cow<'nodes, [InlineNode<'input>]> {
    if !font_icons {
        return Cow::Borrowed(inlines);
    }
    let prefix = metadata.attributes.get_string("line-comment");
    let mut result = Cow::Borrowed(inlines);
    for (index, node) in inlines.iter().enumerate() {
        let Some(original) = text(Some(node)) else {
            continue;
        };
        let mut content = original;
        if index
            .checked_sub(1)
            .is_some_and(|previous| is_xml_callout(inlines, previous))
        {
            content = content.strip_prefix("-->").unwrap_or(content);
        }
        if inlines
            .get(index + 1)
            .is_some_and(|node| matches!(node, InlineNode::CalloutRef(_)))
        {
            if is_xml_callout(inlines, index + 1) {
                content = content.strip_suffix("<!--").unwrap_or(content);
            }
            content = strip_line_guard(content, prefix.as_deref());
        }
        if content.len() != original.len() {
            match result.to_mut().get_mut(index) {
                Some(InlineNode::VerbatimText(node)) => node.content = content,
                Some(InlineNode::PlainText(node)) => node.content = content,
                _ => {}
            }
        }
    }
    if let Cow::Owned(nodes) = &mut result {
        nodes.retain(|node| text(Some(node)).is_none_or(|text| !text.is_empty()));
    }
    result
}

fn text<'input>(node: Option<&InlineNode<'input>>) -> Option<&'input str> {
    if let InlineNode::VerbatimText(node) = node? {
        Some(node.content)
    } else if let InlineNode::PlainText(node) = node? {
        Some(node.content)
    } else {
        None
    }
}

fn is_xml_callout(inlines: &[InlineNode<'_>], index: usize) -> bool {
    matches!(inlines.get(index), Some(InlineNode::CalloutRef(_)))
        && index
            .checked_sub(1)
            .and_then(|previous| text(inlines.get(previous)))
            .is_some_and(|text| text.ends_with("<!--"))
        && text(inlines.get(index + 1)).is_some_and(|text| text.starts_with("-->"))
}

fn strip_line_guard<'text>(text: &'text str, prefix: Option<&str>) -> &'text str {
    // Try one optional ASCII space; the prefix itself may also end in spaces.
    let candidate = text.strip_suffix(' ').unwrap_or(text);
    if let Some(prefix) = prefix {
        if prefix.is_empty() {
            text
        } else {
            candidate
                .strip_suffix(prefix)
                .or_else(|| text.strip_suffix(prefix))
                .unwrap_or(text)
        }
    } else {
        ["//", "#", "--", ";;"]
            .into_iter()
            .find_map(|prefix| candidate.strip_suffix(prefix))
            .unwrap_or(text)
    }
}
