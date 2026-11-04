use std::{io, rc::Rc};

use acdc_converters_core::{
    Diagnostics, Options as ConverterOptions, TraversalContext, WarningSource,
    visitor::InlineVisitor,
};
use acdc_parser::{
    CalloutRef, Document, Form, Highlight, InlineMacro, InlineNode, Location, Options, Plain, Raw,
    Substitution, Verbatim, parse_inline,
};

use crate::{Error, HtmlVariant, HtmlVisitor, Processor, RenderOptions};

type TestError = Box<dyn std::error::Error>;

#[test]
fn leaf_inputs_can_be_dropped_before_the_visitor() -> Result<(), TestError> {
    let document = Document::default();
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let processor = Rc::new(Processor::new_with_variant(
            ConverterOptions::default(),
            Options::builder(),
            variant,
        )?);
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        let mut visitor = HtmlVisitor::new(
            Vec::new(),
            processor,
            RenderOptions::default(),
            Diagnostics::new(&source, &mut warnings),
        );
        let mut traversal = TraversalContext::new(&document.attributes);
        let options = RenderOptions::default();
        {
            let text = String::from("temporary <text>");
            let plain = Plain {
                content: &text,
                escaped: true,
                location: Location::default(),
            };
            let verbatim = Verbatim {
                content: &text,
                location: Location::default(),
            };
            visitor.render_plain((&plain).into(), &options, &[Substitution::Replacements])?;
            visitor.render_verbatim((&verbatim).into(), &[])?;
            let callout = CalloutRef::explicit(3, Location::default());
            visitor.render_callout_ref(&traversal, (&callout).into())?;
            let source = String::from(
                r"pass:m[pass:[temporary-pass\]] latexmath:[x < y] https://example.test footnote:temporary[Note]",
            );
            let fragment = parse_inline(&source, &Options::default())?;
            let mut macros = 0;
            for node in fragment.inlines() {
                if let InlineNode::Macro(InlineMacro::Pass(pass)) = node {
                    visitor.render_pass(pass.into())?;
                    macros += 1;
                } else if let InlineNode::Macro(InlineMacro::Stem(stem)) = node {
                    visitor.render_stem(&mut traversal, stem.into())?;
                    macros += 1;
                } else if let InlineNode::Macro(InlineMacro::Autolink(link)) = node {
                    visitor.render_autolink(link.into(), &options)?;
                    macros += 1;
                } else if let InlineNode::Macro(InlineMacro::Footnote(note)) = node {
                    // Caller-created footnotes can use IDs outside the source grammar.
                    let mut note = note.clone();
                    note.id = Some("temporary\"note");
                    visitor.render_footnote((&note).into(), &options)?;
                    macros += 1;
                }
            }
            assert_eq!(macros, 4);
        }
        let output = String::from_utf8(visitor.into_writer())?;
        assert!(output.contains("temporary-pass"), "{output}");
        assert!(output.contains("x &lt; y"), "{output}");
        if variant == HtmlVariant::Standard {
            assert!(
                output.contains("id=\"_footnote_temporary&quot;note\""),
                "{output}"
            );
        }
        assert!(warnings.is_empty(), "{warnings:?}");
    }
    Ok(())
}

struct RefusingWriter;

