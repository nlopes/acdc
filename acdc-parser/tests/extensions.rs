use acdc_parser::{
    CompatibilityMode, Error, Options, ParseResult,
    ast::{
        BlockKind, CaptionRef, ElementIdRef, FootnoteDefinitionRef, InlineKind, ReferenceTargetRef,
    },
    extensions::{
        BlockBuilder, BlockContext, BlockMacro, BlockMacroOutput, BlockMacroProcessor,
        DocumentEditor, MacroExecutionContext, MacroInvocation, Registry, TitleSource,
        TreeProcessor, TreeProcessorRegistration,
    },
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Defer;
impl BlockMacroProcessor for Defer {
    fn process(
        &mut self,
        _: &MacroInvocation<'_>,
        _: &mut MacroExecutionContext,
    ) -> Result<BlockMacroOutput, String> {
        Ok(BlockMacroOutput::Deferred)
    }
}

struct ResolveOwn(&'static str);

impl TreeProcessor for ResolveOwn {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let own: Vec<_> = editor
            .deferred_blocks()
            .map(|request| {
                assert_eq!(request.invocation.name, self.0);
                (
                    request.target().clone(),
                    request.invocation.target.to_string(),
                )
            })
            .collect();
        assert_eq!(
            own.iter()
                .map(|(_, value)| value.as_str())
                .collect::<Vec<_>>(),
            ["one", "two"]
        );
        let others: Vec<_> = editor
            .query()
            .select(BlockContext::Extension)
            .map(|view| view.target().clone())
            .filter(|target| own.iter().all(|(owned, _)| owned != target))
            .collect();
        for target in others {
            assert!(
                editor
                    .replace_placeholder(&target, BlockBuilder::literal("wrong"))
                    .is_err()
            );
            assert!(
                editor
                    .replace_block(&target, BlockBuilder::literal("wrong"))
                    .is_err()
            );
            assert!(editor.discard_placeholder(&target).is_err());
        }
        for (target, value) in own {
            editor.replace_placeholder(&target, BlockBuilder::paragraph(value))?;
        }
        Ok(())
    }
}

#[test]
fn deferred_handlers_only_resolve_their_own_interleaved_requests() -> TestResult {
    for order in [["alpha", "beta"], ["beta", "alpha"]] {
        let mut builder = Registry::builder();
        builder.register_tree_processor(TreeProcessorRegistration::new("alpha", || RemoveFirst))?;
        for name in order {
            let mut package = Registry::builder();
            package.register(BlockMacro::new(name, || Defer))?;
            package
                .register_tree_processor(TreeProcessorRegistration::for_macro(name, move || {
                    ResolveOwn(name)
                }))?;
            builder.extend(&package.build())?;
        }
        let parsed = builder
            .build()
            .parse(
                "alpha::one[]\n\nbeta::one[]\n\nalpha::two[]\n\nbeta::two[]\n\n[#gone]\n== Removed\n\nalpha::cancel[]\n\nbeta::cancel[]",
                &Options::default(),
            )?
            .finish()?;
        assert_eq!(parsed.document().blocks().count(), 4);
    }
    Ok(())
}

#[test]
fn a_deferred_processor_requires_one_registered_macro() -> TestResult {
    use acdc_parser::extensions::RegistrationError;

    let mut builder = Registry::builder();
    let resolve = TreeProcessorRegistration::for_macro("alpha", || ResolveOwn("alpha"));
    assert!(matches!(
        builder.register_tree_processor(resolve.clone()),
        Err(RegistrationError::UnknownName(name)) if name == "alpha"
    ));
    builder.register(BlockMacro::new("alpha", || Defer))?;
    builder.register_tree_processor(resolve.clone())?;
    assert!(matches!(
        builder.register_tree_processor(resolve),
        Err(RegistrationError::DuplicateDeferredProcessor(name)) if name == "alpha"
    ));
    Ok(())
}

struct Fill;
impl TreeProcessor for Fill {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let targets: Vec<_> = editor
            .deferred_blocks()
            .map(|request| request.target().clone())
            .collect();
        for target in targets {
            assert!(matches!(
                editor.query().get(&target).map(|view| view.block().kind()),
                Some(BlockKind::Extension(_))
            ));
            editor.replace_placeholder(
                &target,
                BlockBuilder::unordered_list(["*literal* <&>", "{counter:x}"]),
            )?;
            assert!(matches!(
                editor.query().get(&target).map(|view| view.block().kind()),
                Some(BlockKind::UnorderedList(_))
            ));
        }
        Ok(())
    }
}

