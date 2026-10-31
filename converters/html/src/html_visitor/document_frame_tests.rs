use std::{io, rc::Rc};

use acdc_converters_core::{
    Diagnostics, Options as ConverterOptions, TraversalContext, WarningSource, visitor::Visitor,
};
use acdc_parser::{Document, Header, Location, Options, parse};

use crate::{Error, HtmlVariant, HtmlVisitor, Processor, RenderOptions};

type TestError = Box<dyn std::error::Error>;

#[test]
fn frame_preserves_document_processor_and_replay_snapshots_after_owner_edits()
-> Result<(), TestError> {
    let parsed = parse(
        "[[first]]\n[[last]]\n= Frame\n:lang: fr\n:toc: left\n:toc-class: header-toc\n:revnumber: 1.0\n\nBody.\n",
        &Options::default(),
    )?;
    let replay = parse(
        ":lang: es\n:toc: right\n:toc-class: replay-toc\n\nBody.\n",
        &Options::default(),
    )?;
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        for (explicit, expected_id) in [(false, "last"), (true, "first")] {
            let mut document = Document::default();
            let original = parsed
                .document()
                .header
                .as_ref()
                .ok_or("expected a header")?;
            document.header = Some(
                Header::new(original.title.as_ref().to_vec().into(), Location::default())
                    .with_metadata(original.metadata.clone()),
            );
            document.attributes = parsed.document().attributes.clone();
            let header = document.header.as_mut().ok_or("expected a header")?;
            assert_eq!(header.metadata.anchors.len(), 2);
            header.metadata.id = if explicit {
                header.metadata.anchors.first().cloned()
            } else {
                None
            };
            header.metadata.roles = vec!["alpha", "beta", "alpha"];
            let processor = Processor::new_with_variant(
                ConverterOptions::default(),
                Options::builder()
                    .with_attribute("lang", "de")
                    .with_attribute("doctype", "book")
                    .with_attribute("dark-mode", true)
                    .with_attribute("toc", true)
                    .with_attribute("toc-position", "right")
                    .with_attribute("toc-class", "processor-toc")
                    .with_attribute("revnumber", "9.1")
                    .with_attribute("version-label", "Build")
                    .with_attribute("stylesheet", false)
                    .with_attribute("webfonts", false),
                variant,
            )?;
            let source = WarningSource::new("html");
            let mut warnings = Vec::new();
            let mut visitor = HtmlVisitor::new(
                Vec::new(),
                Rc::new(processor),
                RenderOptions::default(),
                Diagnostics::new(&source, &mut warnings),
            );
            let mut traversal = TraversalContext::new(&replay.document().attributes);
            visitor.visit_document_start(&mut traversal, &document)?;
            visitor.render_footer_version()?;
            let output = String::from_utf8(visitor.into_writer())?;
            assert!(output.contains("<html lang=\"fr\">"), "{output}");
            assert!(output.contains(&format!("<body id=\"{expected_id}\" class=\"book dark header-toc toc-left alpha beta alpha\">")), "{output}");
            assert!(output.ends_with("Build 9.1<br>\n"), "{output}");
            assert!(warnings.is_empty(), "{warnings:?}");
        }
    }
    Ok(())
}

#[test]
fn frame_keeps_embedded_return_and_css_signature_error_after_alias_warnings()
-> Result<(), TestError> {
    let parsed = parse(
        "= Frame\n:syntect-style: InspiredGitHub\n\nBody.\n",
        &Options::default(),
    )?;
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        for embedded in [false, true] {
            let processor = Processor::new_with_variant(
                ConverterOptions::default(),
                Options::builder()
                    .with_attribute("css-signature", "old-id")
                    .with_attribute("stylesheet", false)
                    .with_attribute("webfonts", false),
                variant,
            )?;
            let source = WarningSource::new("html");
            let mut warnings = Vec::new();
            let mut visitor = HtmlVisitor::new(
                Vec::new(),
                Rc::new(processor),
                RenderOptions {
                    embedded,
                    ..RenderOptions::default()
                },
                Diagnostics::new(&source, &mut warnings),
            );
            let mut traversal = TraversalContext::new(&parsed.document().attributes);
            let result = visitor.visit_document_start(&mut traversal, parsed.document());
            let output = String::from_utf8(visitor.into_writer())?;
            if embedded {
                result?;
                assert!(output.is_empty(), "{output}");
            } else {
                assert!(matches!(result, Err(Error::UnsupportedCssSignature)));
                assert!(output.starts_with("<!DOCTYPE html>\n"));
                assert!(output.ends_with("</head>\n"), "{output}");
                assert!(!output.contains("<body"), "{output}");
            }
            assert_eq!(warnings.len(), 1);
            assert_eq!(
                warnings.first().and_then(|warning| warning.advice()),
                Some("Use `highlight-style` instead.")
            );
        }
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
fn frame_propagates_writer_failure_before_css_signature_validation() -> Result<(), TestError> {
    let parsed = parse("= Frame\n\nBody.\n", &Options::default())?;
    let processor = Processor::new_with_variant(
        ConverterOptions::default(),
        Options::builder().with_attribute("css-signature", "old-id"),
        HtmlVariant::Standard,
    )?;
    let source = WarningSource::new("html");
    let mut warnings = Vec::new();
    let mut visitor = HtmlVisitor::new(
        RefusingWriter,
        Rc::new(processor),
        RenderOptions::default(),
        Diagnostics::new(&source, &mut warnings),
    );
    let mut traversal = TraversalContext::new(&parsed.document().attributes);
    let result = visitor.visit_document_start(&mut traversal, parsed.document());
    assert!(matches!(result, Err(Error::Io(error)) if error.to_string() == "closed"));
    Ok(())
}

#[test]
fn headerless_frame_uses_processor_toc_placement_for_its_wrapper() -> Result<(), TestError> {
    let parsed = parse("== Section\n\nBody.\n", &Options::default())?;
    assert!(parsed.document().header.is_none());
    assert_eq!(parsed.document().toc_entries.len(), 1);
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        for (processor_placement, replay_placement, has_wrapper) in
            [("left", "preamble", true), ("preamble", "left", false)]
        {
            let replay = parse(
                &format!(":toc: {replay_placement}\n\nBody.\n"),
                &Options::default(),
            )?;
            let mut processor = Processor::new_with_variant(
                ConverterOptions::default(),
                Options::builder()
                    .with_attribute("toc", true)
                    .with_attribute("toc-placement", processor_placement),
                variant,
            )?;
            processor
                .toc_entries
                .clone_from(&parsed.document().toc_entries);
            let source = WarningSource::new("html");
            let mut warnings = Vec::new();
            let mut visitor = HtmlVisitor::new(
                Vec::new(),
                Rc::new(processor),
                RenderOptions::default(),
                Diagnostics::new(&source, &mut warnings),
            );
            let mut traversal = TraversalContext::new(&replay.document().attributes);
            visitor.visit_body_content_start(&mut traversal, parsed.document())?;
            let output = String::from_utf8(visitor.into_writer())?;
            let opener = if variant == HtmlVariant::Standard {
                "<div id=\"header\">"
            } else {
                "<header id=\"header\">"
            };
            assert_eq!(output.starts_with(opener), has_wrapper, "{output}");
            assert!(warnings.is_empty(), "{warnings:?}");
        }
    }
    Ok(())
}
