use crate::{
    AttributeName, Block, CompatibilityMode, DocumentAttributeAssignment, DocumentAttributes,
    Error, IncludeLoader, Options, SafeMode,
    extensions::{BlockMacro, BlockMacroOutput, BlockMacroProcessor, MacroInvocation, Registry},
    grammar::SourceHandler,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Default)]
struct CountAccepted(usize);

impl SourceHandler for CountAccepted {
    fn accepted<'a>(
        &mut self,
        _: &Block<'a>,
        _: &DocumentAttributes<'a>,
    ) -> Result<Vec<(AttributeName<'a>, DocumentAttributeAssignment<'a>)>, String> {
        self.0 += 1;
        Ok(Vec::new())
    }
}

#[rstest::rstest]
#[case::paragraph("[role=note]\nA paragraph.")]
#[case::anchor("[[anchor]]\n.Title\nA paragraph.")]
#[case::listing("[source,rust]\n----\nlet answer = 42;\n----")]
#[case::image(".Caption\n[role=note]\nimage::target.png[]")]
fn source_driver_calls_handler_once_after_block_disambiguation(#[case] input: &str) -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut handler = CountAccepted::default();
        let parsed = crate::parse_with_source_handler(input, &options, None, Some(&mut handler))?;
        assert_eq!(parsed.document().blocks().count(), 1);
        assert_eq!(handler.0, 1);
    }
    Ok(())
}

// Keep the A1 seam private while measuring it in optimized builds. Run this
// ignored test alone through nextest with --release and --nocapture. The
// output directory and order can be set for paired, alternating runs.
#[test]
#[ignore = "on-demand Criterion measurement of the private source driver"]
fn benchmark_ordered_source_driver() -> TestResult {
    use std::{hint::black_box, time::Duration};

    use criterion::{BenchmarkId, Criterion};

    let output = std::env::var_os("ACDC_SOURCE_BENCH_OUTPUT")
        .ok_or("set ACDC_SOURCE_BENCH_OUTPUT to the Criterion output directory")?;
    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2))
        .nresamples(10_000)
        .without_plots()
        .output_directory(std::path::Path::new(&output));
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("part.adoc"),
        "Included paragraph.\n\n".repeat(100),
    )?;
    let options = Options::builder()
        .with_safe_mode(SafeMode::Safe)
        .with_base_dir(directory.path())
        .build()?;
    let nested = "====\n****\n* Parent\n** Child\n****\n====\n\n".repeat(1000);
    let includes = "include::part.adoc[]\n\n".repeat(20);
    let paragraphs = "A cheap callback follows this paragraph.\n\n".repeat(1000);
    let inputs = [
        ("basic", include_str!("../fixtures/tests/basic_header.adoc")),
        ("cheap_callbacks_1000", &paragraphs),
        ("large_nested", &nested),
        ("includes", &includes),
    ];
    let ordered_first = std::env::var_os("ACDC_SOURCE_BENCH_ORDERED_FIRST").is_some();
    let order = if ordered_first {
        [true, false]
    } else {
        [false, true]
    };
    let mut group = criterion.benchmark_group("source_driver");
    for (name, input) in inputs {
        for ordered in order {
            let path = if ordered { "ordered" } else { "native" };
            let mut handler = CountAccepted::default();
            let selected = ordered.then_some(&mut handler as &mut dyn SourceHandler);
            let _ = crate::parse_with_source_handler(input, &options, None, selected)?;
            if ordered && name == "cheap_callbacks_1000" {
                assert_eq!(handler.0, 1000);
            }
            group.bench_with_input(BenchmarkId::new(name, path), &input, |b, input| {
                b.iter(|| {
                    let mut handler = CountAccepted::default();
                    let selected = ordered.then_some(&mut handler as &mut dyn SourceHandler);
                    let parsed = crate::parse_with_source_handler(
                        black_box(input),
                        &options,
                        None,
                        selected,
                    );
                    black_box((parsed, handler.0))
                });
            });
        }
    }
    group.finish();
    Ok(())
}