#[test]
fn extensions_finish_checks_pending_requests_and_returns_normal_parse_result() -> TestResult {
    fn require_send(_: impl Send) {}
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut builder = Registry::builder();
        builder.register(BlockMacro::new("request", || Defer))?;
        let pending = builder.build().parse("request::[]", &options)?;
        let error = pending
            .finish()
            .err()
            .ok_or("expected unresolved request")?;
        assert!(matches!(error, Error::Extension(..)));
        assert_eq!(
            error
                .source_location()
                .map(|source| source.location.start.line),
            Some(1)
        );

        let mut builder = Registry::builder();
        builder.register(BlockMacro::new("request", || Defer))?;
        builder
            .register_tree_processor(TreeProcessorRegistration::for_macro("request", || Fill))?;
        let parsed: ParseResult = builder
            .build()
            .parse(".A *title*\n[#list]\nrequest::[]", &options)?
            .finish()?;
        let block = parsed.document().blocks().next().ok_or("list")?;
        assert_eq!(block.id().map(ElementIdRef::as_str), Some("list"));
        assert!(block.title().is_some_and(|title| {
            title
                .inlines()
                .any(|inline| matches!(inline.kind(), InlineKind::Bold(_)))
        }));
        let BlockKind::UnorderedList(list) = block.kind() else {
            return Err("list kind".into());
        };
        let texts: Vec<_> = list
            .items()
            .filter_map(|item| {
                item.principal().inlines().find_map(|inline| {
                    if let InlineKind::Plain(text) = inline.kind() {
                        Some(text.text())
                    } else {
                        None
                    }
                })
            })
            .collect();
        assert_eq!(texts, ["*literal* <&>", "{counter:x}"]);
        require_send(parsed);
    }
    Ok(())
}

#[test]
fn extensions_typed_queries_preserve_prepared_reference_labels() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = Registry::default().parse(":label: Early\n\n.A *formatted* title\n[reftext='{label} *label*']\nimage::one.svg[]\n\n:label: Late", &options)?;
        let target = result
            .query()
            .select(BlockContext::Image)
            .next()
            .ok_or("image")?
            .target()
            .clone();
        let id = result.ensure_id(&target)?.to_owned();
        let query = result.query();
        let image = query.get(&target).ok_or("target")?;
        assert!(matches!(image.block().kind(), BlockKind::Image(_)));
        assert!(image.title().is_some_and(|title| {
            title
                .inlines()
                .any(|inline| matches!(inline.kind(), InlineKind::Bold(_)))
        }));
        assert!(image.attributes().is_some());
        let parsed = result.finish()?;
        let label = parsed
            .document()
            .references()
            .get(&id)
            .ok_or("reference")?
            .xreflabel()
            .ok_or("label")?;
        assert!(
            label
                .inlines()
                .any(|inline| matches!(inline.kind(), InlineKind::Bold(_)))
        );
        assert_eq!(parsed.warnings(), []);
    }
    Ok(())
}

struct RemoveFirst;
impl TreeProcessor for RemoveFirst {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let target = editor
            .query()
            .select(BlockContext::Section)
            .find(|view| view.output_id() == Some("gone"))
            .map(|view| view.target().clone());
        if let Some(target) = target {
            editor.remove_section(&target)?;
        }
        Ok(())
    }
}

#[test]
fn extensions_removal_preserves_cell_footnote_owners_and_prepared_numbers() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut builder = Registry::builder();
        builder
            .register_tree_processor(TreeProcessorRegistration::new("remove", || RemoveFirst))?;
        let source = "= Notes\n:sectnums:\n\n[#gone]\n== Removed\n\nfootnote:same[Outer definition <<gone>> [[inside]]]\n\n[#keep]\n== Kept\n\nfootnote:same[] and <<inside>>.\n\n[cols=a]\n|===\n|Cell footnote:same[Cell definition]\n|===\n";
        assert!(
            acdc_parser::parse(source, &options)?
                .document()
                .references()
                .get("inside")
                .is_some()
        );
        let parsed = builder.build().parse(source, &options)?.finish()?;
        assert!(parsed.warnings().iter().any(|warning| matches!(
            &warning.kind,
            acdc_parser::WarningKind::UnresolvedReference { target } if target == "gone"
        )));
        let document = parsed.document();
        assert_eq!(document.footnotes().count(), 1);
        assert!(document.references().get("inside").is_some());
        assert_eq!(
            document
                .references()
                .get("keep")
                .and_then(ReferenceTargetRef::section_number),
            Some("2")
        );
        let section = document
            .blocks()
            .find_map(|block| {
                if let BlockKind::Section(section) = block.kind() {
                    Some(section)
                } else {
                    None
                }
            })
            .ok_or("kept section")?;
        assert_eq!(section.number(), Some("2"));
        let table = section
            .blocks()
            .find_map(|block| {
                if let BlockKind::DelimitedTable(table) = block.kind() {
                    Some(table)
                } else {
                    None
                }
            })
            .ok_or("table")?;
        let cell = table
            .rows()
            .next()
            .and_then(|row| row.cells().next())
            .ok_or("cell")?;
        assert_eq!(cell.footnotes().count(), 1);
        assert_ne!(
            cell.footnotes().next().map(FootnoteDefinitionRef::number),
            document
                .footnotes()
                .next()
                .map(FootnoteDefinitionRef::number)
        );
    }
    Ok(())
}

