use std::io::Write;

use acdc_converters_core::{Diagnostics, TraversalContext, visitor::Visitor};
use acdc_parser::{
    InlineNode, Verbatim,
    ast::{AttributeValueRef, AttributesRef, InlineKind, InlineRef},
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
        let indentation = IndentedSource::new(inlines.iter().map(InlineRef::from), width);
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

fn text(node: InlineRef<'_>) -> Option<&str> {
    match node.kind() {
        InlineKind::Verbatim(node) => Some(node.text()),
        InlineKind::Plain(node) => Some(node.text()),
        InlineKind::Raw(node) => Some(node.text()),
        InlineKind::Bold(_)
        | InlineKind::Italic(_)
        | InlineKind::Monospace(_)
        | InlineKind::Highlight(_)
        | InlineKind::Subscript(_)
        | InlineKind::Superscript(_)
        | InlineKind::CurvedQuotation(_)
        | InlineKind::CurvedApostrophe(_)
        | InlineKind::StandaloneCurvedApostrophe(_)
        | InlineKind::LineBreak(_)
        | InlineKind::Anchor(_)
        | InlineKind::Callout(_)
        | InlineKind::Footnote(_)
        | InlineKind::Icon(_)
        | InlineKind::Image(_)
        | InlineKind::Keyboard(_)
        | InlineKind::Button(_)
        | InlineKind::Menu(_)
        | InlineKind::Url(_)
        | InlineKind::Link(_)
        | InlineKind::Mailto(_)
        | InlineKind::Autolink(_)
        | InlineKind::CrossReference(_)
        | InlineKind::Pass(_)
        | InlineKind::Stem(_)
        | InlineKind::IndexTerm(_)
        | _ => None,
    }
}

fn common_indent<'inline>(inlines: impl Iterator<Item = InlineRef<'inline>>) -> usize {
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
    pub(crate) fn new<'inline>(
        inlines: impl Iterator<Item = InlineRef<'inline>> + Clone,
        width: Option<u16>,
    ) -> Option<Self> {
        let width = usize::from(width?);
        let common = common_indent(inlines.clone());
        if width == common
            && !inlines
                .clone()
                .filter_map(text)
                .any(|text| text.contains('\t'))
        {
            return None;
        }
        let indent = " ".repeat(width);
        let mut leading = true;
        let mut trim = common;
        let text = inlines
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
            let normalized = IndentedSource::new(inlines.iter().map(InlineRef::from), Some(width))
                .ok_or("expected indentation to change")?;
            let output = normalized.inlines(&inlines);
            assert_eq!(
                output
                    .iter()
                    .map(InlineRef::from)
                    .filter_map(text)
                    .collect::<String>(),
                expected
            );
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
        assert!(IndentedSource::new(inlines.iter().map(InlineRef::from), None).is_none());
        assert!(IndentedSource::new(inlines.iter().map(InlineRef::from), Some(2)).is_none());
    }

    #[test]
    fn normalization_preserves_text_kinds_profiles_and_include_spans()
    -> Result<(), Box<dyn std::error::Error>> {
        use acdc_parser::{Location, Plain, Raw, Substitution};

        let mut location = Location::default();
        location.start.file = Some(vec!["outer.adoc".into(), "selected.adoc".into()].into());
        location.end.file = Some(vec!["outer.adoc".into(), "selected.adoc".into()].into());
        let inlines = [
            InlineNode::PlainText(Plain {
                content: "  ",
                location: location.clone(),
                escaped: true,
            }),
            InlineNode::RawText(Raw {
                content: "  α\n    ",
                location: location.clone(),
                subs: vec![
                    Substitution::Replacements,
                    Substitution::SpecialChars,
                    Substitution::Replacements,
                ],
            }),
            InlineNode::VerbatimText(Verbatim {
                content: "\n      β\n    γ",
                location,
            }),
        ];
        for (width, expected) in [(0, "α\n\n  β\nγ"), (2, "  α\n\n    β\n  γ")] {
            let normalized = IndentedSource::new(inlines.iter().map(InlineRef::from), Some(width))
                .ok_or("expected indentation to change")?;
            let output = normalized.inlines(&inlines);
            assert_eq!(
                output
                    .iter()
                    .map(InlineRef::from)
                    .filter_map(text)
                    .collect::<String>(),
                expected
            );
            let [
                InlineNode::PlainText(plain),
                InlineNode::RawText(raw),
                InlineNode::VerbatimText(_),
            ] = output.as_slice()
            else {
                return Err("text kinds changed".into());
            };
            assert!(plain.escaped);
            assert_eq!(
                raw.subs,
                [
                    Substitution::Replacements,
                    Substitution::SpecialChars,
                    Substitution::Replacements,
                ]
            );
            for (original, output) in inlines.iter().zip(&output) {
                assert_eq!(original.location(), output.location());
            }
        }
        Ok(())
    }

    #[test]
    fn structured_content_starts_a_line_and_retains_its_source_span()
    -> Result<(), Box<dyn std::error::Error>> {
        use acdc_parser::{CompatibilityMode, Location, Options, parse_inline};

        for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
            let options = Options::builder().with_compatibility_mode(mode).build()?;
            let parsed = parse_inline("*bold*", &options)?;
            let [InlineNode::BoldText(span)] = parsed.inlines() else {
                return Err("expected one formatting span".into());
            };
            let mut span = span.clone();
            span.location.start.file = Some(vec!["selected.adoc".into()].into());
            span.location.end.file = Some(vec!["selected.adoc".into()].into());
            let original = InlineNode::BoldText(span);
            let inlines = [
                InlineNode::VerbatimText(Verbatim {
                    content: "\t",
                    location: Location::default(),
                }),
                original.clone(),
                InlineNode::VerbatimText(Verbatim {
                    content: "\n  after",
                    location: Location::default(),
                }),
            ];
            let normalized = IndentedSource::new(inlines.iter().map(InlineRef::from), Some(2))
                .ok_or("expected indentation to change")?;
            let output = normalized.inlines(&inlines);
            let [
                InlineNode::VerbatimText(first),
                InlineNode::VerbatimText(prefix),
                span,
                InlineNode::VerbatimText(tail),
            ] = output.as_slice()
            else {
                return Err("unexpected normalized fragments".into());
            };
            assert_eq!(first.content, "");
            assert_eq!(prefix.content, "  ");
            assert_eq!(span, &original);
            assert_eq!(&prefix.location, original.location());
            assert_eq!(tail.content, "\n   after");
        }
        Ok(())
    }
}
