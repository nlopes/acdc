use std::io::Write;

use acdc_converters_core::{Diagnostics, TraversalContext, visitor::Visitor};
use acdc_parser::{BlockMetadata, InlineNode, Verbatim};

const MAX_INDENT: u16 = 1024;

impl<'a, W: Write> crate::HtmlVisitor<'a, '_, W> {
    pub(crate) fn visit_indented_inlines(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: &[InlineNode<'_>],
        metadata: &BlockMetadata<'_>,
    ) -> Result<(), crate::Error> {
        let width = resolve(metadata, traversal, &mut self.diagnostics);
        let indentation = IndentedSource::new(inlines, width);
        let indented = indentation.as_ref().map(|source| source.inlines(inlines));
        self.visit_inline_nodes(traversal, indented.as_deref().unwrap_or(inlines))
    }
}

pub(crate) fn resolve(
    metadata: &BlockMetadata<'_>,
    attributes: &TraversalContext<'_>,
    diagnostics: &mut Diagnostics<'_>,
) -> Option<u16> {
    let local = metadata.attributes.get_string("indent");
    let value = local.as_deref().or_else(|| {
        (metadata.style == Some("source"))
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