#[test]
fn extensions_preserve_main_recovery_and_header_semantics() -> TestResult {
    let marker = ".".repeat(256);
    let sources = [
        ":toc:\n\n= Title\n\nBody.".to_owned(),
        "Before.\n\n|===\n|Cell\n|====\n\nLater cell.\n|===\n\nAfter.".to_owned(),
        "Before.\n\n|===\n|Cell\n|====\n\nLater cell.".to_owned(),
        "Before.\n\n[cols=4294967296a]\n|===\n|Cell\n|===\n\nAfter.".to_owned(),
        format!("Before.\n\n.Kept title\n[id=overflow]\n{marker} Literal item.\n\nAfter."),
        "Before.\n\nifeval::[1 = 1]\nHidden.\nendif::[]\n\nAfter.".to_owned(),
    ];
    let mut builder = Registry::builder();
    builder.register(BlockMacro::new("unused", || Defer))?;
    let registry = builder.build();
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for source in &sources {
            // Force the registered-name reader path without executing a callback.
            let source = format!("{source}\n\n\\unused::[]");
            let native = acdc_parser::parse(&source, &options)?;
            let extended = registry.parse(&source, &options)?.finish()?;
            assert_eq!(
                serde_json::to_value(extended.document())?,
                serde_json::to_value(native.document())?,
                "{mode:?}: {source}"
            );
            assert_eq!(extended.warnings(), native.warnings(), "{mode:?}: {source}");
        }
    }
    Ok(())
}

#[test]
fn extensions_preserve_scoped_include_assignments_and_origins() -> TestResult {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("outer.adoc"),
        "== Outer\n\ninclude::inner.adoc[leveloffset=+1]\n\nOuter after.",
    )?;
    std::fs::write(
        directory.path().join("inner.adoc"),
        "== Inner\n\nInner body.",
    )?;
    let source = "= Title\n:leveloffset: +1\n\ninclude::outer.adoc[leveloffset=+1]\n\n== After\n\nFinal body.";
    let mut builder = Registry::builder();
    builder.register(BlockMacro::new("unused", || Defer))?;
    let registry = builder.build();
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_base_dir(directory.path())
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?;
        let native = acdc_parser::parse(source, &options)?;
        let extended = registry.parse(source, &options)?.finish()?;
        assert_eq!(
            serde_json::to_value(extended.document())?,
            serde_json::to_value(native.document())?,
            "{mode:?}"
        );
        assert_eq!(extended.warnings(), native.warnings(), "{mode:?}");
    }
    Ok(())
}

struct ObserveLogo;

impl TreeProcessor for ObserveLogo {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        assert_eq!(editor.query().select(BlockContext::Image).count(), 1);
        assert_eq!(editor.query().select(BlockContext::Extension).count(), 0);
        Ok(())
    }
}

