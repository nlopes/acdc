use std::io::Write;

use acdc_converters_core::{Diagnostics, TraversalContext, visitor::Visitor};
use acdc_parser::{
    InlineNode, Verbatim,
    ast::{AttributeValueRef, AttributesRef},
    strip_quotes,
};

const MAX_INDENT: u16 = 1024;

impl<'a, W: Write> crate::HtmlVisitor<'a, '_, W> {
    pub(crate) fn visit_indented_inlines(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: &[InlineNode<'_>],
        element_attributes: Option<AttributesRef<'_>>,
    ) -> Result<(), crate::Error> {
        let width = resolve(element_attributes, traversal, &mut self.diagnostics);
        let guarded = crate::callouts::strip_guards(
            inlines,
            element_attributes,
            crate::callouts::font_icons(traversal),
        );
        let inlines = guarded.as_ref();
        let indentation = IndentedSource::new(inlines, width);
        let indented = indentation.as_ref().map(|source| source.inlines(inlines));
        self.visit_inline_nodes(traversal, indented.as_deref().unwrap_or(inlines))
    }
}

pub(crate) fn resolve(
    element_attributes: Option<AttributesRef<'_>>,
    attributes: &TraversalContext<'_>,
    diagnostics: &mut Diagnostics<'_>,
) -> Option<u16> {
    let local = match element_attributes.and_then(|attributes| attributes.get("indent")) {
        Some(AttributeValueRef::Text(value)) => Some(strip_quotes(value)),
        _ => None,
    };
    let value = local.or_else(|| {
        matches!(
            element_attributes.and_then(|attributes| attributes.get("style")),
            Some(AttributeValueRef::Text("source"))
        )
        .then(|| {
            attributes
                .get("source-indent")
                .and_then(|value| value.text())
        })
        .flatten()
    })?;
    match value.trim().parse::<i128>() {
        Ok(value) if value < 0 => None,
        Ok(value) if value <= i128::from(MAX_INDENT) => u16::try_from(value).ok(),
        _ => {
            diagnostics.warn_with_advice(
                format!("unsupported source indentation {value:?}; keeping the original indentation"),
                "Use an integer from 0 to 1024, or a negative value to keep the original indentation.",
            );
            None
        }
    }
}

fn text<'a>(node: &'a InlineNode<'_>) -> Option<&'a str> {
    match node {
        InlineNode::VerbatimText(node) => Some(node.content),
        InlineNode::PlainText(node) => Some(node.content),
        InlineNode::RawText(node) => Some(node.content),
        InlineNode::BoldText(_)
        | InlineNode::ItalicText(_)
        | InlineNode::MonospaceText(_)
        | InlineNode::HighlightText(_)
        | InlineNode::SubscriptText(_)
        | InlineNode::SuperscriptText(_)
        | InlineNode::CurvedQuotationText(_)
        | InlineNode::CurvedApostropheText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::Macro(_)
        | InlineNode::CalloutRef(_)
        | _ => None,
    }
}

