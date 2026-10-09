use std::{
    collections::HashMap,
    error::Error,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use acdc_parser::{
    Block, IncludeLoader, IncludeSource, IncludeSourceError, IncludeSourceErrorKind,
    IncludeSourceProvider, IncludeSourceTarget, InlineMacro, InlineNode, Options, SafeMode, Source,
    parse, parse_file, parse_from_reader,
};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// One byte more than the limit for selected include text.
const OVERSIZED_SOURCE_BYTES: usize = 10 * 1024 * 1024 + 1;

#[derive(Default)]
struct MapSourceProvider {
    sources: HashMap<IncludeSourceTarget, Vec<u8>>,
    requests: Mutex<Vec<IncludeSourceTarget>>,
}

impl MapSourceProvider {
    fn new(sources: impl IntoIterator<Item = (IncludeSourceTarget, Vec<u8>)>) -> Self {
        Self {
            sources: sources.into_iter().collect(),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<IncludeSourceTarget> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl IncludeSourceProvider for MapSourceProvider {
    fn open(&self, target: &IncludeSourceTarget) -> Result<IncludeSource, IncludeSourceError> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(target.clone());
        self.sources
            .get(target)
            .cloned()
            .map(IncludeSource::from_bytes)
            .ok_or_else(|| {
                IncludeSourceError::new(
                    IncludeSourceErrorKind::NotFound,
                    format!("source not found: {target:?}"),
                )
            })
    }
}

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> std::io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "acdc-parser-source-provider-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn paragraph_text(result: &acdc_parser::ParseResult) -> Result<&str, Box<dyn Error>> {
    let [Block::Paragraph(paragraph)] = result.document().blocks.as_slice() else {
        return Err(format!("unexpected blocks: {:?}", result.document().blocks).into());
    };
    let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
        return Err(format!("unexpected paragraph: {paragraph:?}").into());
    };
    Ok(text.content)
}

fn file_target(path: impl AsRef<Path>) -> IncludeSourceTarget {
    IncludeSourceTarget::File(path.as_ref().to_path_buf())
}

/// Keep a shared provider handle so tests can inspect which targets were requested.
fn loader(provider: &Arc<MapSourceProvider>) -> IncludeLoader {
    let shared: Arc<MapSourceProvider> = Arc::clone(provider);
    IncludeLoader::Custom(shared)
}

#[test]
fn default_options_deny_includes_for_every_input_kind() -> TestResult {
    let directory = TempDirectory::new()?;
    let target = directory.0.join("part.adoc");
    let entry = directory.0.join("main.adoc");
    fs::write(&target, "PRIVATE CONTENT")?;
    let input = format!("include::{}[]", target.display());
    fs::write(&entry, &input)?;

    for options in [
        Options::default(),
        Options::builder().build()?,
        Options::default().into_static().into_builder().build()?,
    ] {
        assert_eq!(options.safe_mode, SafeMode::Secure);
        assert!(matches!(options.include_loader, IncludeLoader::System));
        for parsed in [
            parse(&input, &options)?,
            parse_from_reader(Cursor::new(&input), &options)?,
            parse_file(&entry, &options)?,
        ] {
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected a fallback paragraph".into());
            };
            let [InlineNode::Macro(InlineMacro::Link(link))] = paragraph.content.as_slice() else {
                return Err("expected an include link instead of private content".into());
            };
            assert_eq!(link.target, Source::Path(target.clone()));
            assert!(parsed.source_recovery().is_some());
        }
    }
    Ok(())
}

#[test]
fn custom_provider_and_uri_permission_do_not_lower_the_secure_default() -> TestResult {
    let provider = Arc::new(MapSourceProvider::default());
    let options = Options::builder()
        .with_include_loader(loader(&provider))
        .with_attribute("allow-uri-read", true)
        .build()?;
    let result = parse(
        "= Document\n:safe-mode-name: unsafe\n\ninclude::part.adoc[]\n\ninclude::https://example.test/part.adoc[]",
        &options,
    )?;
    assert_eq!(provider.requests(), []);
    assert_eq!(result.document().blocks.len(), 2);
    assert!(result.source_recovery().is_some());
    Ok(())
}

#[test]
fn disabled_loader_does_not_read_includes() -> TestResult {
    let directory = TempDirectory::new()?;
    fs::write(directory.0.join("part.adoc"), "DISK CONTENT")?;
    let options = Options::builder()
        .with_safe_mode(SafeMode::Server)
        .with_include_loader(IncludeLoader::Disabled)
        .with_base_dir(&directory.0)
        .build()?;
    let input = "include::part.adoc[]";

    let string_result = parse(input, &options)?;
    let reader_result = parse_from_reader(Cursor::new(input), &options)?;

    assert_eq!(paragraph_text(&string_result)?, input);
    assert_eq!(paragraph_text(&reader_result)?, input);
    assert!(!paragraph_text(&string_result)?.contains("DISK CONTENT"));
    assert!(!paragraph_text(&reader_result)?.contains("DISK CONTENT"));
    assert!(string_result.source_recovery().is_some());
    assert!(reader_result.source_recovery().is_some());
    Ok(())
}