#[rstest::rstest]
#[case::setext("Title\n=====\nAuthor Name\n:thing: Value\n\nBody.")]
#[case::comment("////\nIgnored.\n\n:thing: hidden\n////\n= Title\nAuthor Name\n\nBody.")]
#[case::comment_gap("////\nIgnored.\n////\n\n\n= Title\n\nBody.")]
#[case::attributes(":thing: Value\n// Retained.\n= Title\n\nBody.")]
#[case::leading_blanks("\n\n= Title\n\nBody.")]
#[case::empty("")]
#[case::blank("\n\n")]
#[case::normalized("\u{feff}= Title\r\n\r\nBody.\r\n")]
fn source_driver_preserves_prefix_forms(#[case] input: &str) -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        assert_equivalent(input, mode)?;
    }
    Ok(())
}

#[rstest::rstest]
#[case::prose("Ordinary café prose with a https://example.com[link].\n\nAfter.")]
#[case::colon_description("term:: Description.\nother:: Another item.")]
#[case::semicolon_description("term;; Description.\nother;; Another item.")]
#[case::setext("Heading\n-------\n\nBody.")]
#[case::media("image::example.png[]\n\nAfter.")]
#[case::admonition("NOTE: First line.\nSecond line.\n\nAfter.")]
fn source_driver_prose_filters_preserve_native_forms(#[case] input: &str) -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        assert_equivalent(input, mode)?;
    }
    Ok(())
}

#[test]
fn source_driver_pass_through_keeps_committed_attribute_timing() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let mut handler = Configure::default();
        let input = "configure::first[]\n\nValue {enabled}.";
        let parsed =
            crate::parse_with_source_handler(input, &options(mode)?, None, Some(&mut handler))?;
        assert_eq!(handler.events, ["first"]);
        assert_eq!(parsed.preprocessed_source(), input);
        assert!(serde_json::to_string(parsed.document_storage())?.contains("Value yes."));
    }
    Ok(())
}

#[test]
fn source_driver_resumes_attribute_driven_comment_styles() -> TestResult {
    let directory = tempfile::tempdir()?;
    std::fs::write(directory.path().join("part.adoc"), "Exposed include.")?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = options(mode)?
            .into_builder()
            .with_safe_mode(SafeMode::Safe)
            .with_base_dir(directory.path())
            .build()?;
        let input = "= Entry\n:enabled: comment\n\nconfigure::first[]\n\n[{enabled}]\n--\ninclude::part.adoc[]\n--";
        let parsed = crate::parse_with_source_handler(
            input,
            &options,
            None,
            Some(&mut Configure::default()),
        )?;
        assert!(parsed.preprocessed_source().contains("Exposed include."));
    }
    Ok(())
}

impl SourceHandler for () {
    fn accepted<'a>(
        &mut self,
        _: &Block<'a>,
        _: &DocumentAttributes<'a>,
    ) -> Result<Vec<(AttributeName<'a>, DocumentAttributeAssignment<'a>)>, String> {
        Ok(Vec::new())
    }
}

