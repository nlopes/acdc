use acdc_parser::{
    CompatibilityMode, Error, Options, ParseResult,
    ast::{BlockKind, ElementIdRef, FootnoteDefinitionRef, InlineKind, ReferenceTargetRef},
    extensions::{
        BlockBuilder, BlockContext, BlockMacro, BlockMacroOutput, BlockMacroProcessor,
        DocumentEditor, MacroInvocation, Registry, TreeProcessor, TreeProcessorRegistration,
    },
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Defer;
impl BlockMacroProcessor for Defer {
    fn process(
        &mut self,
        _: &MacroInvocation<'_>,
        _: &mut acdc_parser::extensions::MacroExecutionContext,
    ) -> Result<BlockMacroOutput, String> {
        Ok(BlockMacroOutput::Deferred)
    }
}

struct Fill;
impl TreeProcessor for Fill {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let targets: Vec<_> = editor
            .deferred_blocks()
            .iter()
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
        builder.register_tree_processor(TreeProcessorRegistration::new("fill", || Fill))?;
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
                if let BlockKind::Table(table) = block.kind() {
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
