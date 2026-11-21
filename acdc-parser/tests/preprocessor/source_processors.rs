use std::{
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use acdc_parser::{
    CompatibilityMode, Error, IncludeLoader, IncludeSource, IncludeSourceError,
    IncludeSourceErrorKind, IncludeSourceTarget, Options, SafeMode,
    extensions::{
        BlockMacro, BlockMacroOutput, MacroExecutionContext, MacroInvocation, Registry, SourceEdit,
        SourceProcessorContext,
    },
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[rstest::rstest]
fn ordered_callbacks_share_committed_attributes_and_nested_origins(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let trace = Arc::new(Mutex::new(Vec::new()));
    let mut builder = Registry::builder();
    let events = trace.clone();
    builder.register_preprocessor("expand", move || {
        let events = events.clone();
        move |input: &str, _: &SourceProcessorContext<'_>| {
            assert!(input.starts_with("omit\nconfigure"));
            events.lock().map_err(|e| e.to_string())?.push("preprocess");
            Ok(vec![SourceEdit::Remove(1)])
        }
    })?;
    let events = trace.clone();
    builder.register(BlockMacro::new("configure", move || {
        let events = events.clone();
        move |call: &MacroInvocation<'_>, context: &mut MacroExecutionContext| {
            assert_eq!(call.location.location.start.line, 2);
            events.lock().map_err(|e| e.to_string())?.push("block");
            context.set_attribute("audience", "expert");
            Ok(BlockMacroOutput::None)
        }
    }))?;
    let events = trace.clone();
    builder.register_include_processor("virtual", move || {
        let events = events.clone();
        let mut ordinal = 0;
        move |target: &IncludeSourceTarget, context: &SourceProcessorContext<'_>| {
            let IncludeSourceTarget::File(path) = target else {
                return Ok(None);
            };
            assert_eq!(
                context
                    .attributes
                    .get("audience")
                    .and_then(acdc_parser::DocumentAttributeValue::text),
                Some("expert")
            );
            ordinal += 1;
            if path.ends_with("parts/expert.adoc") {
                assert_eq!((ordinal, context.line), (1, 5));
                events.lock().map_err(|e| e.to_string())?.push("outer");
                return Ok(Some(IncludeSource::from_string(
                    "ignored\ninclude::nested.adoc[lines=2]\nignored",
                )));
            }
            assert!(path.ends_with("parts/nested.adoc"));
            assert_eq!((ordinal, context.line), (2, 2));
            assert!(
                context
                    .file
                    .is_some_and(|path| path.ends_with("parts/expert.adoc"))
            );
            context.warn("nested virtual include");
            events.lock().map_err(|e| e.to_string())?.push("nested");
            Ok(Some(IncludeSource::from_string("ignored\nSelected.")))
        }
    })?;
    let registry = builder.build();
    let source = "omit\nconfigure::expert[]\n\nifdef::audience[]\ninclude::parts/{audience}.adoc[lines=2]\nendif::[]\n\nTail.";
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("main.adoc");
    fs::write(&file, source)?;
    let options = Options::builder()
        .with_base_dir(directory.path())
        .with_safe_mode(SafeMode::Safe)
        .with_compatibility_mode(mode)
        .build()?;
    for parsed in [
        registry.parse(source, &options)?,
        registry.parse_from_reader(source.as_bytes(), &options)?,
        registry.parse_file(&file, &options)?,
    ] {
        let result = parsed.finish()?;
        let conditional = result.conditionals().next().ok_or("missing conditional")?;
        assert_eq!(
            (conditional.start_line(), conditional.is_active()),
            (4, true)
        );
        let blocks: Vec<_> = result.document().blocks().collect();
        let selected = blocks
            .iter()
            .find(|b| b.location().start().include_chain().is_some())
            .ok_or("missing included block")?;
        assert_eq!(selected.location().start().line(), 2);
        assert_eq!(
            selected
                .location()
                .start()
                .include_chain()
                .ok_or("missing include chain")?
                .count(),
            2
        );
        assert_eq!(
            blocks
                .last()
                .ok_or("missing tail")?
                .location()
                .start()
                .line(),
            8
        );
        assert!(result.preprocessed_source().contains("Selected."));
        assert!(!result.preprocessed_source().contains("ignored"));
        assert_eq!(result.warnings().len(), 1);
        let warning = result
            .warnings()
            .first()
            .ok_or("missing include warning")?
            .source_location()
            .ok_or("unlocated warning")?;
        assert!(
            warning
                .file
                .as_ref()
                .is_some_and(|path| path.ends_with("parts/expert.adoc"))
        );
        assert_eq!(warning.location.start.line, 2);
        assert_eq!(warning.location.absolute_start, "ignored\n".len());
    }
    assert_eq!(
        *trace.lock().map_err(|e| e.to_string())?,
        ["preprocess", "block", "outer", "nested"].repeat(3)
    );
    Ok(())
}