#[test]
fn source_driver_preserves_selected_include_origins_and_nested_base_directory() -> TestResult {
    let directory = tempfile::tempdir()?;
    std::fs::create_dir(directory.path().join("parts"))?;
    std::fs::write(
        directory.path().join("parts/outer.adoc"),
        "Excluded.\n// tag::selected[]\n    == Included\n\n    include::inner.adoc[]\n// end::selected[]\nExcluded again.",
    )?;
    std::fs::write(
        directory.path().join("parts/inner.adoc"),
        "[id=target]\nCafé 世界 {counter:unsupported}.\n",
    )?;
    let input =
        "= Entry\n\ninclude::parts/outer.adoc[tag=selected,indent=0,leveloffset=+1]\n\nAfter.";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let prepared = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Safe)
            .with_include_loader(IncludeLoader::System)
            .with_base_dir(directory.path())
            .build()?
            .prepare_for_parse(crate::document_attribute::InputKind::String);
        let warnings = std::rc::Rc::default();
        let preprocessed = crate::preprocessor::Preprocessor::process(
            input,
            &prepared,
            std::rc::Rc::clone(&warnings),
        )?;
        let expected = crate::parse_input(preprocessed, prepared.clone(), None, warnings, None)?;
        let actual = crate::parse_ordered_source(
            input,
            prepared,
            None,
            std::rc::Rc::default(),
            None,
            &mut (),
        )?;
        assert_eq!(actual.preprocessed_source(), expected.preprocessed_source());
        assert_eq!(actual.document_storage(), expected.document_storage());
        assert_eq!(actual.warnings(), expected.warnings());
        assert!(
            actual.warnings().iter().any(|warning| matches!(
                warning.kind,
                crate::WarningKind::UnsupportedCounter { .. }
            ))
        );
    }
    Ok(())
}

#[test]
fn nested_list_id_keeps_its_authored_position_in_a_compound_body() -> TestResult {
    let input = "Before.\n\n====\n. Parent\n[id=child]\n.. Child\n====";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = crate::parse(input, &options)?;
        let Some(Block::DelimitedBlock(example)) = parsed.document_storage().blocks.get(1) else {
            return Err("missing example".into());
        };
        let crate::DelimitedBlockType::DelimitedExample(body) = &example.inner else {
            return Err("missing example body".into());
        };
        let Some(Block::OrderedList(parent)) = body.first() else {
            return Err("missing parent list".into());
        };
        let Some(Block::OrderedList(child)) =
            parent.items.first().and_then(|item| item.blocks.first())
        else {
            return Err("missing child list".into());
        };
        let id = child.metadata.id.as_ref().ok_or("missing child ID")?;
        assert_eq!(id.location.start, crate::Position::new(5, 5));
        assert_eq!(
            id.location.absolute_start,
            input.find("child").ok_or("missing authored ID")?
        );
    }
    Ok(())
}

pub(crate) fn assert_equivalent(input: &str, mode: CompatibilityMode) -> TestResult {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    for input in [
        input.to_owned(),
        format!("ifdef::source-driver-disabled[]\nremoved\nendif::[]\n{input}"),
    ] {
        let prepared = options
            .clone()
            .prepare_for_parse(crate::document_attribute::InputKind::String);
        let warnings = std::rc::Rc::default();
        let preprocessed = crate::preprocessor::Preprocessor::process(
            &input,
            &prepared,
            std::rc::Rc::clone(&warnings),
        )?;
        let expected = crate::parse_input(preprocessed, prepared.clone(), None, warnings, None)?;
        let actual = crate::parse_ordered_source(
            &input,
            prepared,
            None,
            std::rc::Rc::default(),
            None,
            &mut (),
        )?;
        assert_eq!(
            actual.preprocessed_source(),
            expected.preprocessed_source(),
            "{mode:?}"
        );
        assert!(
            actual.document_storage() == expected.document_storage(),
            "{mode:?}: {}",
            difference(
                &serde_json::to_value(actual.document_storage())?,
                &serde_json::to_value(expected.document_storage())?,
                "document"
            )
            .unwrap_or_else(|| "unserialized document state differs".into())
        );
        assert_eq!(actual.warnings(), expected.warnings(), "{mode:?}");
    }
    Ok(())
}

fn difference(
    actual: &serde_json::Value,
    expected: &serde_json::Value,
    path: &str,
) -> Option<String> {
    if actual == expected {
        return None;
    }
    match (actual, expected) {
        (serde_json::Value::Object(actual), serde_json::Value::Object(expected)) => {
            for key in actual.keys().chain(expected.keys()) {
                if let Some(difference) =
                    difference(&actual[key], &expected[key], &format!("{path}.{key}"))
                {
                    return Some(difference);
                }
            }
        }
        (serde_json::Value::Array(actual), serde_json::Value::Array(expected))
            if actual.len() == expected.len() =>
        {
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                if let Some(difference) = difference(actual, expected, &format!("{path}[{index}]"))
                {
                    return Some(difference);
                }
            }
        }
        _ => return Some(format!("{path}: actual {actual}, expected {expected}")),
    }
    None
}

