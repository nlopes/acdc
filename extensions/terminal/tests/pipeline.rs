//! Native terminal selection preserves output semantics and immutable parsed documents.

use acdc_converters_core::{
    Converter as _, Diagnostics, Options, Warning,
    pipeline::{ExtensionCatalog, Pipeline},
};
use acdc_converters_html::{Converter, HtmlVariant};
use acdc_parser::{CompatibilityMode, Document, Options as ParserOptions, SafeMode};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn render(
    pipeline: &Pipeline<Converter<'_>>,
    document: acdc_parser::ast::DocumentRef<'_>,
) -> Result<(String, Vec<Warning>), acdc_converters_html::Error> {
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = pipeline.converter().warning_source();
    pipeline.write_to(
        document,
        &mut output,
        None,
        None,
        &mut Diagnostics::new(&source, &mut warnings),
    )?;
    Ok((String::from_utf8(output)?, warnings))
}

#[test]
fn terminal_selection_is_explicit_and_repeated_conversion_preserves_metadata_and_storage()
-> TestResult {
    let mut available = ExtensionCatalog::default();
    available.register("terminal", acdc_extensions_terminal::extension())?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
            let inactive = Pipeline::new(Converter::new_with_variant(
                Options::builder()
                    .embedded(true)
                    .safe_mode(SafeMode::Unsafe)
                    .build(),
                ParserOptions::builder().with_compatibility_mode(mode),
                variant,
            )?);
            let source = ":listing-caption: Screen\n\n.Session *title*\n[terminal#screen.role,cols=40,rows=4]\n----\n\x1b[32mhello\x1b[0m <unsafe>\n----\n\nSee <<screen>>.\n\n[terminal%replay,cols=40,rows=4]\n----\none\ntwo\n----";
            let parsed = inactive.parse(source)?;
            let original = Document::from(parsed.document());
            let (ordinary, warnings) = render(&inactive, parsed.document())?;
            assert!(!ordinary.contains("terminal-view"));
            assert!(warnings.is_empty(), "{warnings:?}");
            let selected = available.select(["terminal"])?.ok_or("selection")?;
            assert!(selected.parser_registry().is_none());
            let active = inactive.with_extensions(selected);
            let first = render(&active, parsed.document())?;
            let second = render(&active, parsed.document())?;
            assert_eq!(first.0, second.0);
            assert!(first.1.is_empty(), "{:?}", first.1);
            for fragment in [
                "terminal-view--replay",
                "id=\"screen\"",
                "role",
                "Screen 1.",
                "<strong>title</strong>",
                "href=\"#screen\"",
            ] {
                assert!(
                    first.0.contains(fragment),
                    "missing {fragment}: {mode:?}, {variant:?}"
                );
            }
            assert!(!first.0.contains("<unsafe>"));
            assert_eq!(Document::from(parsed.document()), original);
        }
    }
    Ok(())
}

#[test]
fn terminal_csp_and_safe_mode_follow_activation_and_warnings_stay_local() -> TestResult {
    for safe_mode in [
        SafeMode::Unsafe,
        SafeMode::Safe,
        SafeMode::Server,
        SafeMode::Secure,
    ] {
        let pipeline = Pipeline::new(Converter::new(
            Options::builder().safe_mode(safe_mode).build(),
            ParserOptions::builder(),
        )?)
        .with_extensions(acdc_extensions_terminal::extension());
        let parsed =
            pipeline.parse("= Terminal\n:csp:\n\n[terminal,cols=20,rows=3]\n----\nhello\n----")?;
        let (output, warnings) = render(&pipeline, parsed.document())?;
        assert!(output.contains(acdc_extensions_terminal::REPLAY_PLAYER_SCRIPT_CSP_HASH));
        if safe_mode < SafeMode::Server {
            assert!(output.contains("terminal-view"));
            assert!(warnings.is_empty(), "{warnings:?}");
        } else {
            assert!(!output.contains("class=\"terminal-view"));
            assert_eq!(warnings.len(), 1);
            assert!(
                warnings
                    .iter()
                    .all(|warning| warning.message.contains("safe mode"))
            );
        }
        let ordinary = pipeline.parse("plain")?;
        let (_, warnings) = render(&pipeline, ordinary.document())?;
        assert!(warnings.is_empty(), "{warnings:?}");
    }
    Ok(())
}
