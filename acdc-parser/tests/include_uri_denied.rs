use std::{
    error::Error,
    fs, io,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{Block, InlineMacro, InlineNode, Options, ParseResult, SafeMode, parse_file};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDocument {
    directory: PathBuf,
    path: PathBuf,
}

impl TempDocument {
    fn new(source: &str) -> io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "acdc-parser-uri-denied-{}-{sequence}",
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

fn assert_plain_paragraph(block: &Block<'_>, expected: &str) -> TestResult {
    let Block::Paragraph(paragraph) = block else {
        return Err(format!("expected paragraph, got {block:?}").into());
    };
    let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
        return Err(format!("expected plain paragraph text, got {paragraph:?}").into());
    };
    assert_eq!(text.content, expected);
    Ok(())
}

fn assert_denied_uri_fallback(result: &ParseResult, target: &str) -> TestResult {
    let [before, Block::Paragraph(fallback), after] = result.document().blocks.as_slice() else {
        return Err(format!(
            "expected before, fallback, and after paragraphs, got {:?}",
            result.document().blocks
        )
        .into());
    };
    assert_plain_paragraph(before, "BEFORE")?;
    assert_plain_paragraph(after, "AFTER")?;

    let [InlineNode::Macro(InlineMacro::Link(link))] = fallback.content.as_slice() else {
        return Err(format!("expected one fallback link, got {:?}", fallback.content).into());
    };
    assert_eq!(link.target.to_string(), target);
    assert!(link.text.is_empty());
    assert_eq!(link.attributes.iter().count(), 1);
    assert_eq!(
        link.attributes.get_string("role").as_deref(),
        Some("include")
    );
    assert_eq!(fallback.location.start.line, 3);
    assert!(fallback.location.start.file.is_none());
    assert!(result.warnings().is_empty());
    Ok(())
}

#[test]
fn caller_denied_uri_uses_link_fallback_and_continues_in_non_secure_modes() -> TestResult {
    let target = "https://example.invalid/secret.adoc";
    let document = TempDocument::new(&format!("BEFORE\n\ninclude::{target}[]\n\nAFTER"))?;

    for safe_mode in [SafeMode::Unsafe, SafeMode::Safe, SafeMode::Server] {
        let options = Options::builder().with_safe_mode(safe_mode).build();
        let result = parse_file(&document.path, &options)?;

        assert_denied_uri_fallback(&result, target)?;
    }
    Ok(())
}