#[rstest::rstest]
#[case::unordered("* Parent\n+\n")]
#[case::ordered(". Parent\n+\n")]
#[case::nested("* Parent\n** Child\n+\n")]
#[case::description("term:: Parent\n+\n")]
#[case::callout("----\ncode <1>\n----\n\n<1> Parent\n+\n")]
fn source_driver_commits_list_attachment_before_later_directives(
    #[case] prefix: &str,
) -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = options(mode)?;
        let input = format!(
            "{prefix}configure::attached[]\n\nifdef::enabled[]\nSelected {{enabled}}.\nendif::[]"
        );
        let mut handler = Configure::default();
        let parsed = crate::parse_with_source_handler(&input, &options, None, Some(&mut handler))?;
        assert_eq!(handler.events, ["attached"]);
        assert!(
            parsed
                .conditionals()
                .next()
                .ok_or("missing conditional")?
                .is_active()
        );
        assert!(serde_json::to_string(parsed.document_storage())?.contains("Selected yes."));
    }
    Ok(())
}

#[test]
fn source_driver_preserves_reader_attributes_from_parser_lookahead() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = options(mode)?;
        let mut handler = Configure::default();
        let parsed = crate::parse_with_source_handler(
            "Paragraph.\n:reader-only:\n\nconfigure::first[]\n\nifdef::reader-only[]\nSelected {enabled}.\nendif::[]",
            &options,
            None,
            Some(&mut handler),
        )?;
        assert!(
            parsed
                .conditionals()
                .next()
                .ok_or("missing conditional")?
                .is_active()
        );
        assert!(serde_json::to_string(parsed.document_storage())?.contains("Selected yes."));
    }
    Ok(())
}

#[rstest::rstest]
#[case::compound("--\n", "\n--", true)]
#[case::table_cell("[cols=a]\n|===\na|", "\n|===", false)]
fn source_driver_preserves_buffered_body_and_cell_attribute_scope(
    #[case] prefix: &str,
    #[case] suffix: &str,
    #[case] propagates: bool,
) -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = options(mode)?;
        let input = format!(
            "{prefix}configure::inside[]\n\nifdef::enabled[]\nEarly.\nendif::[]{suffix}\n\nifdef::enabled[]\nLater.\nendif::[]"
        );
        let mut handler = Configure::default();
        let parsed = crate::parse_with_source_handler(&input, &options, None, Some(&mut handler))?;
        assert_eq!(handler.events, ["inside"]);
        assert_eq!(
            parsed
                .conditionals()
                .map(crate::ConditionalEvaluation::is_active)
                .collect::<Vec<_>>(),
            [true, propagates]
        );
        assert!(parsed.preprocessed_source().contains("Early."));
        assert_eq!(parsed.preprocessed_source().contains("Later."), propagates);
    }
    Ok(())
}

#[test]
fn source_driver_respects_caller_attribute_locks_for_parser_and_reader() -> TestResult {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("caller.adoc"),
        "Caller file {enabled}.",
    )?;
    std::fs::write(directory.path().join("yes.adoc"), "Wrong file.")?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = options(mode)?
            .into_builder()
            .with_safe_mode(SafeMode::Safe)
            .with_include_loader(IncludeLoader::System)
            .with_base_dir(directory.path())
            .with_attribute("enabled", "caller")
            .build()?;
        let mut handler = Configure::default();
        let parsed = crate::parse_with_source_handler(
            "configure::locked[]\n\ninclude::{enabled}.adoc[]",
            &options,
            None,
            Some(&mut handler),
        )?;
        assert_eq!(handler.events, ["locked"]);
        assert!(parsed.preprocessed_source().contains("Caller file"));
        assert!(serde_json::to_string(parsed.document_storage())?.contains("Caller file caller."));
    }
    Ok(())
}