#[test]
fn provider_loads_nested_sources_on_demand() -> TestResult {
    let directory = TempDirectory::new()?;
    let base_dir = directory.0.as_path();
    let outer = base_dir.join("chapters/one.adoc");
    let inner = base_dir.join("chapters/two.adoc");
    let provider = Arc::new(MapSourceProvider::new([
        (file_target(&outer), b"include::two.adoc[]".to_vec()),
        (file_target(&inner), b"NESTED CONTENT".to_vec()),
    ]));
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_include_loader(loader(&provider))
        .with_base_dir(base_dir)
        .build()?;

    let result = parse("include::chapters/one.adoc[]", &options)?;

    assert_eq!(paragraph_text(&result)?, "NESTED CONTENT");
    let block = result
        .document()
        .blocks
        .first()
        .ok_or("missing included block")?;
    assert_eq!(
        result.source_location(block.location()).file.as_ref(),
        Some(&inner)
    );
    assert_eq!(
        provider.requests(),
        [file_target(outer), file_target(inner)]
    );
    Ok(())
}

#[test]
fn provider_overlay_is_used_instead_of_the_filesystem() -> TestResult {
    let directory = TempDirectory::new()?;
    let target = directory.0.join("part.adoc");
    fs::write(&target, "DISK CONTENT")?;
    let provider = Arc::new(MapSourceProvider::new([(
        file_target(&target),
        b"OVERLAY CONTENT".to_vec(),
    )]));
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_include_loader(loader(&provider))
        .with_base_dir(&directory.0)
        .build()?;

    let result = parse("include::part.adoc[]", &options)?;

    assert_eq!(paragraph_text(&result)?, "OVERLAY CONTENT");
    assert_eq!(provider.requests(), [file_target(target)]);
    Ok(())
}

#[test]
fn safe_mode_confines_the_target_before_calling_the_provider() -> TestResult {
    let directory = TempDirectory::new()?;
    let base_dir = directory.0.as_path();
    let recovered = base_dir.join("secret.adoc");
    let provider = Arc::new(MapSourceProvider::new([(
        file_target(&recovered),
        b"CONFINED CONTENT".to_vec(),
    )]));
    let options = Options::builder()
        .with_include_loader(loader(&provider))
        .with_base_dir(base_dir)
        .with_safe_mode(SafeMode::Safe)
        .build()?;

    let result = parse("include::../secret.adoc[]", &options)?;

    assert_eq!(paragraph_text(&result)?, "CONFINED CONTENT");
    assert_eq!(provider.requests(), [file_target(recovered)]);
    let [warning] = result.warnings() else {
        return Err(format!("unexpected warnings: {:?}", result.warnings()).into());
    };
    assert_eq!(
        warning.kind.to_string(),
        "include file has illegal reference to ancestor of jail; recovering automatically"
    );
    Ok(())
}

#[test]
fn secure_mode_does_not_call_the_provider() -> TestResult {
    let provider = Arc::new(MapSourceProvider::default());
    let options = Options::builder()
        .with_include_loader(loader(&provider))
        .with_safe_mode(SafeMode::Secure)
        .build()?;

    let result = parse("include::secret.adoc[]", &options)?;

    assert_eq!(provider.requests(), []);
    assert!(result.source_recovery().is_some());
    // Secure mode degrades the include to Asciidoctor's link fallback; it must not
    // leave the directive sitting in the output as ordinary text.
    let [Block::Paragraph(paragraph)] = result.document().blocks.as_slice() else {
        return Err(format!("unexpected blocks: {:?}", result.document().blocks).into());
    };
    let [InlineNode::Macro(InlineMacro::Link(link))] = paragraph.content.as_slice() else {
        return Err(format!("unexpected paragraph: {paragraph:?}").into());
    };
    let Source::Path(target) = &link.target else {
        return Err(format!("unexpected link target: {:?}", link.target).into());
    };
    assert_eq!(target, Path::new("secret.adoc"));
    Ok(())
}