impl io::Write for RefusingWriter {
    fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("closed"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn raw_capture_owns_text_even_when_writing_its_placeholder_fails() -> Result<(), TestError> {
    let processor = Rc::new(Processor::new_with_variant(
        ConverterOptions::default(),
        Options::builder(),
        HtmlVariant::Standard,
    )?);
    let source = WarningSource::new("html");
    let mut warnings = Vec::new();
    let mut visitor = HtmlVisitor::new(
        RefusingWriter,
        processor,
        RenderOptions::default(),
        Diagnostics::new(&source, &mut warnings),
    );
    visitor.captured_raw_fragments = Some(Vec::new());
    {
        let text = String::from("temporary <raw>");
        let raw = Raw {
            metadata: None,
            content: &text,
            location: Location::default(),
            subs: Vec::new(),
        };
        assert!(matches!(
            visitor.render_raw((&raw).into()),
            Err(Error::Io(_))
        ));
    }
    let captured = visitor
        .captured_raw_fragments
        .as_ref()
        .ok_or("expected raw capture")?;
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured.first().map(String::as_str),
        Some("temporary <raw>")
    );
    Ok(())
}

#[test]
fn retained_pass_skips_absent_and_empty_text_but_propagates_write_errors() -> Result<(), TestError>
{
    let processor = Rc::new(Processor::new_with_variant(
        ConverterOptions::default(),
        Options::builder(),
        HtmlVariant::Standard,
    )?);
    let source = WarningSource::new("html");
    let mut warnings = Vec::new();
    let mut visitor = HtmlVisitor::new(
        RefusingWriter,
        processor,
        RenderOptions::default(),
        Diagnostics::new(&source, &mut warnings),
    );
    let fragment = parse_inline(r"pass:m[pass:[content\]]", &Options::default())?;
    let Some(InlineNode::Macro(InlineMacro::Pass(pass))) = fragment.inlines().first() else {
        return Err("expected a retained passthrough".into());
    };
    for text in [None, Some("")] {
        let mut pass = pass.clone();
        pass.text = text;
        visitor.render_pass((&pass).into())?;
    }
    assert!(matches!(
        visitor.render_pass(pass.into()),
        Err(Error::Io(_))
    ));
    Ok(())
}

#[test]
fn span_warning_order_and_inline_state_survive_a_write_error() -> Result<(), TestError> {
    let document = Document::default();
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let processor = Rc::new(Processor::new_with_variant(
            ConverterOptions::default(),
            Options::builder(),
            variant,
        )?);
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        let mut visitor = HtmlVisitor::new(
            RefusingWriter,
            processor,
            RenderOptions::default(),
            Diagnostics::new(&source, &mut warnings),
        );
        let mut traversal = TraversalContext::new(&document.attributes);
        {
            let role = String::from("big \t small big");
            let node = InlineNode::HighlightText(Highlight {
                id: None,
                role: Some(&role),
                form: Form::Constrained,
                content: Vec::new(),
                location: Location::default(),
            });
            assert!(matches!(
                visitor.visit_inline_node(&mut traversal, (&node).into()),
                Err(Error::Io(_))
            ));
        }
        assert!(!visitor.render_options.in_inline_span);
        drop(visitor);
        assert_eq!(
            warnings
                .iter()
                .map(|warning| warning.message.as_ref())
                .collect::<Vec<_>>(),
            [
                "deprecated role `big`",
                "deprecated role `small`",
                "deprecated role `big`"
            ]
        );
    }
    Ok(())
}

#[test]
fn rich_inline_sequences_restore_context_after_write_errors() -> Result<(), TestError> {
    let document = Document::default();
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let processor = Rc::new(Processor::new_with_variant(
            ConverterOptions::default(),
            Options::builder(),
            variant,
        )?);
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        let mut visitor = HtmlVisitor::new(
            RefusingWriter,
            processor,
            RenderOptions::default(),
            Diagnostics::new(&source, &mut warnings),
        );
        let mut traversal = TraversalContext::new(&document.attributes);
        let boundaries = acdc_converters_core::substitutions::TextBoundaries::NONE;
        visitor.text_boundaries = boundaries;
        {
            let source = String::from("*bold _nested_* link:https://example.test[label]");
            let parsed = parse_inline(&source, &Options::default())?;
            assert!(
                acdc_converters_core::visitor::InlineVisitor::visit_inline_nodes(
                    &mut visitor,
                    &mut traversal,
                    parsed.content()
                )
                .is_err()
            );
        }
        assert_eq!(visitor.text_boundaries, boundaries);
        assert!(!visitor.render_options.in_inline_span);
        assert!(visitor.inline_link.is_none());
        drop(visitor);
        assert!(warnings.is_empty(), "{warnings:?}");
    }
    Ok(())
}

#[test]
fn typed_content_can_be_dropped_before_the_visitor_in_each_render_mode() -> Result<(), TestError> {
    let document = Document::default();
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        for (basic, toc) in [(false, false), (true, false), (false, true)] {
            let processor = Rc::new(Processor::new_with_variant(
                ConverterOptions::default(),
                Options::builder(),
                variant,
            )?);
            let source = WarningSource::new("html");
            let mut warnings = Vec::new();
            let mut visitor = HtmlVisitor::new(
                Vec::new(),
                processor,
                RenderOptions {
                    inlines_basic: basic,
                    toc_mode: toc,
                    ..RenderOptions::default()
                },
                Diagnostics::new(&source, &mut warnings),
            );
            let mut traversal = TraversalContext::new(&document.attributes);
            let boundaries = visitor.text_boundaries;
            {
                let source = String::from(
                    "*bold _nested_* link:https://example.test[*label*] image:photo.png[Alt] \
                     btn:[Save] kbd:[Ctrl+X] menu:File[Open > Recent] icon:heart[alt=Heart] \
                     ((index label)) pass:[<i>raw</i>] latexmath:[x < y]",
                );
                let options = Options::builder()
                    .with_attribute("experimental", "")
                    .build()?;
                let parsed = parse_inline(&source, &options)?;
                acdc_converters_core::visitor::InlineVisitor::visit_inline_nodes(
                    &mut visitor,
                    &mut traversal,
                    parsed.content(),
                )?;
            }
            assert_eq!(visitor.text_boundaries, boundaries);
            assert!(!visitor.render_options.in_inline_span);
            {
                let source = String::from(" trailing fragment");
                let parsed = parse_inline(&source, &Options::default())?;
                for node in parsed.content().inlines() {
                    acdc_converters_core::visitor::InlineVisitor::visit_inline_node(
                        &mut visitor,
                        &mut traversal,
                        node,
                    )?;
                }
            }
            let output = String::from_utf8(visitor.into_writer())?;
            for text in [
                "bold",
                "nested",
                "label",
                "photo.png",
                "Save",
                "Ctrl",
                "Open",
                "index label",
                "trailing fragment",
            ] {
                assert!(
                    output.contains(text),
                    "{variant:?}, basic={basic}, toc={toc}: {output}"
                );
            }
            assert!(
                !output.contains("<a href=\"https://example.test\"><a"),
                "{output}"
            );
            assert!(warnings.is_empty(), "{warnings:?}");
        }
    }
    Ok(())
}