struct Failed;

struct FailedProse;

impl SourceHandler for FailedProse {
    fn accepted<'a>(
        &mut self,
        block: &Block<'a>,
        _: &DocumentAttributes<'a>,
    ) -> Result<Vec<(AttributeName<'a>, DocumentAttributeAssignment<'a>)>, String> {
        if matches!(block, Block::Paragraph(_)) {
            return Err("stop after the committed paragraph".into());
        }
        Ok(Vec::new())
    }
}

#[test]
fn source_driver_buffered_prose_commits_before_later_include_reads() -> TestResult {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let reads = Arc::new(AtomicUsize::default());
        let provider_reads = Arc::clone(&reads);
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Safe)
            .with_base_dir("/virtual")
            .with_include_loader(IncludeLoader::custom(
                move |_: &crate::IncludeSourceTarget| {
                    provider_reads.fetch_add(1, Ordering::Relaxed);
                    Ok(crate::IncludeSource::from_bytes(
                        b"First paragraph.\n\nSecond paragraph.\n\n".to_vec(),
                    ))
                },
            ))
            .build()?;
        let result = crate::parse_with_source_handler(
            "include::part.adoc[]\n\ninclude::later.adoc[]",
            &options,
            None,
            Some(&mut FailedProse),
        );
        assert!(matches!(result, Err(Error::Extension(..))), "{result:?}");
        assert_eq!(reads.load(Ordering::Relaxed), 1);
    }
    Ok(())
}

#[rstest::rstest]
#[case::unordered("* Parent\n** Child\n+\n")]
#[case::ordered(". Parent\n.. Child\n+\n")]
#[case::description("* Parent\nterm:: Child\n+\n")]
fn source_driver_fatal_reader_error_escapes_nested_recovery(#[case] prefix: &str) -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Safe)
            .with_base_dir("/virtual")
            .with_include_loader(IncludeLoader::custom(|_: &crate::IncludeSourceTarget| {
                Err(crate::IncludeSourceError::new(
                    crate::IncludeSourceErrorKind::Fatal,
                    "document store is unavailable",
                ))
            }))
            .build()?;
        let input = format!("{prefix}include::part.adoc[]");
        for ordered in [false, true] {
            let mut handler = ();
            let selected = ordered.then_some(&mut handler as &mut dyn SourceHandler);
            let result = crate::parse_with_source_handler(&input, &options, None, selected);
            assert!(
                matches!(&result, Err(Error::IncludeSource(error)) if error.kind() == crate::IncludeSourceErrorKind::Fatal),
                "{mode:?}, ordered={ordered}: {result:?}"
            );
        }
    }
    Ok(())
}

impl SourceHandler for Failed {
    fn accepted<'a>(
        &mut self,
        block: &Block<'a>,
        _: &DocumentAttributes<'a>,
    ) -> Result<Vec<(AttributeName<'a>, DocumentAttributeAssignment<'a>)>, String> {
        if matches!(block, Block::Extension(_)) {
            Err("stop source processing".into())
        } else {
            Ok(Vec::new())
        }
    }
}

struct CountReads(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl crate::IncludeSourceProvider for CountReads {
    fn open(
        &self,
        _: &crate::IncludeSourceTarget,
    ) -> Result<crate::IncludeSource, crate::IncludeSourceError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(crate::IncludeSource::from_bytes(b"Later include.".to_vec()))
    }
}

