#![cfg(not(feature = "network"))]

use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{Block, InlineNode, Options, ParseResult, SafeMode, parse_file};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDocument {
    directory: PathBuf,
    path: PathBuf,
}

impl TempDocument {
    fn new(source: &str) -> io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "acdc-parser-uri-network-disabled-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&directory)?;
        let path = directory.join("main.adoc");
        fs::write(&path, source)?;
        Ok(Self { directory, path })
    }
}

impl Drop for TempDocument {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn paragraph_texts(result: &ParseResult) -> TestResult<Vec<String>> {
    result
        .document()
        .blocks
        .iter()
        .map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return Err(format!("expected paragraph, got {block:?}").into());
            };
            paragraph
                .content
                .iter()
                .map(|inline| {
                    let InlineNode::PlainText(text) = inline else {
                        return Err(format!("expected plain paragraph text, got {inline:?}").into());
                    };
                    Ok(text.content)
                })
                .collect::<TestResult<Vec<_>>>()
                .map(|parts| parts.concat())
        })
        .collect()
}

fn assert_warning(result: &ParseResult, uri: &str, source: &Path) -> TestResult {
    let [warning] = result.warnings() else {
        return Err(format!("expected one warning, got {:?}", result.warnings()).into());
    };
    assert_eq!(
        warning.kind.to_string(),
        format!("network support is disabled, cannot fetch remote includes: {uri}")
    );
    let Some(location) = warning.source_location() else {
        return Err("expected the warning to have a source location".into());
    };
    assert_eq!(location.file.as_deref(), Some(source));
    assert_eq!(location.location.start.line, 3);
    Ok(())
}

#[test]
fn authorized_uri_is_preserved_when_network_support_is_disabled() -> TestResult {
    let uri = "https://example.invalid/remote.adoc";
    let document = TempDocument::new(&format!("BEFORE\n\ninclude::{uri}[lines=1]\n\nAFTER"))?;
    let options = Options::builder()
        .with_safe_mode(SafeMode::Server)
        .with_attribute("allow-uri-read", true)
        .build();

    let result = parse_file(&document.path, &options)?;

    let unresolved = format!("Unresolved directive in main.adoc - include::{uri}[lines=1]");
    assert_eq!(
        paragraph_texts(&result)?,
        ["BEFORE".to_string(), unresolved, "AFTER".to_string()]
    );
    assert_warning(&result, uri, &document.path)?;

    let Some(Block::Paragraph(fallback)) = result.document().blocks.get(1) else {
        return Err("expected the fallback paragraph".into());
    };
    assert_eq!(fallback.location.start.line, 3);
    assert!(fallback.location.start.file.is_none());
    Ok(())
}
