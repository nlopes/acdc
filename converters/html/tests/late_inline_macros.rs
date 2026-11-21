//! Exercise the public inline registration and conversion boundary.
use acdc_converters_core::{Converter as _, Diagnostics, Options, pipeline::Pipeline};
use acdc_converters_html::{Converter, HtmlVariant};
use acdc_parser::{
    Options as ParserOptions,
    extensions::{
        InlineConversionContext, InlineInvocation, InlineMacroOutput, InlineMacroProcessor,
        InlineMatcher, Registry,
    },
};

struct Count(usize);
impl InlineMacroProcessor for Count {
    fn process(
        &mut self,
        call: &InlineInvocation<'_>,
        ctx: &mut InlineConversionContext<'_>,
    ) -> Result<InlineMacroOutput, String> {
        if call.body == "fail" {
            return Err("processor failed".into());
        }
        self.0 += 1;
        let count = ctx.increment("all");
        let configured = ctx
            .attribute("setting")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        let authored = ctx
            .attribute("audience")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        ctx.warn(format!(
            "{} {}",
            call.location.location.start.line, call.location.location.start.column
        ));
        Ok(InlineMacroOutput::Strong(format!(
            "{} {} {} {}/{} {} {} <{}>",
            ctx.backend,
            configured,
            authored,
            self.0,
            count,
            call.target,
            call.body,
            ctx.document.blocks().count()
        )))
    }
}

fn registry() -> Result<Registry, Box<dyn std::error::Error>> {
    let mut builder = Registry::builder();
    for (name, matcher) in [
        ("label", InlineMatcher::Full),
        ("tag", InlineMatcher::Short),
        (
            "delim",
            InlineMatcher::Delimited {
                open: "@(".into(),
                close: ")".into(),
            },
        ),
    ] {
        builder.register_inline_macro(name, matcher, || Count(0))?;
    }
    Ok(builder.build())
}

#[test]
fn repeated_and_parallel_conversions_expand_eligible_inline_macros()
-> Result<(), Box<dyn std::error::Error>> {
    let registry = registry()?;
    let source = ":audience: old\n\nπ label:rust[A & B] and tag:[short].\n\n:audience: new\n\n@(other) and pass:[tag:[skip]] and \\tag:[escaped].\n\n[subs=-macros]\ntag:[skip]\n\n----\ntag:[literal]\n----";
    let parsed = registry
        .parse(source, &ParserOptions::default())?
        .finish()?;
    assert!(parsed.document().has_inline_macros());
    let document = parsed.document();
    let render = |variant, setting: &'static str| -> Result<String, String> {
        let converter = Converter::new_with_variant(
            Options::builder().embedded(true).build(),
            ParserOptions::builder().with_attribute("setting", setting),
            variant,
        )
        .map_err(|e| e.to_string())?;
        let pipeline = Pipeline::new(converter).with_extensions(registry.clone());
        let mut output = Vec::new();
        let mut warnings = Vec::new();
        pipeline
            .write_to(
                document,
                &mut output,
                None,
                None,
                &mut Diagnostics::new(&pipeline.converter().warning_source(), &mut warnings),
            )
            .map_err(|e| e.to_string())?;
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        assert_eq!(warnings.first().map(|w| w.message.as_ref()), Some("3 3"));
        String::from_utf8(output).map_err(|e| e.to_string())
    };
    let (first, other) = std::thread::scope(|scope| {
        let one = scope.spawn(|| render(HtmlVariant::Standard, "first"));
        let two = scope.spawn(|| render(HtmlVariant::Semantic, "other"));
        (one.join(), two.join())
    });
    let first = first.map_err(|_| "first conversion panicked")??;
    let other = other.map_err(|_| "second conversion panicked")??;
    assert_eq!(first, render(HtmlVariant::Standard, "first")?);
    assert!(
        first.contains("html first old 1/1 rust A &amp; B &lt;"),
        "{first}"
    );
    assert!(first.contains("html first old 1/2 short short"));
    assert!(first.contains("html first new 1/3  other"));
    assert!(other.contains("html5s other old 1/1"));
    assert!(first.contains("tag:[escaped]"));
    assert!(first.contains("tag:[literal]"));
    Ok(())
}