#[rstest::rstest]
#[case::top("", "")]
#[case::section("== Section\n\n", "")]
#[case::list("* Item\n+\n", "")]
#[case::description("term:: Item\n+\n", "")]
#[case::compound("--\n", "\n--")]
#[case::table_cell("[cols=a]\n|===\na|", "\n|===")]
fn source_driver_handler_failure_prevents_later_include_reads(
    #[case] prefix: &str,
    #[case] suffix: &str,
) -> TestResult {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let reads = Arc::new(AtomicUsize::default());
        let options = options(mode)?
            .into_builder()
            .with_safe_mode(SafeMode::Safe)
            .with_include_loader(IncludeLoader::Custom(Arc::new(CountReads(Arc::clone(
                &reads,
            )))))
            .with_base_dir("/virtual")
            .build()?;
        let input = format!("{prefix}configure::fail[]{suffix}\n\ninclude::later.adoc[]");
        let result = crate::parse_with_source_handler(&input, &options, None, Some(&mut Failed));
        assert!(matches!(result, Err(Error::Extension(..))), "{result:?}");
        assert_eq!(reads.load(Ordering::Relaxed), 0);
    }
    Ok(())
}

struct Never;

impl BlockMacroProcessor for Never {
    fn process(&mut self, _: &MacroInvocation<'_>) -> Result<BlockMacroOutput, String> {
        Err("public callbacks must retain their post-acceptance boundary".into())
    }
}

#[derive(Default)]
struct Configure {
    events: Vec<String>,
}

impl SourceHandler for Configure {
    fn accepted<'a>(
        &mut self,
        block: &Block<'a>,
        _: &DocumentAttributes<'a>,
    ) -> Result<Vec<(AttributeName<'a>, DocumentAttributeAssignment<'a>)>, String> {
        let Block::Extension(extension) = block else {
            return Ok(Vec::new());
        };
        self.events.push(extension.invocation.target.to_string());
        Ok(vec![(
            "enabled".into(),
            DocumentAttributeAssignment::Set("yes".into()),
        )])
    }
}

fn options(mode: CompatibilityMode) -> Result<Options<'static>, Box<dyn std::error::Error>> {
    let mut registry = Registry::builder();
    registry.register(BlockMacro::new("configure", || Never))?;
    let mut options = Options::builder().with_compatibility_mode(mode).build()?;
    options.extensions = Some(registry.build());
    Ok(options)
}

#[test]
fn source_driver_commits_macro_before_reading_later_conditionals() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = options(mode)?;
        let mut handler = Configure::default();
        let parsed = crate::parse_with_source_handler(
            "configure::first[]\n\nifdef::enabled[]\nSelected {enabled}.\nendif::[]",
            &options,
            None,
            Some(&mut handler),
        )?;
        assert_eq!(handler.events, ["first"]);
        assert!(parsed.preprocessed_source().contains("Selected {enabled}."));
        assert!(
            parsed
                .conditionals()
                .next()
                .ok_or("missing conditional")?
                .is_active()
        );
        let document = serde_json::to_string(parsed.document_storage())?;
        assert!(document.contains("Selected yes."), "{document}");
    }
    Ok(())
}

#[test]
fn source_driver_commits_inside_include_before_later_directives() -> TestResult {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("outer.adoc"),
        "== Included\n\nconfigure::inside[]\n\nifdef::enabled[]\ninclude::inner.adoc[leveloffset=+1]\nendif::[]",
    )?;
    std::fs::write(
        directory.path().join("inner.adoc"),
        "== Inner\n\nSelected {enabled}.",
    )?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = options(mode)?
            .into_builder()
            .with_safe_mode(SafeMode::Safe)
            .with_include_loader(IncludeLoader::System)
            .with_base_dir(directory.path())
            .build()?;
        let mut handler = Configure::default();
        let parsed = crate::parse_with_source_handler(
            "include::outer.adoc[]\n\n== After\n\nLast.",
            &options,
            None,
            Some(&mut handler),
        )?;
        assert_eq!(handler.events, ["inside"]);
        let document = serde_json::to_string(parsed.document_storage())?;
        assert!(document.contains("Selected yes."), "{document}");
        assert!(document.contains("inner.adoc"));
        assert!(parsed.preprocessed_source().contains("== Inner"));
    }
    Ok(())
}