#[test]
fn committed_macro_returns_an_ordinary_image_at_its_include_location() -> TestResult {
    let directory = tempfile::tempdir()?;
    let included = directory.path().join("branding.adoc");
    std::fs::write(&included, ".The *logo*\n[#brand.wide]\nlogo::acme[]\n")?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(acdc_parser::SafeMode::Safe)
            .with_base_dir(directory.path())
            .build()?;
        for observe in [false, true] {
            let mut registry = Registry::builder();
            registry.register(BlockMacro::new("logo", || {
                let mut called = false;
                move |call: &MacroInvocation<'_>, _: &mut MacroExecutionContext| {
                    assert!(!std::mem::replace(&mut called, true));
                    assert_eq!(call.target, "acme");
                    Ok(BlockBuilder::image(format!("{}.svg", call.target)).into())
                }
            }))?;
            if observe {
                registry
                    .register_tree_processor(TreeProcessorRegistration::new("observe", || {
                        ObserveLogo
                    }))?;
            }
            let result = registry
                .build()
                .parse("Intro.\n\ninclude::branding.adoc[]", &options)?;
            assert!(result.deferred_blocks().is_empty());
            let logo = result
                .query()
                .select(BlockContext::Image)
                .next()
                .ok_or("missing image")?;
            assert_eq!(logo.output_id(), Some("brand"));
            assert_eq!(
                logo.source_location().file.as_deref(),
                Some(included.as_path())
            );
            // A block span starts at its first attached metadata line.
            assert_eq!(logo.source_location().location.start.line, 1);
            assert_eq!(logo.source_location().location.end.line, 3);
            assert_eq!(
                logo.title_source().map(TitleSource::raw_source),
                Some("The *logo*")
            );
            assert!(logo.title().is_some_and(|title| {
                title
                    .inlines()
                    .any(|inline| matches!(inline.kind(), InlineKind::Bold(_)))
            }));
            let BlockKind::Image(image) = logo.block().kind() else {
                return Err("missing image".into());
            };
            assert!(image.roles().contains("wide"));
            assert_eq!(image.source().to_string(), "acme.svg");
            assert!(
                matches!(logo.caption(), Some(CaptionRef::Numbered { number: Some(n), .. }) if n.get() == 1)
            );
            let parsed = result.finish()?;
            assert_eq!(parsed.warnings(), []);
        }
    }
    Ok(())
}

#[test]
fn committed_macro_interprets_builder_attributes_at_the_invocation() -> TestResult {
    let mut registry = Registry::builder();
    registry.register(BlockMacro::new("logo", || {
        |_: &MacroInvocation<'_>, _: &mut MacroExecutionContext| {
            Ok(BlockBuilder::image("brand.svg")
                .with_attributes("alt={label},width=120")
                .into())
        }
    }))?;
    let registry = registry.build();
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let result = registry.parse(
            ":label: Early\n\n[#brand.wide]\nlogo::acme[]\n\n:label: Late",
            &options,
        )?;
        let logo = result
            .query()
            .select(BlockContext::Image)
            .next()
            .ok_or("missing image")?;
        let BlockKind::Image(image) = logo.block().kind() else {
            return Err("missing image".into());
        };
        assert!(image.roles().contains("wide"));
        assert_eq!(logo.output_id(), Some("brand"));
        let alt = image
            .alt()
            .and_then(|alt| alt.inlines().next())
            .ok_or("missing alt")?;
        assert!(
            matches!(alt.kind(), InlineKind::Plain(plain) if plain.text() == "Early")
                || matches!(alt.kind(), InlineKind::Raw(raw) if raw.text() == "Early")
        );
        assert_eq!(alt.location().start().line(), 3);
        assert_eq!(alt.location().end().line(), 4);
    }
    Ok(())
}

#[test]
fn invalid_direct_macro_output_reports_the_included_invocation() -> TestResult {
    let directory = tempfile::tempdir()?;
    let included = directory.path().join("branding.adoc");
    std::fs::write(&included, "First.\n\nlogo::acme[]")?;
    let options = Options::builder()
        .with_safe_mode(acdc_parser::SafeMode::Safe)
        .with_base_dir(directory.path())
        .build()?;
    for invalid in [
        BlockBuilder::image("http://[invalid"),
        BlockBuilder::paragraph("text").with_id(""),
        BlockBuilder::literal("text").with_attributes("role=one\nrole=two"),
    ] {
        let mut registry = Registry::builder();
        registry.register(BlockMacro::new("logo", move || {
            let invalid = invalid.clone();
            move |_: &MacroInvocation<'_>, _: &mut MacroExecutionContext| Ok(invalid.clone().into())
        }))?;
        let error = registry
            .build()
            .parse("include::branding.adoc[]", &options)
            .err()
            .ok_or("expected invalid builder to fail")?;
        let Error::Extension(origin, _) = error else {
            return Err(error.into());
        };
        assert_eq!(origin.file.as_deref(), Some(included.as_path()));
        assert_eq!(origin.location.start.line, 3);
    }
    Ok(())
}

#[path = "extensions/raw_styles.rs"]
mod raw_styles;

#[path = "extensions/compound_styles.rs"]
mod compound_styles;

#[path = "extensions/parity.rs"]
mod parity;

#[path = "extensions/recovery.rs"]
mod recovery;
