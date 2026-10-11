use std::{path::Path, sync::Arc};

use acdc_parser::{
    Block, DelimitedBlockType, IncludeLoader, IncludeSource, IncludeSourceError,
    IncludeSourceErrorKind, IncludeSourceProvider, IncludeSourceTarget, InlineMacro, InlineNode,
    Options, ParseResult, SafeMode, Stem, StemContent, StemNotation, parse, parse_inline,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn inline_stem(parsed: &ParseResult) -> Result<&Stem<'_>, Box<dyn std::error::Error>> {
    parsed
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            paragraph.content.iter().find_map(|node| {
                if let InlineNode::Macro(InlineMacro::Stem(stem)) = node {
                    Some(stem)
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| "expected an inline STEM macro".into())
}

fn block_stem(parsed: &ParseResult) -> Result<&StemContent<'_>, Box<dyn std::error::Error>> {
    parsed
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            let Block::DelimitedBlock(block) = block else {
                return None;
            };
            if let DelimitedBlockType::DelimitedStem(stem) = &block.inner {
                Some(stem)
            } else {
                None
            }
        })
        .ok_or_else(|| "expected a STEM block".into())
}

#[rstest::rstest]
#[case("", StemNotation::Asciimath)]
#[case(":stem:\n", StemNotation::Asciimath)]
#[case(":stem: asciimath\n", StemNotation::Asciimath)]
#[case(":stem: latexmath\n", StemNotation::Latexmath)]
#[case(":stem: latex\n", StemNotation::Latexmath)]
#[case(":stem: tex\n", StemNotation::Latexmath)]
#[case(":stem: invalid\n", StemNotation::Asciimath)]
fn stem_document_notation_applies_to_inline_and_block_content(
    #[case] header: &str,
    #[case] expected: StemNotation,
) -> TestResult {
    let source = format!("{header}\nInline stem:[x^2].\n\n[stem]\n++++\nsqrt(4)\n++++\n");
    let parsed = parse(&source, &Options::default())?;
    assert_eq!(inline_stem(&parsed)?.notation, expected);
    assert_eq!(block_stem(&parsed)?.notation, expected);
    Ok(())
}

#[rstest::rstest]
#[case("stem,asciimath", StemNotation::Asciimath)]
#[case("stem,latexmath", StemNotation::Latexmath)]
#[case("stem,latex", StemNotation::Latexmath)]
#[case("stem,tex", StemNotation::Latexmath)]
#[case("stem,invalid", StemNotation::Asciimath)]
#[case("stem,", StemNotation::Latexmath)]
#[case("asciimath", StemNotation::Asciimath)]
#[case("latexmath", StemNotation::Latexmath)]
fn stem_block_notation_overrides_the_document(
    #[case] style: &str,
    #[case] expected: StemNotation,
) -> TestResult {
    let source = format!(":stem: latexmath\n\n[{style}]\n++++\nα + β\n++++\n");
    let parsed = parse(&source, &Options::default())?;
    let stem = block_stem(&parsed)?;
    assert_eq!(stem.notation, expected);
    assert_eq!(stem.content, "α + β");
    Ok(())
}

#[test]
fn stem_inline_notation_overrides_the_document() -> TestResult {
    for (prefix, document, expected) in [
        ("latexmath", "asciimath", StemNotation::Latexmath),
        ("asciimath", "latexmath", StemNotation::Asciimath),
    ] {
        let source = format!(":stem: {document}\n\n{prefix}:[x^2]");
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(inline_stem(&parsed)?.notation, expected);
    }
    Ok(())
}