#[test]
fn failed_processor_reports_its_location_before_writing() -> Result<(), Box<dyn std::error::Error>>
{
    let pipeline = Pipeline::new(Converter::new_with_variant(
        Options::default(),
        ParserOptions::builder(),
        HtmlVariant::Standard,
    )?)
    .with_extensions(registry()?);
    let parsed = pipeline.parse("ordinary text")?;
    assert!(!parsed.document().has_inline_macros());
    let parsed = pipeline.parse("before\n\ntag:[fail]")?;
    let mut output = Vec::new();
    let error = pipeline
        .write_to(
            parsed.document(),
            &mut output,
            None,
            None,
            &mut Diagnostics::new(&pipeline.converter().warning_source(), &mut Vec::new()),
        )
        .err()
        .ok_or("macro should fail")?;
    assert!(error.to_string().contains("processor failed"), "{error}");
    assert_eq!(output.as_slice(), b"");
    Ok(())
}

struct RemoveSection;
impl acdc_parser::extensions::TreeProcessor for RemoveSection {
    fn process(
        &mut self,
        editor: &mut acdc_parser::extensions::DocumentEditor<'_>,
    ) -> Result<(), acdc_parser::Error> {
        let target = editor
            .query()
            .select(acdc_parser::extensions::BlockContext::Section)
            .find(|section| section.output_id() == Some("omit"))
            .map(|section| section.target().clone());
        if let Some(target) = target {
            editor.remove_section(&target)?;
        }
        Ok(())
    }
}

#[test]
fn late_title_uses_source_context_and_runs_once_after_tree_edits()
-> Result<(), Box<dyn std::error::Error>> {
    let mut builder = Registry::builder();
    builder.extend(&registry()?)?;
    builder.register_tree_processor(acdc_parser::extensions::TreeProcessorRegistration::new(
        "remove",
        || RemoveSection,
    ))?;
    let registry = builder.build();
    let source = "= Doc\n:audience: early\n:setting: parsed\n:toc:\n\n[#omit]\n== Removed\n\ntag:[fail]\n\n[#keep]\n== label:section[{audience}/{setting}]\n\n:audience: later\n\nSee <<keep>>.";
    let parsed = registry
        .parse(source, &ParserOptions::default())?
        .finish()?;
    let converter = Converter::new_with_variant(
        Options::builder().embedded(true).build(),
        ParserOptions::builder().with_attribute("setting", "converted"),
        HtmlVariant::Standard,
    )?;
    let pipeline = Pipeline::new(converter).with_extensions(registry);
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    pipeline.write_to(
        parsed.document(),
        &mut output,
        None,
        None,
        &mut Diagnostics::new(&pipeline.converter().warning_source(), &mut warnings),
    )?;
    let html = String::from_utf8(output)?;
    assert_eq!(warnings.len(), 1, "{warnings:?} {html}");
    assert!(html.contains("early 1/1 section early/converted"), "{html}");
    assert!(!html.contains("Removed"));
    Ok(())
}

#[test]
fn included_macro_warnings_identify_the_file_and_column() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempfile::tempdir()?;
    let entry = directory.path().join("entry.adoc");
    let chapter = directory.path().join("chapter.adoc");
    std::fs::write(&entry, "= Entry\n\ninclude::chapter.adoc[]")?;
    std::fs::write(&chapter, "π tag:[two]")?;
    let pipeline = Pipeline::new(Converter::new_with_variant(
        Options::builder().embedded(true).build(),
        ParserOptions::builder().with_safe_mode(acdc_parser::SafeMode::Unsafe),
        HtmlVariant::Standard,
    )?)
    .with_extensions(registry()?);
    let parsed = pipeline.parse_file(&entry)?;
    let mut warnings = Vec::new();
    pipeline.write_to(
        parsed.document(),
        Vec::new(),
        Some(&entry),
        None,
        &mut Diagnostics::new(&pipeline.converter().warning_source(), &mut warnings),
    )?;
    let origin = warnings
        .first()
        .and_then(acdc_converters_core::Warning::source_location)
        .ok_or("expected a located inline warning")?;
    assert_eq!(origin.file.as_deref(), Some(chapter.as_path()));
    assert_eq!(
        (origin.location.start.line, origin.location.start.column),
        (1, 3)
    );
    Ok(())
}