fn common_indent(inlines: &[InlineNode<'_>]) -> usize {
    let mut minimum = None;
    let mut leading = true;
    let mut width = 0;
    for node in inlines {
        if let Some(text) = text(node) {
            for byte in text.bytes() {
                match byte {
                    b'\n' => {
                        leading = true;
                        width = 0;
                    }
                    b' ' | b'\t' if leading => width += 1,
                    _ if leading => {
                        minimum = Some(minimum.map_or(width, |minimum: usize| minimum.min(width)));
                        leading = false;
                    }
                    _ => {}
                }
            }
        } else if leading {
            minimum = Some(minimum.map_or(width, |minimum: usize| minimum.min(width)));
            leading = false;
        }
    }
    minimum.unwrap_or(0)
}

/// Own only the replacement text; the temporary nodes borrow this buffer.
pub(crate) struct IndentedSource {
    text: Vec<Option<String>>,
}

impl IndentedSource {
    pub(crate) fn new(inlines: &[InlineNode<'_>], width: Option<u16>) -> Option<Self> {
        let width = usize::from(width?);
        let common = common_indent(inlines);
        if width == common
            && !inlines
                .iter()
                .filter_map(text)
                .any(|text| text.contains('\t'))
        {
            return None;
        }
        let indent = " ".repeat(width);
        let mut leading = true;
        let mut trim = common;
        let text = inlines
            .iter()
            .map(|node| {
                let Some(text) = text(node) else {
                    let prefix = leading.then(|| indent.clone());
                    leading = false;
                    return prefix;
                };
                let mut output = String::with_capacity(text.len());
                for part in text.split_inclusive('\n') {
                    let part = if leading {
                        let count = part
                            .bytes()
                            .take_while(|byte| matches!(byte, b' ' | b'\t'))
                            .count()
                            .min(trim);
                        trim -= count;
                        part.split_at(count).1
                    } else {
                        part
                    };
                    if leading && !part.is_empty() && part != "\n" {
                        output.push_str(&indent);
                        leading = false;
                    }
                    output.push_str(part);
                    if part.ends_with('\n') {
                        leading = true;
                        trim = common;
                    }
                }
                Some(output)
            })
            .collect();
        Some(Self { text })
    }

    pub(crate) fn inlines<'a>(&'a self, inlines: &'a [InlineNode<'a>]) -> Vec<InlineNode<'a>> {
        let mut output = Vec::with_capacity(inlines.len());
        for (node, text) in inlines.iter().zip(&self.text) {
            let mut node = node.clone();
            if let Some(text) = text {
                match &mut node {
                    InlineNode::VerbatimText(node) => node.content = text,
                    InlineNode::PlainText(node) => node.content = text,
                    InlineNode::RawText(node) => node.content = text,
                    InlineNode::BoldText(_)
                    | InlineNode::ItalicText(_)
                    | InlineNode::MonospaceText(_)
                    | InlineNode::HighlightText(_)
                    | InlineNode::SubscriptText(_)
                    | InlineNode::SuperscriptText(_)
                    | InlineNode::CurvedQuotationText(_)
                    | InlineNode::CurvedApostropheText(_)
                    | InlineNode::StandaloneCurvedApostrophe(_)
                    | InlineNode::LineBreak(_)
                    | InlineNode::InlineAnchor(_)
                    | InlineNode::Macro(_)
                    | InlineNode::CalloutRef(_)
                    | _ => {
                        if !text.is_empty() {
                            output.push(InlineNode::VerbatimText(Verbatim {
                                content: text,
                                location: node.location().clone(),
                            }));
                        }
                    }
                }
            }
            output.push(node);
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_indent_preserves_quoting_presence_limits_and_source_default()
    -> Result<(), Box<dyn std::error::Error>> {
        use acdc_converters_core::WarningSource;
        use acdc_parser::{AttributeValue, Location, Options, Paragraph, ast::ParagraphRef, parse};
        let parsed = parse(":source-indent: 4\n\nText.", &Options::default())?;
        let traversal = TraversalContext::new(parsed.document_ref().attributes());
        let source = WarningSource::new("html");
        for style in [None, Some("source"), Some("literal")] {
            for (local, expected, warning) in [
                (None, None, false),
                (Some(AttributeValue::None), None, false),
                (Some(AttributeValue::Bool(false)), None, false),
                (Some(AttributeValue::from("\"2\"")), Some(2), false),
                (
                    Some(AttributeValue::from(String::from("' 2 '"))),
                    Some(2),
                    false,
                ),
                (Some(AttributeValue::from("0")), Some(0), false),
                (Some(AttributeValue::from("-1")), None, false),
                (Some(AttributeValue::from("1024")), Some(1024), false),
                (Some(AttributeValue::from("1025")), None, true),
                (Some(AttributeValue::from("")), None, true),
            ] {
                let mut paragraph = Paragraph::new(Vec::new(), Location::default());
                paragraph.metadata.style = style;
                paragraph
                    .metadata
                    .attributes
                    .set("style".into(), "source".into());
                let fallback = matches!(
                    local,
                    None | Some(AttributeValue::None | AttributeValue::Bool(_))
                );
                if let Some(local) = local {
                    paragraph.metadata.attributes.set("indent".into(), local);
                }
                let expected = if fallback && style == Some("source") {
                    Some(4)
                } else {
                    expected
                };
                let mut warnings = Vec::new();
                assert_eq!(
                    resolve(
                        Some(ParagraphRef::from(&paragraph).attributes()),
                        &traversal,
                        &mut Diagnostics::new(&source, &mut warnings)
                    ),
                    expected
                );
                assert_eq!(warnings.len(), usize::from(warning));
            }
        }
        let mut warnings = Vec::new();
        assert_eq!(
            resolve(
                None,
                &traversal,
                &mut Diagnostics::new(&source, &mut warnings)
            ),
            None
        );
        assert_eq!(warnings.len(), 0);
        Ok(())
    }

    #[test]
    fn normalization_retains_relative_indent_across_node_boundaries()
    -> Result<(), Box<dyn std::error::Error>> {
        let inlines = ["  ", "  α\n    ", "\n      β\n    γ"].map(|content| {
            InlineNode::VerbatimText(Verbatim {
                content,
                location: acdc_parser::Location::default(),
            })
        });
        for (width, expected) in [(0, "α\n\n  β\nγ"), (2, "  α\n\n    β\n  γ")] {
            let normalized = IndentedSource::new(&inlines, Some(width))
                .ok_or("expected indentation to change")?;
            let output = normalized.inlines(&inlines);
            assert_eq!(output.iter().filter_map(text).collect::<String>(), expected);
            for (original, output) in inlines.iter().zip(&output) {
                assert_eq!(original.location(), output.location());
            }
        }
        Ok(())
    }

    #[test]
    fn unchanged_indentation_needs_no_replacement_buffer() {
        let inlines = [InlineNode::VerbatimText(Verbatim {
            content: "  first\n    second",
            location: acdc_parser::Location::default(),
        })];
        assert!(IndentedSource::new(&inlines, None).is_none());
        assert!(IndentedSource::new(&inlines, Some(2)).is_none());
    }
}