#[test]
fn resolved_symbols_keep_html_encoding_without_reinterpreting_source() -> Result<(), TestError> {
    use acdc_parser::{Symbol, SymbolKind};

    let document = Document::default();
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let processor = Rc::new(Processor::new_with_variant(
            ConverterOptions::default(),
            Options::builder(),
            variant,
        )?);
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        let mut visitor = HtmlVisitor::new(
            Vec::new(),
            processor,
            RenderOptions::default(),
            Diagnostics::new(&source, &mut warnings),
        );
        let mut traversal = TraversalContext::new(&document.attributes);
        let options = RenderOptions::default();
        let mut expected_output = String::new();
        for (kind, expected) in [
            (SymbolKind::SpacedEmDash, "&thinsp;&mdash;&thinsp;"),
            (SymbolKind::WordBoundedEmDash, "&#8212;&#8203;"),
            (SymbolKind::Ellipsis, "&#8230;&#8203;"),
            (SymbolKind::Copyright, "&#169;"),
            (SymbolKind::RegisteredTrademark, "&#174;"),
            (SymbolKind::Trademark, "&#8482;"),
            (SymbolKind::CurvedApostrophe, "&#8217;"),
            (SymbolKind::RightArrow, "&#8594;"),
            (SymbolKind::LeftArrow, "&#8592;"),
            (SymbolKind::RightDoubleArrow, "&#8658;"),
            (SymbolKind::LeftDoubleArrow, "&#8656;"),
        ] {
            let node = InlineNode::Symbol(Symbol {
                kind,
                location: Location::default(),
            });
            expected_output.push_str(expected);
            visitor.render_inline_node_content(&mut traversal, (&node).into(), &options, &[])?;
        }
        assert_eq!(String::from_utf8(visitor.into_writer())?, expected_output);
        assert!(warnings.is_empty(), "{warnings:?}");
    }
    Ok(())
}

#[test]
fn escaped_text_keeps_html_character_escaping() -> Result<(), TestError> {
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let processor = Rc::new(Processor::new_with_variant(
            ConverterOptions::default(),
            Options::builder(),
            variant,
        )?);
        let source = WarningSource::new("html");
        let mut warnings = Vec::new();
        let mut visitor = HtmlVisitor::new(
            Vec::new(),
            processor,
            RenderOptions::default(),
            Diagnostics::new(&source, &mut warnings),
        );
        let plain = Plain {
            content: r"Keep (C) \=> <tag> & raw",
            escaped: true,
            location: Location::default(),
        };
        visitor.render_plain(
            (&plain).into(),
            &RenderOptions::default(),
            &[
                Substitution::SpecialChars,
                Substitution::Quotes,
                Substitution::Replacements,
            ],
        )?;
        let output = String::from_utf8(visitor.into_writer())?;
        assert!(output.contains("(C)"), "{output}");
        assert!(output.contains(r"\=&gt;"), "{output}");
        assert!(output.contains("&lt;tag&gt;"), "{output}");
        assert!(output.contains("&amp; raw"), "{output}");
        assert!(warnings.is_empty(), "{warnings:?}");
    }
    Ok(())
}

#[test]
fn retained_pass_uses_its_own_escaping_setting() -> Result<(), TestError> {
    let processor = Rc::new(Processor::new_with_variant(
        ConverterOptions::default(),
        Options::builder(),
        HtmlVariant::Standard,
    )?);
    let source = WarningSource::new("html");
    let mut warnings = Vec::new();
    let mut visitor = HtmlVisitor::new(
        Vec::new(),
        processor,
        RenderOptions::default(),
        Diagnostics::new(&source, &mut warnings),
    );
    let fragment = parse_inline(r"pass:m[pass:c[<tag> & raw\]]", &Options::default())?;
    let node = fragment.inlines().first().ok_or("passthrough")?;
    let InlineNode::Macro(InlineMacro::Pass(pass)) = node else {
        return Err("unprocessed pass macro".into());
    };
    visitor.render_pass(pass.into())?;
    let output = String::from_utf8(visitor.into_writer())?;
    assert!(output.contains("&lt;tag&gt; &amp; raw"), "{output}");
    assert!(warnings.is_empty(), "{warnings:?}");
    Ok(())
}