#[test]
fn stem_notation_follows_caller_values_and_body_assignments() -> TestResult {
    let options = Options::builder().with_attribute("stem", "tex").build()?;
    let parsed = parse(":stem: asciimath\n\nstem:[x^2]", &options)?;
    assert_eq!(inline_stem(&parsed)?.notation, StemNotation::Latexmath);
    let parsed = parse(
        ":stem: asciimath\n\nstem:[a]\n\n:stem: tex\n\nstem:[b]\n\n:stem!:\n\nstem:[c]",
        &Options::default(),
    )?;
    let notations = parsed
        .document()
        .blocks
        .iter()
        .filter_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            paragraph.content.iter().find_map(|node| {
                if let InlineNode::Macro(InlineMacro::Stem(stem)) = node {
                    Some(stem.notation.clone())
                } else {
                    None
                }
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        notations,
        [
            StemNotation::Asciimath,
            StemNotation::Latexmath,
            StemNotation::Asciimath
        ]
    );
    for alias in ["latexmath", "latex", "tex"] {
        assert_eq!(alias.parse::<StemNotation>()?, StemNotation::Latexmath);
    }
    assert!("invalid".parse::<StemNotation>().is_err());
    Ok(())
}

#[test]
fn stem_default_substitutions_preserve_attributes_and_formula_operators() -> TestResult {
    let parsed = parse(
        ":value: expanded\n\nBefore stem:[{value} *x* + y --] after {value}.\n\n[stem]\n++++\n{value} *x* +y+ --\n++++",
        &Options::default(),
    )?;
    assert_eq!(inline_stem(&parsed)?.content, "{value} *x* + y --");
    assert_eq!(block_stem(&parsed)?.content, "{value} *x* +y+ --");
    let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
        return Err("expected the first paragraph".into());
    };
    assert!(paragraph.content.iter().any(|node| {
        matches!(node, InlineNode::PlainText(text) if text.content.contains("expanded"))
    }));
    Ok(())
}

#[test]
fn stem_passthroughs_run_before_formula_attributes_are_protected() -> TestResult {
    for (formula, expected) in [
        ("{value} *x* +y+ --", "{value} *x* y --"),
        ("x=+{value}+", "x={value}"),
        ("x++{value}++", "x{value}"),
        ("x+++{value}+++", "x{value}"),
        ("x pass:[{value}]", "x {value}"),
    ] {
        let source = format!(":value: a longer value\n\nπ {{value}} stem:[{formula}] ω {{value}}.");
        let parsed = parse(&source, &Options::default())?;
        let stem = inline_stem(&parsed)?;
        assert_eq!(stem.content, expected);
        assert_eq!(stem.location.start.column, 11);
        assert_eq!(
            &source[stem.location.absolute_start..=stem.location.absolute_end],
            format!("stem:[{formula}]")
        );
    }
    Ok(())
}

#[test]
fn stem_bracket_escapes_preserve_unicode_source_locations() -> TestResult {
    let source = "π stem:[α \\in [0,1\\]] ω.";
    let parsed = parse(source, &Options::default())?;
    let stem = inline_stem(&parsed)?;
    assert_eq!(stem.content, "α \\in [0,1]");
    assert_eq!(stem.location.start.column, 3);
    assert_eq!(
        &source[stem.location.absolute_start..=stem.location.absolute_end],
        "stem:[α \\in [0,1\\]]"
    );
    Ok(())
}

#[test]
fn stem_keeps_its_bracket_escape_and_multiline_contracts() -> TestResult {
    for (source, expected) in [
        ("stem:[\\[x\\]]", "[x]"),
        ("stem:[a\\\\]", "a\\"),
        ("stem:[x\n  + y]", "x\n  + y"),
    ] {
        let parsed = parse(source, &Options::default())?;
        assert_eq!(inline_stem(&parsed)?.content, expected);
    }
    Ok(())
}

#[test]
fn stem_latex_removes_one_enclosing_dollar_pair() -> TestResult {
    for (source, expected) in [
        ("latexmath:[$x^2$]", "x^2"),
        ("latexmath:[$$x^2$$]", "$x^2$"),
        ("asciimath:[$x^2$]", "$x^2$"),
        ("latexmath:[$x^2]", "$x^2"),
    ] {
        let parsed = parse(source, &Options::default())?;
        let stem = inline_stem(&parsed)?;
        assert_eq!(stem.content, expected);
        assert_eq!(
            &source[stem.location.absolute_start..=stem.location.absolute_end],
            source
        );
    }
    Ok(())
}

#[test]
fn stem_formula_validity_is_separate_from_container_syntax() -> TestResult {
    for source in ["stem:[]", "latexmath:[\\notacommand{]", "stem:[α +]"] {
        let parsed = parse(source, &Options::default())?;
        inline_stem(&parsed)?;
        assert_eq!(parsed.warnings(), []);
    }
    let parsed = parse_inline("stem:[x^2", &Options::default())?;
    assert!(
        parsed
            .inlines()
            .iter()
            .all(|node| !matches!(node, InlineNode::Macro(InlineMacro::Stem(_))))
    );
    Ok(())
}

