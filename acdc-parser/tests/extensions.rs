use acdc_parser::{
    CompatibilityMode, Error, Options, ParseResult,
    ast::{BlockKind, InlineKind},
    extensions::{
        BlockContext, BlockMacro, BlockMacroOutput, BlockMacroProcessor, MacroInvocation, Registry,
    },
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Defer;
impl BlockMacroProcessor for Defer {
    fn process(&mut self, _: &MacroInvocation<'_>) -> Result<BlockMacroOutput, String> {
        Ok(BlockMacroOutput::Deferred)
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
        let parsed: ParseResult = Registry::default()
            .parse("Ordinary content.", &options)?
            .finish()?;
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
