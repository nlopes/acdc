use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{Block, InlineMacro, InlineNode, Options, parse_file};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDocument {
    directory: PathBuf,
    main: PathBuf,
}

impl TempDocument {
    fn new(source: &str) -> io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "acdc-parser-uri-classification-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&directory)?;
        let main = directory.join("main.adoc");
        fs::write(&main, source)?;
        Ok(Self { directory, main })
    }

    fn write(&self, relative: impl AsRef<Path>, content: &str) -> io::Result<()> {
        let path = self.directory.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)
    }
}

impl Drop for TempDocument {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn plain_text(paragraph: &acdc_parser::Paragraph<'_>) -> Result<String, Box<dyn Error>> {
    paragraph
        .content
        .iter()
        .map(|node| {
            let InlineNode::PlainText(text) = node else {
                return Err(format!("unexpected inline node: {node:?}").into());
            };
            Ok(text.content)
        })
        .collect()
}

#[test]
fn denied_non_http_uri_uses_link_fallback_instead_of_local_file_handling() -> TestResult {
    let target = "ftp://example.test/part.adoc";
    let document = TempDocument::new(&format!("BEFORE\n\ninclude::{target}[]\n\nAFTER"))?;
    document.write("ftp:/example.test/part.adoc", "LOCAL FILE MUST NOT BE READ")?;

    for compat_mode in [false, true] {
        let options = if compat_mode {
            Options::builder()
                .with_attribute("compat-mode", true)
                .build()
        } else {
            Options::default()
        };
        let result = parse_file(&document.main, &options)?;

        let [
            Block::Paragraph(before),
            Block::Paragraph(fallback),
            Block::Paragraph(after),
        ] = result.document().blocks.as_slice()
        else {
            return Err(format!("unexpected blocks: {:?}", result.document().blocks).into());
        };
        assert_eq!(plain_text(before)?, "BEFORE");
        assert_eq!(plain_text(after)?, "AFTER");

        let [InlineNode::Macro(InlineMacro::Link(link))] = fallback.content.as_slice() else {
            return Err(format!("unexpected fallback: {fallback:?}").into());
        };
        assert_eq!(link.target.to_string(), target);
        assert_eq!(
            link.attributes.get_string("role").as_deref(),
            (!compat_mode).then_some("include")
        );
        assert!(result.warnings().is_empty());
    }
    Ok(())
}

#[test]
fn authorized_unsupported_uri_recovers_without_reading_a_local_file() -> TestResult {
    let target = "ftp://example.test/part.adoc";
    let document = TempDocument::new(&format!("BEFORE\n\ninclude::{target}[]\n\nAFTER"))?;
    document.write("ftp:/example.test/part.adoc", "LOCAL FILE MUST NOT BE READ")?;
    let options = Options::builder()
        .with_attribute("allow-uri-read", true)
        .build();

    let result = parse_file(&document.main, &options)?;

    let [
        Block::Paragraph(before),
        Block::Paragraph(fallback),
        Block::Paragraph(after),
    ] = result.document().blocks.as_slice()
    else {
        return Err(format!("unexpected blocks: {:?}", result.document().blocks).into());
    };
    assert_eq!(plain_text(before)?, "BEFORE");
    assert_eq!(plain_text(after)?, "AFTER");
    let text = plain_text(fallback)?;
    assert!(text.contains(&format!("include::{target}[]")));
    assert!(!text.contains("LOCAL FILE"));

    let [warning] = result.warnings() else {
        return Err(format!("unexpected warnings: {:?}", result.warnings()).into());
    };
    assert_eq!(
        warning.kind.to_string(),
        format!("include uri not readable: {target}")
    );
    let Some(location) = warning.source_location() else {
        return Err("expected a located warning".into());
    };
    assert_eq!(location.file.as_deref(), Some(document.main.as_path()));
    assert_eq!(location.location.start.line, 3);
    Ok(())
}
