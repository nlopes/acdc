//! Check delivery boundaries and measure the output buffer independently of parser allocations.
use acdc_converters_core::{
    BackendTarget, Converter, Diagnostics, Options, OutputDestination,
    pipeline::{ExtensionCatalog, Extensions, Pipeline},
    postprocess::{PostprocessError, TextPostprocessor},
};
use acdc_parser::{Options as ParserOptions, OptionsBuilder, ast::DocumentRef};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};
use std::{
    alloc::System,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type TestResult = Result<(), Box<dyn std::error::Error>>;
#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Parser(#[from] acdc_parser::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}
struct Chunked {
    options: Options,
    parser: ParserOptions<'static>,
    utf8: bool,
    chunk: &'static [u8],
}
impl Converter<'static> for Chunked {
    type Error = Error;
    fn new(options: Options, parser: OptionsBuilder<'static>) -> Result<Self, Error> {
        Ok(Self {
            options,
            parser: parser.build()?,
            utf8: true,
            chunk: &[b'x'; 1024],
        })
    }
    fn options(&self) -> &Options {
        &self.options
    }
    fn parser_options(&self) -> &ParserOptions<'static> {
        &self.parser
    }
    fn name(&self) -> &'static str {
        "chunked"
    }
    fn supports_text_output(&self) -> bool {
        self.utf8
    }
    fn derive_output_path(&self, _: &Path, _: DocumentRef<'_>) -> Result<Option<PathBuf>, Error> {
        Ok(None)
    }
    fn write_to<W: Write>(
        &self,
        _: DocumentRef<'_>,
        mut writer: W,
        _: Option<&Path>,
        _: Option<&Path>,
        _: &mut Diagnostics<'_>,
    ) -> Result<(), Error> {
        for _ in 0..64 {
            writer.write_all(self.chunk)?;
        }
        Ok(())
    }
}
#[derive(Default)]
struct Sink {
    writes: usize,
    bytes: usize,
}
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.writes += 1;
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn only_selected_postprocessors_allocate_a_complete_output_buffer() -> TestResult {
    let mut catalog = ExtensionCatalog::default();
    for name in ["first", "second"] {
        catalog.register(
            name,
            Extensions::default().with_postprocessor(TextPostprocessor::new(
                "chunked",
                move |text, ctx, diag| {
                    assert_eq!(ctx.backend, "chunked");
                    assert!(ctx.source_file.is_none() && ctx.output_path.is_none());
                    if name == "first" {
                        assert_eq!(text.len(), 65_536);
                        text.truncate(1);
                    } else {
                        assert_eq!(text, "x");
                        diag.warn("completed");
                    }
                    Ok(())
                },
            )),
        )?;
    }
    let wrong = Extensions::default()
        .with_postprocessor(TextPostprocessor::new("other", |_, _, _| {
            Err("unexpected callback".into())
        }));
    let wrong_variant = Extensions::default().with_postprocessor(TextPostprocessor::for_target(
        BackendTarget::variant("chunked", "alternate"),
        |_, _, _| Err("unexpected variant callback".into()),
    ));
    for (extensions, active) in [
        (Extensions::default(), false),
        (wrong, false),
        (wrong_variant, false),
        (
            catalog
                .select(["first", "second", "first"])?
                .ok_or("missing selection")?,
            true,
        ),
    ] {
        let backend = Chunked::new(Options::default(), ParserOptions::builder())?;
        let pipeline = Pipeline::new(backend).with_extensions(extensions);
        let parsed = pipeline.parse("hello")?;
        for _ in 0..2 {
            let mut sink = Sink::default();
            let mut warnings = Vec::new();
            let source = pipeline.warning_source();
            let region = Region::new(ALLOCATOR);
            pipeline.write_to(
                parsed.document(),
                &mut sink,
                None,
                None,
                &mut Diagnostics::new(&source, &mut warnings),
            )?;
            let allocated = region.change().bytes_allocated;
            if active {
                assert_eq!((sink.writes, sink.bytes, warnings.len()), (1, 1, 1));
                assert!(allocated >= 65_536, "{allocated}");
            } else {
                assert_eq!((sink.writes, sink.bytes, warnings.len()), (64, 65_536, 0));
                assert!(allocated < 4096, "{allocated}");
            }
            eprintln!("active={active}, allocated={allocated}");
        }
    }
    output_errors_keep_existing_files_and_skip_later_processors()
}

fn output_errors_keep_existing_files_and_skip_later_processors() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("out");
    let input = dir.path().join("source.adoc");
    for (supported, valid) in [(false, true), (true, false), (true, true)] {
        let mut backend = Chunked::new(
            Options::builder()
                .output_destination(OutputDestination::File(path.clone()))
                .build(),
            ParserOptions::builder(),
        )?;
        backend.utf8 = supported;
        if !valid {
            backend.chunk = b"\xff";
        }
        let processors = Extensions::default()
            .with_postprocessor(TextPostprocessor::new("chunked", |_, ctx, _| {
                assert!(ctx.source_file.is_some_and(|p| p.ends_with("source.adoc")));
                assert!(ctx.output_path.is_some_and(|p| p.ends_with("out")));
                Err("failed".into())
            }))
            .with_postprocessor(TextPostprocessor::new("chunked", |_, _, _| {
                Err("unexpected later callback".into())
            }));
        let pipeline = Pipeline::new(backend).with_extensions(processors);
        let parsed = pipeline.parse("test")?;
        for exists in [false, true] {
            if exists {
                std::fs::write(&path, "original")?;
            }
            let Err(Error::Io(e)) = pipeline.convert(parsed.document(), Some(&input)) else {
                return Err("missing output error".into());
            };
            let error = e
                .get_ref()
                .and_then(|e| e.downcast_ref::<PostprocessError>())
                .ok_or("missing postprocessor error")?;
            match (supported, valid, error) {
                (false, _, PostprocessError::Unsupported("chunked"))
                | (true, false, PostprocessError::InvalidText { .. }) => (),
                (true, true, PostprocessError::Callback { source, .. }) => {
                    assert_eq!(source.to_string(), "failed");
                }
                _ => return Err(format!("unexpected error {error}").into()),
            }
            if exists {
                assert_eq!(std::fs::read_to_string(&path)?, "original");
                std::fs::remove_file(&path)?;
            } else {
                assert!(!path.exists());
            }
        }
    }
    Ok(())
}