/// Secure mode produces the same link fallback with a custom or disabled loader.
#[test]
fn secure_mode_falls_back_identically_with_and_without_a_provider() -> TestResult {
    let provider = Arc::new(MapSourceProvider::default());
    let options = Options::builder()
        .with_include_loader(loader(&provider))
        .with_safe_mode(SafeMode::Secure)
        .build()?;
    let input = "include::secret.adoc[]";

    let with_provider = parse(input, &options)?;
    let mut denied = options.clone();
    denied.include_loader = IncludeLoader::Disabled;
    let without_provider = parse(input, &denied)?;

    assert_eq!(provider.requests(), []);
    let link_target = |result: &acdc_parser::ParseResult| -> Result<PathBuf, Box<dyn Error>> {
        let [Block::Paragraph(paragraph)] = result.document().blocks.as_slice() else {
            return Err(format!("unexpected blocks: {:?}", result.document().blocks).into());
        };
        let [InlineNode::Macro(InlineMacro::Link(link))] = paragraph.content.as_slice() else {
            return Err(format!("unexpected paragraph: {paragraph:?}").into());
        };
        let Source::Path(target) = &link.target else {
            return Err(format!("unexpected link target: {:?}", link.target).into());
        };
        Ok(target.clone())
    };

    assert_eq!(link_target(&with_provider)?, PathBuf::from("secret.adoc"));
    assert_eq!(
        link_target(&without_provider)?,
        PathBuf::from("secret.adoc")
    );
    Ok(())
}

#[test]
fn custom_provider_can_supply_authorized_uri_sources() -> TestResult {
    let uri = "https://example.test/part.adoc";
    let provider = Arc::new(MapSourceProvider::new([(
        IncludeSourceTarget::Uri(uri.to_string()),
        b"REMOTE CONTENT".to_vec(),
    )]));
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_include_loader(loader(&provider))
        .with_attribute("allow-uri-read", true)
        .build()?;

    let result = parse(&format!("include::{uri}[]"), &options)?;

    assert_eq!(paragraph_text(&result)?, "REMOTE CONTENT");
    assert_eq!(
        provider.requests(),
        [IncludeSourceTarget::Uri(uri.to_string())]
    );
    Ok(())
}

#[test]
fn provider_bytes_are_decoded_before_line_selection() -> TestResult {
    let directory = TempDirectory::new()?;
    let base_dir = directory.0.as_path();
    let target = base_dir.join("part.adoc");
    let provider = Arc::new(MapSourceProvider::new([(
        file_target(&target),
        b"\xEF\xBB\xBFFIRST\nSECOND\nTHIRD".to_vec(),
    )]));
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_include_loader(loader(&provider))
        .with_base_dir(base_dir)
        .build()?;

    let result = parse("include::part.adoc[lines=2]", &options)?;

    assert_eq!(paragraph_text(&result)?, "SECOND");
    assert_eq!(provider.requests(), [file_target(target)]);
    Ok(())
}

#[test]
fn provider_not_found_uses_normal_include_recovery() -> TestResult {
    let directory = TempDirectory::new()?;
    let base_dir = directory.0.as_path();
    let target = base_dir.join("missing.adoc");
    let provider = Arc::new(MapSourceProvider::default());
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_include_loader(loader(&provider))
        .with_base_dir(base_dir)
        .build()?;

    let result = parse("include::missing.adoc[]", &options)?;

    assert_eq!(
        paragraph_text(&result)?,
        "Unresolved directive in <stdin> - include::missing.adoc[]"
    );
    assert_eq!(provider.requests(), [file_target(&target)]);
    let [warning] = result.warnings() else {
        return Err(format!("unexpected warnings: {:?}", result.warnings()).into());
    };
    assert_eq!(
        warning.kind.to_string(),
        format!("include file not found: {}", target.display())
    );
    Ok(())
}

/// A provider can supply generated or remote bytes for a `File` target.
/// The size limit must apply to both file and URI targets, with the same error.
#[test]
fn oversized_file_source_is_rejected() -> TestResult {
    let directory = TempDirectory::new()?;
    let base_dir = directory.0.as_path();
    let target = base_dir.join("huge.adoc");
    let provider = Arc::new(MapSourceProvider::new([(
        file_target(&target),
        vec![b'a'; OVERSIZED_SOURCE_BYTES],
    )]));
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_include_loader(loader(&provider))
        .with_base_dir(base_dir)
        .build()?;

    let Err(error) = parse("include::huge.adoc[]", &options) else {
        return Err("expected an oversized file source to be rejected".into());
    };
    let acdc_parser::Error::IncludeSourceTooLarge(source) = error else {
        return Err(format!("unexpected error: {error:?}").into());
    };
    assert_eq!(source, target.display().to_string());
    Ok(())
}