#[test]
fn preprocessors_compose_and_retain_authored_positions() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_preprocessor("first", || {
        |_: &str, _: &SourceProcessorContext<'_>| {
            Ok(vec![
                SourceEdit::Replace {
                    line: 1,
                    text: "Generated one.\n\nGenerated two.".into(),
                },
                SourceEdit::Remove(3),
            ])
        }
    })?;
    builder.register_preprocessor("second", || {
        |source: &str, _: &SourceProcessorContext<'_>| {
            assert!(source.starts_with("Generated one.\n\nGenerated two."));
            Ok(vec![SourceEdit::Remove(1)])
        }
    })?;
    let result = builder
        .build()
        .parse("x\n\nomit\nTail.", &Options::default())?
        .finish()?;
    let positions: Vec<_> = result
        .document()
        .blocks()
        .map(acdc_parser::ast::BlockRef::location)
        .collect();
    let [generated, tail] = positions.as_slice() else {
        return Err("expected two paragraphs".into());
    };
    assert_eq!(generated.start().line(), 1);
    assert_eq!(generated.end().column(), 1);
    assert_eq!(generated.absolute_end(), 0);
    assert_eq!(tail.start().line(), 4);
    assert_eq!(tail.absolute_start(), 8);
    Ok(())
}

#[test]
fn policies_control_dispatch_and_fallback() -> TestResult {
    let count = Arc::new(AtomicUsize::new(0));
    let visits = count.clone();
    let mut builder = Registry::builder();
    builder.register_include_processor("virtual", move || {
        let visits = visits.clone();
        move |_: &IncludeSourceTarget, _: &SourceProcessorContext<'_>| {
            visits.fetch_add(1, Ordering::Relaxed);
            Ok(Some(IncludeSource::from_string("virtual")))
        }
    })?;
    let registry = builder.build();
    let include = "include::child.adoc[]";
    let remote = "include::https://example.invalid/doc.adoc[]";
    let denied = IncludeLoader::custom(|_: &IncludeSourceTarget| {
        Err(IncludeSourceError::new(
            IncludeSourceErrorKind::Unavailable,
            "denied",
        ))
    });
    for (input, options) in [
        (include, Options::default()),
        (
            include,
            Options::builder()
                .with_safe_mode(SafeMode::Safe)
                .with_include_loader(IncludeLoader::Disabled)
                .build()?,
        ),
        (
            include,
            Options::builder()
                .with_safe_mode(SafeMode::Safe)
                .with_include_loader(denied)
                .build()?,
        ),
        (
            include,
            Options::builder()
                .with_safe_mode(SafeMode::Safe)
                .with_attribute("max-include-depth", "0")
                .build()?,
        ),
        (
            ":allow-uri-read:\n\ninclude::https://example.invalid/doc.adoc[]",
            Options::builder().with_safe_mode(SafeMode::Safe).build()?,
        ),
        (
            remote,
            Options::builder()
                .with_safe_mode(SafeMode::Unsafe)
                .build()?,
        ),
        (
            "ifdef::missing[]\ninclude::ignored.adoc[]\nendif::[]\n\\include::ignored.adoc[]",
            Options::builder().with_safe_mode(SafeMode::Safe).build()?,
        ),
    ] {
        let result = registry.parse(input, &options)?.finish()?;
        assert!(!result.preprocessed_source().contains("virtual"));
        assert_eq!(count.load(Ordering::Relaxed), 0);
    }
    let allowed = Options::builder()
        .with_safe_mode(SafeMode::Safe)
        .with_attribute("allow-uri-read", true)
        .with_include_loader(IncludeLoader::custom(|target: &IncludeSourceTarget| {
            assert!(matches!(target, IncludeSourceTarget::Uri(_)));
            Ok(IncludeSource::from_string("permitted"))
        }))
        .build()?;
    assert_eq!(
        registry
            .parse(remote, &allowed)?
            .finish()?
            .preprocessed_source(),
        "virtual"
    );
    let limited = Options::builder()
        .with_safe_mode(SafeMode::Safe)
        .with_max_total_include_bytes(3)
        .build()?;
    assert!(matches!(
        registry.parse(include, &limited),
        Err(Error::IncludeExpansionTooLarge(..))
    ));
    assert_eq!(count.load(Ordering::Relaxed), 2);
    Ok(())
}

