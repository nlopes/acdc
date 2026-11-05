use acdc_converters_core::TraversalContext;
use acdc_parser::{
    InlineNode,
    ast::{AttributeValueRef, AttributesRef, InlineKind, InlineRef},
};

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

/// Only parsed callouts permit removal of a neighboring comment guard.
pub(crate) fn strip_guards<'input>(
    inlines: &[InlineRef<'input>],
    attributes: Option<AttributesRef<'_>>,
    font_icons: bool,
) -> Option<Vec<Option<InlineNode<'input>>>> {
    if !font_icons {
        return None;
    }
    let prefix = match attributes.and_then(|attributes| attributes.get("line-comment")) {
        Some(AttributeValueRef::Text(value)) => Some(value),
        _ => None,
    };
    let mut result: Option<Vec<_>> = None;
    for (index, &node) in inlines.iter().enumerate() {
        let replacement = text(Some(node)).and_then(|original| {
            let mut content = original;
            if index
                .checked_sub(1)
                .is_some_and(|previous| is_xml_callout(inlines, previous))
            {
                content = content.strip_prefix("-->").unwrap_or(content);
            }
            if inlines
                .get(index + 1)
                .is_some_and(|node| matches!(node.kind(), InlineKind::Callout(_)))
            {
                if is_xml_callout(inlines, index + 1) {
                    content = content.strip_suffix("<!--").unwrap_or(content);
                }
                content = strip_line_guard(content, prefix);
            }
            if content.len() == original.len() {
                None
            } else {
                crate::source::replace_text(node, content)
            }
        });
        if let Some(result) = &mut result {
            result.push(replacement);
        } else if replacement.is_some() {
            let mut replacements = vec![None; index];
            replacements.push(replacement);
            result = Some(replacements);
        }
    }
    result
}

fn text(node: Option<InlineRef<'_>>) -> Option<&str> {
    if let InlineKind::Verbatim(node) = node?.kind() {
        Some(node.text())
    } else if let InlineKind::Plain(node) = node?.kind() {
        Some(node.text())
    } else {
        None
    }
}

fn is_xml_callout(inlines: &[InlineRef<'_>], index: usize) -> bool {
    matches!(
        inlines.get(index).map(|node| node.kind()),
        Some(InlineKind::Callout(_))
    ) && index
        .checked_sub(1)
        .and_then(|previous| text(inlines.get(previous).copied()))
        .is_some_and(|text| text.ends_with("<!--"))
        && text(inlines.get(index + 1).copied()).is_some_and(|text| text.starts_with("-->"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::{
        AttributeValue, CalloutRef, Location, Paragraph, Verbatim, ast::ParagraphRef,
    };

    #[test]
    fn callout_guards_preserve_nontext_empty_quoted_and_disabled_controls()
    -> Result<(), Box<dyn std::error::Error>> {
        let inlines = [
            InlineNode::VerbatimText(Verbatim::new("value # ", Location::default())),
            InlineNode::CalloutRef(CalloutRef::explicit(1, Location::default())),
        ];
        for (prefix, expected) in [
            (None, "value "),
            (Some(AttributeValue::None), "value "),
            (Some(AttributeValue::Bool(false)), "value "),
            (Some(AttributeValue::from("#")), "value "),
            (Some(AttributeValue::from("'#'")), "value # "),
            (
                Some(AttributeValue::from(String::from("\"#\""))),
                "value # ",
            ),
            (Some(AttributeValue::from("")), "value # "),
            (Some(AttributeValue::from("?")), "value # "),
        ] {
            let mut paragraph = Paragraph::new(Vec::new(), Location::default());
            let mut metadata = paragraph.take_metadata();
            if let Some(prefix) = prefix {
                metadata.attributes.set("line-comment".into(), prefix);
            }
            paragraph.set_metadata(metadata)?;
            let attributes = Some(ParagraphRef::from(&paragraph).attributes());
            let refs = inlines.iter().map(InlineRef::from).collect::<Vec<_>>();
            let edits = strip_guards(&refs, attributes, true);
            let output = edits
                .as_ref()
                .map(|nodes| crate::source::replaced_inlines(&refs, nodes).collect::<Vec<_>>());
            let output = output.as_deref().unwrap_or(&refs);
            assert_eq!(text(output.first().copied()), Some(expected));
            assert_eq!(edits.is_none(), expected == "value # ");
            assert!(strip_guards(&refs, attributes, false).is_none());
            for (original, rendered) in inlines.iter().zip(output.iter()) {
                assert_eq!(original.location(), &Location::from(rendered.location()));
            }
        }
        Ok(())
    }
}
