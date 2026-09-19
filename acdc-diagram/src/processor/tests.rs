use std::error::Error;

use acdc_parser::{CompatibilityMode, Options as ParserOptions, parse};
use rstest::rstest;

use super::Processor;
use crate::Options;

#[rstest]
#[case("graphviz::missing.dot[]")]
#[case("== Section\n\ngraphviz::missing.dot[]")]
#[case("[NOTE]\n====\ngraphviz::missing.dot[]\n====")]
#[case("****\ngraphviz::missing.dot[]\n****")]
#[case("* Item\n+\ngraphviz::missing.dot[]")]
#[case(". Item\n+\ngraphviz::missing.dot[]")]
#[case("Term:: Definition\n+\ngraphviz::missing.dot[]")]
#[case("[cols=\"a\"]\n|===\n|graphviz::missing.dot[]\n|===")]
fn abort_preserves_document_and_nested_content(#[case] body: &str) -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let processor = Processor::new(Options::builder().base_dir(directory.path()).build());
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let source = format!("= Diagrams\n:diagram-on-error: abort\n\n{body}\n\nAfter.\n");
        let mut parsed = parse(
            &source,
            &ParserOptions::builder()
                .with_compatibility_mode(mode)
                .build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let mut warnings = Vec::new();
        let result = parsed
            .with_document_mut(|document, arena| processor.process(document, arena, &mut warnings));
        assert!(result.is_err(), "{mode:?}: {body}");
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        assert_eq!(
            serde_json::to_value(parsed.document())?,
            before,
            "{mode:?}: {body}"
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn generated_image_keeps_title_identity_and_alt_content() -> Result<(), Box<dyn Error>> {
    use std::{fs, os::unix::fs::PermissionsExt};

    use acdc_parser::ast::{BlockKind, ElementIdRef, InlineKind, InlineRef};

    let directory = tempfile::tempdir()?;
    let tool = directory.path().join("dot");
    fs::write(
        &tool,
        "#!/bin/sh\ncat >/dev/null\nfor arg do\ncase \"$arg\" in\n-o*) printf '<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"10\"></svg>' >\"${arg#-o}\";;\nesac\ndone\n",
    )?;
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755))?;
    let processor = Processor::new(
        Options::builder()
            .base_dir(directory.path())
            .output_dir(directory.path())
            .backend("html5")
            .build(),
    );
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let source = format!(
            "= Diagrams\n:graphvizdot: {}\n\n.Chart\n[graphviz#chart.graph,chart,svg,alt=\"A & B\"]\n----\ndigraph {{ a -> b }}\n----\n",
            tool.display()
        );
        let mut parsed = parse(
            &source,
            &ParserOptions::builder()
                .with_compatibility_mode(mode)
                .build()?,
        )?;
        let mut warnings = Vec::new();
        parsed.with_document_mut(|document, arena| {
            processor.process(document, arena, &mut warnings)
        })?;
        assert!(warnings.is_empty(), "{mode:?}: {warnings:?}");
        let block = parsed
            .document()
            .blocks()
            .next()
            .ok_or("missing image block")?;
        let BlockKind::Image(image) = block.kind() else {
            return Err(format!("expected generated image in {mode:?}: {block:?}").into());
        };
        assert_eq!(image.id().map(ElementIdRef::as_str), Some("chart"));
        assert!(image.roles().contains("graph"));
        let title = image.title().ok_or("missing image title")?;
        assert_eq!(super::plain_text(title).as_deref(), Some("Chart"));
        let alt = image.alt().ok_or("missing image alt")?;
        assert_eq!(super::plain_text(alt).as_deref(), Some("A & B"));
        assert!(matches!(
            alt.inlines().next().map(InlineRef::kind),
            Some(InlineKind::Plain(_))
        ));
        assert!(directory.path().join("chart.svg").is_file());
    }
    Ok(())
}