#[test]
fn stem_unterminated_block_retains_the_formula_and_warns() -> TestResult {
    let parsed = parse("[stem]\n++++\nα + β", &Options::default())?;
    assert_eq!(block_stem(&parsed)?.content, "α + β");
    assert_eq!(parsed.warnings().len(), 1);
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn stem_substitutions_expand_attributes_only_when_requested() -> TestResult {
    for (substitutions, expected) in [
        ("attributes", "expanded"),
        ("+attributes", "expanded"),
        ("none", "{value}"),
        ("-attributes", "{value}"),
    ] {
        let source =
            format!(":value: expanded\n\n[stem,subs=\"{substitutions}\"]\n++++\n{{value}}\n++++");
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(block_stem(&parsed)?.content, expected);
        let Some(Block::DelimitedBlock(block)) = parsed.document().blocks.first() else {
            return Err("expected the STEM block".into());
        };
        assert_eq!(block.source_text(), Some("{value}\n"));
    }
    let parsed = parse(
        ":value: expanded\n\n[subs=attributes]\nstem:[{value}]",
        &Options::default(),
    )?;
    let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
        return Err("expected a paragraph with macros disabled".into());
    };
    assert!(
        !paragraph
            .content
            .iter()
            .any(|node| matches!(node, InlineNode::Macro(InlineMacro::Stem(_))))
    );
    Ok(())
}

#[test]
fn stem_escaped_macros_still_expand_surrounding_attributes() -> TestResult {
    let parsed = parse(":value: expanded\n\n\\stem:[{value}]", &Options::default())?;
    let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
        return Err("expected a literal escaped macro".into());
    };
    let mut text = String::new();
    for node in &paragraph.content {
        let InlineNode::PlainText(plain) = node else {
            return Err("expected plain escaped macro text".into());
        };
        text.push_str(plain.content);
    }
    assert_eq!(text, "stem:[expanded]");
    Ok(())
}

struct Sources;

impl IncludeSourceProvider for Sources {
    fn open(&self, target: &IncludeSourceTarget) -> Result<IncludeSource, IncludeSourceError> {
        let name = match target {
            IncludeSourceTarget::File(path) => path.file_name().and_then(|name| name.to_str()),
            IncludeSourceTarget::Uri(_) | _ => None,
        };
        let text = match name {
            Some("outer.adoc") => ":stem: tex\ninclude::inner.adoc[lines=2..7]",
            Some("inner.adoc") => {
                "Not selected.\nπ stem:[α \\in [0,1\\]] ω.\n\n[asciimath]\n++++\nβ < γ\n++++\nNot selected."
            }
            _ => {
                return Err(IncludeSourceError::new(
                    IncludeSourceErrorKind::NotFound,
                    "unknown test source",
                ));
            }
        };
        Ok(IncludeSource::from_bytes(text.as_bytes().to_vec()))
    }
}

#[test]
fn stem_nested_selected_includes_keep_the_original_source_location() -> TestResult {
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_include_loader(IncludeLoader::Custom(Arc::new(Sources)))
        .build()?;
    let parsed = parse("include::outer.adoc[]", &options)?;
    let stem = inline_stem(&parsed)?;
    assert_eq!(stem.notation, StemNotation::Latexmath);
    assert_eq!(stem.content, "α \\in [0,1]");
    let source = parsed.source_location(&stem.location);
    assert_eq!(
        source.file.as_deref().and_then(Path::file_name),
        Some(Path::new("inner.adoc").as_os_str())
    );
    assert_eq!(
        (source.location.start.line, source.location.start.column),
        (2, 3)
    );
    assert_eq!(block_stem(&parsed)?.content, "β < γ");
    let Some(Block::DelimitedBlock(block)) = parsed.document().blocks.last() else {
        return Err("expected the included STEM block".into());
    };
    let open = parsed.source_location(
        block
            .open_delimiter_location
            .as_ref()
            .ok_or("missing opening delimiter")?,
    );
    assert_eq!(
        open.file.as_deref().and_then(Path::file_name),
        Some(Path::new("inner.adoc").as_os_str())
    );
    assert_eq!(open.location.start.line, 5);
    assert_eq!(block.source_text(), Some("β < γ\n"));
    Ok(())
}