#[test]
fn invalid_source_edits_are_located() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_preprocessor("bad", || {
        |_: &str, _: &SourceProcessorContext<'_>| Ok(vec![SourceEdit::Remove(4)])
    })?;
    let Err(Error::Extension(location, message)) =
        builder.build().parse("One.", &Options::default())
    else {
        return Err("expected source edit error".into());
    };
    assert_eq!(location.location.start.line, 1);
    assert!(message.contains("line 4"));
    Ok(())
}

#[test]
fn include_failures_are_located_and_stop_the_chain() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_include_processor("fail", || {
        |_: &IncludeSourceTarget, _: &SourceProcessorContext<'_>| Err("failed include".into())
    })?;
    builder.register_include_processor("unreached", || {
        |_: &IncludeSourceTarget, _: &SourceProcessorContext<'_>| Err("unexpected call".into())
    })?;
    let options = Options::builder().with_safe_mode(SafeMode::Safe).build()?;
    let Err(Error::Extension(location, message)) = builder
        .build()
        .parse("Intro.\n\ninclude::child.adoc[]", &options)
    else {
        return Err("expected include error".into());
    };
    assert_eq!(location.location.start.line, 3);
    assert_eq!(message, "failed include");
    Ok(())
}

#[test]
fn include_registrations_compose_atomically_and_decline_to_the_loader() -> TestResult {
    let directory = tempfile::tempdir()?;
    fs::write(directory.path().join("part.adoc"), "disk")?;
    let options = Options::builder()
        .with_safe_mode(SafeMode::Safe)
        .with_base_dir(directory.path())
        .build()?;
    let mut builder = Registry::builder();
    let base = directory.path().to_owned();
    builder.register_include_processor("first", move || {
        let base = base.clone();
        move |target: &IncludeSourceTarget, _: &SourceProcessorContext<'_>| {
            assert_eq!(target, &IncludeSourceTarget::File(base.join("part.adoc")));
            Ok(None)
        }
    })?;
    let mut duplicate = Registry::builder();
    duplicate.register_preprocessor("unwanted", || {
        |_: &str, _: &SourceProcessorContext<'_>| Err("partial registry merge".into())
    })?;
    duplicate.register_include_processor("first", || {
        |_: &IncludeSourceTarget, _: &SourceProcessorContext<'_>| Err("duplicate processor".into())
    })?;
    assert!(builder.extend(&duplicate.build()).is_err());
    assert!(
        builder
            .register_preprocessor("-", || |_: &str, _: &SourceProcessorContext<'_>| Ok(vec![]))
            .is_err()
    );
    let registry = builder.build();
    let input = "include::../part.adoc[]";
    assert_eq!(
        registry
            .parse(input, &options)?
            .finish()?
            .preprocessed_source(),
        "disk"
    );
    let mut package = Registry::builder();
    package.register_include_processor("second", || {
        |_: &IncludeSourceTarget, _: &SourceProcessorContext<'_>| {
            Ok(Some(IncludeSource::from_string("override")))
        }
    })?;
    let mut all = Registry::builder();
    all.extend(&registry)?.extend(&package.build())?;
    assert_eq!(
        all.build()
            .parse(input, &options)?
            .finish()?
            .preprocessed_source(),
        "override"
    );
    Ok(())
}

#[test]
fn concurrent_parses_create_independent_include_processors() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_include_processor("count", || {
        let mut count = 0;
        move |_: &IncludeSourceTarget, _: &SourceProcessorContext<'_>| {
            count += 1;
            Ok(Some(IncludeSource::from_string(format!("call {count}"))))
        }
    })?;
    let registry = builder.build();
    let parse = || -> Result<(), Error> {
        let options = Options::builder().with_safe_mode(SafeMode::Safe).build()?;
        let parsed = registry
            .parse("include::one.adoc[]\n\ninclude::two.adoc[]", &options)?
            .finish()?;
        assert_eq!(parsed.preprocessed_source(), "call 1\n\ncall 2");
        Ok(())
    };
    std::thread::scope(|scope| -> TestResult {
        let other = scope.spawn(parse);
        parse()?;
        other.join().map_err(|_| "concurrent parse failed")??;
        Ok(())
    })
}