#[test]
fn oversized_uri_source_is_rejected() -> TestResult {
    let uri = "https://example.test/huge.adoc";
    let provider = Arc::new(MapSourceProvider::new([(
        IncludeSourceTarget::Uri(uri.to_string()),
        vec![b'a'; OVERSIZED_SOURCE_BYTES],
    )]));
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_include_loader(loader(&provider))
        .with_attribute("allow-uri-read", true)
        .build()?;

    let Err(error) = parse(&format!("include::{uri}[]"), &options) else {
        return Err("expected an oversized uri source to be rejected".into());
    };
    let acdc_parser::Error::IncludeSourceTooLarge(source) = error else {
        return Err(format!("unexpected error: {error:?}").into());
    };
    assert_eq!(source, uri);
    Ok(())
}

#[test]
fn small_selections_from_large_local_and_custom_sources_keep_locations() -> TestResult {
    let directory = TempDirectory::new()?;
    let target = directory.0.join("large.adoc");
    let skipped = "x".repeat(OVERSIZED_SOURCE_BYTES);
    let content = format!("{skipped}\n// tag::sample[]\nChosen.\n// end::sample[]\n");
    fs::write(&target, &content)?;
    let provider = Arc::new(MapSourceProvider::new([
        (file_target(&target), content.as_bytes().to_vec()),
        (
            IncludeSourceTarget::Uri("https://example.test/large.adoc".to_string()),
            content.into_bytes(),
        ),
    ]));

    for (include_loader, include_target) in [
        (IncludeLoader::System, "large.adoc"),
        (loader(&provider), "large.adoc"),
        (loader(&provider), "https://example.test/large.adoc"),
    ] {
        let options = Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_include_loader(include_loader)
            .with_attribute("allow-uri-read", true)
            .with_base_dir(&directory.0)
            .build()?;
        for selector in ["lines=3", "lines=3..3", "tag=sample"] {
            let parsed = parse(&format!("include::{include_target}[{selector}]"), &options)?;
            assert_eq!(paragraph_text(&parsed)?, "Chosen.");
            assert_eq!(parsed.warnings(), []);
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected a paragraph".into());
            };
            assert_eq!(paragraph.location.start.line, 3);
        }
    }
    Ok(())
}

#[test]
fn provider_fatal_error_aborts_parsing() -> TestResult {
    let provider = |_target: &IncludeSourceTarget| {
        Err(IncludeSourceError::new(
            IncludeSourceErrorKind::Fatal,
            "document store is unavailable",
        ))
    };

    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_include_loader(IncludeLoader::custom(provider))
        .build()?;

    let Err(error) = parse("include::part.adoc[]", &options) else {
        return Err("expected the provider failure to abort parsing".into());
    };
    let acdc_parser::Error::IncludeSource(source_error) = error else {
        return Err(format!("unexpected error: {error:?}").into());
    };
    assert_eq!(source_error.kind(), IncludeSourceErrorKind::Fatal);
    assert_eq!(source_error.to_string(), "document store is unavailable");
    Ok(())
}

#[test]
fn file_input_honors_disabled_loader_after_options_round_trip() -> TestResult {
    let directory = TempDirectory::new()?;
    let entry = directory.0.join("main.adoc");
    fs::write(&entry, "include::part.adoc[]")?;
    fs::write(directory.0.join("part.adoc"), "DISK CONTENT")?;
    let options = Options::builder()
        .with_safe_mode(SafeMode::Server)
        .with_include_loader(IncludeLoader::Disabled)
        .build()?
        .into_static()
        .into_builder()
        .build()?;

    let mut result = parse_file(&entry, &options)?;
    assert_eq!(paragraph_text(&result)?, "include::part.adoc[]");
    assert!(result.source_recovery().is_some());
    result.take_warnings();
    assert!(result.source_recovery().is_some());
    Ok(())
}

#[test]
fn document_cannot_grant_uri_authority_to_a_custom_provider() -> TestResult {
    let provider = Arc::new(MapSourceProvider::default());
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_include_loader(loader(&provider))
        .build()?;
    let result = parse(
        "= Document\n:allow-uri-read:\n\ninclude::https://example.test/part.adoc[]",
        &options,
    )?;
    assert_eq!(provider.requests(), []);
    assert!(result.source_recovery().is_some());
    Ok(())
}

#[test]
fn provider_survives_options_round_trip_with_locked_attributes() -> TestResult {
    let directory = TempDirectory::new()?;
    let target = directory.0.join("part.adoc");
    let provider = Arc::new(MapSourceProvider::new([(
        file_target(&target),
        b"PROVIDER CONTENT".to_vec(),
    )]));
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_include_loader(loader(&provider))
        .with_base_dir(&directory.0)
        .with_attribute("part", "part.adoc")
        .build()?
        .into_static()
        .into_builder()
        .build()?;
    let result = parse(
        "= Document\n:part: ignored.adoc\n\ninclude::{part}[]",
        &options,
    )?;
    assert_eq!(paragraph_text(&result)?, "PROVIDER CONTENT");
    assert_eq!(provider.requests(), [file_target(target)]);
    Ok(())
}
