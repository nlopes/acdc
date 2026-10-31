use std::{io, rc::Rc};

use acdc_converters_core::{
    Diagnostics, Options as ConverterOptions, TraversalContext, WarningSource,
};
use acdc_parser::{
    CalloutRef, Document, InlineMacro, InlineNode, Location, Options, Plain, Raw, Substitution,
    Verbatim, parse_inline,
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
            visitor.render_verbatim(&mut traversal, (&verbatim).into(), &options, &[])?;
            let callout = CalloutRef::explicit(3, Location::default());
            visitor.render_callout_ref(&traversal, (&callout).into())?;
            let source = String::from("pass:[temporary-pass] latexmath:[x < y]");
            let fragment = parse_inline(&source, &Options::default())?;
            let mut macros = 0;
            for node in fragment.inlines() {
                if let InlineNode::Macro(InlineMacro::Pass(pass)) = node {
                    visitor.render_pass(pass.into(), &options, &[])?;
                    macros += 1;
                } else if let InlineNode::Macro(InlineMacro::Stem(stem)) = node {
                    visitor.render_stem(&mut traversal, stem.into())?;
                    macros += 1;
                }
            }
            assert_eq!(macros, 2);
        }
        let output = String::from_utf8(visitor.into_writer())?;
        assert!(output.contains("temporary-pass"), "{output}");
        assert!(output.contains("x &lt; y"), "{output}");
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
    let fragment = parse_inline("pass:[content]", &Options::default())?;
    let Some(InlineNode::Macro(InlineMacro::Pass(pass))) = fragment.inlines().first() else {
        return Err("expected a retained passthrough".into());
    };
    for text in [None, Some("")] {
        let mut pass = pass.clone();
        pass.text = text;
        visitor.render_pass((&pass).into(), &RenderOptions::default(), &[])?;
    }
    assert!(matches!(
        visitor.render_pass(pass.into(), &RenderOptions::default(), &[]),
        Err(Error::Io(_))
    ));
    Ok(())
}
