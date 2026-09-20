use std::{
    error::Error,
    fs,
    io::Cursor,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{Block, InlineNode, Options, ParseResult, parse, parse_file, parse_from_reader};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> std::io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "acdc-parser-front-matter-{}-{sequence}",
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

fn paragraph_texts(result: &ParseResult) -> Result<Vec<&str>, Box<dyn Error>> {
    result
        .document()
        .blocks
        .iter()
        .map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return Err(format!("unexpected block: {block:?}").into());
            };
            let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
                return Err(format!("unexpected paragraph: {paragraph:?}").into());
            };
            Ok(text.content)
        })
        .collect()
}

fn options() -> Result<Options<'static>, acdc_parser::Error> {
    Options::builder()
        .with_attribute("skip-front-matter", true)
        .build()
}

#[test]
fn caller_option_captures_front_matter_before_parsing() -> TestResult {
    let source = "---\nlayout: default\ntags: [one, two]\n---\n= Document Title\n\nCaptured: {front-matter}\n";
    let parsed = parse(source, &options()?)?;

    assert_eq!(
        parsed
            .document()
            .attributes
            .get("front-matter")
            .and_then(|value| value.text()),
        Some("layout: default\ntags: [one, two]")
    );
    assert_eq!(
        parsed.source(),
        "= Document Title\n\nCaptured: {front-matter}"
    );
    assert_eq!(
        paragraph_texts(&parsed)?,
        ["Captured: layout: default\ntags: [one, two]"]
    );
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected one paragraph".into());
    };
    assert_eq!(paragraph.location.start.line, 7);
    Ok(())
}

#[test]
fn captured_front_matter_overrides_a_caller_value() -> TestResult {
    let options = Options::builder()
        .with_attribute("skip-front-matter", true)
        .with_attribute("front-matter", "caller")
        .build()?;
    let parsed = parse("---\ncaptured\n---\n{front-matter}\n", &options)?;

    assert_eq!(
        parsed
            .document()
            .attributes
            .get("front-matter")
            .and_then(|value| value.text()),
        Some("captured")
    );
    assert_eq!(paragraph_texts(&parsed)?, ["captured"]);
    Ok(())
}

#[test]
fn captured_front_matter_is_available_to_conditionals_and_includes() -> TestResult {
    let directory = TempDirectory::new()?;
    let main = directory.0.join("main.adoc");
    fs::write(
        &main,
        "---\nroot\n---\n= Title\n\nifdef::front-matter[]\nRoot: {front-matter}\n\ninclude::child.adoc[]\nendif::[]\n",
    )?;
    fs::write(
        directory.0.join("child.adoc"),
        "---\nchild\n---\nifdef::front-matter[]\nChild: {front-matter}\nendif::[]\n",
    )?;
    let parsed = parse_file(main, &options()?)?;
    assert_eq!(paragraph_texts(&parsed)?, ["Root: root", "Child: root"]);
    assert_eq!(
        parsed
            .document()
            .attributes
            .get("front-matter")
            .and_then(|value| value.text()),
        Some("root")
    );
    Ok(())
}

#[test]
fn source_cannot_enable_front_matter_processing() -> TestResult {
    let source = ":skip-front-matter:\n---\nvalue\n---\n";
    let parsed = parse(source, &Options::default())?;

    assert_eq!(parsed.source(), source.trim_end());
    assert!(
        !parsed
            .document()
            .attributes
            .contains_key("skip-front-matter")
    );
    assert!(!parsed.document().attributes.contains_key("front-matter"));
    Ok(())
}

#[test]
fn unterminated_front_matter_is_left_unchanged() -> TestResult {
    let source = "---\nvalue\n= Title\n";
    let parsed = parse(source, &options()?)?;

    assert_eq!(parsed.source(), source.trim_end());
    assert!(!parsed.document().attributes.contains_key("front-matter"));
    Ok(())
}

#[test]
fn string_reader_and_file_inputs_share_front_matter_behavior() -> TestResult {
    let directory = TempDirectory::new()?;
    let file = directory.0.join("document.adoc");
    let source = "---\ninput: shared\n---\n{front-matter}\n";
    fs::write(&file, source)?;

    let parsed_string = parse(source, &options()?)?;
    let parsed_reader = parse_from_reader(Cursor::new(source), &options()?)?;
    let parsed_file = parse_file(file, &options()?)?;

    for parsed in [&parsed_string, &parsed_reader, &parsed_file] {
        assert_eq!(
            parsed
                .document()
                .attributes
                .get("front-matter")
                .and_then(|value| value.text()),
            Some("input: shared")
        );
        assert_eq!(paragraph_texts(parsed)?, ["input: shared"]);
    }
    Ok(())
}

#[test]
fn includes_drop_front_matter_without_replacing_the_primary_value() -> TestResult {
    let directory = TempDirectory::new()?;
    let main = directory.0.join("main.adoc");
    let child = directory.0.join("child.adoc");
    fs::write(
        &main,
        "---\nprimary: yes\n---\n= Main\n\nBefore: {front-matter}\n\ninclude::child.adoc[]\n\nAfter: {front-matter}\n",
    )?;
    fs::write(&child, "---\nchild: yes\n---\nChild: {front-matter}\n")?;

    let parsed = parse_file(main, &options()?)?;

    assert_eq!(
        parsed
            .document()
            .attributes
            .get("front-matter")
            .and_then(|value| value.text()),
        Some("primary: yes")
    );
    assert_eq!(
        paragraph_texts(&parsed)?,
        [
            "Before: primary: yes",
            "Child: primary: yes",
            "After: primary: yes"
        ]
    );
    let Some(Block::Paragraph(child_paragraph)) = parsed.document().blocks.get(1) else {
        return Err("expected included paragraph".into());
    };
    assert_eq!(child_paragraph.location.start.line, 4);
    assert_eq!(
        child_paragraph
            .location
            .start
            .file
            .as_ref()
            .map(|chain| chain.as_slice()),
        Some(["child.adoc".to_string()].as_slice())
    );
    Ok(())
}

#[test]
fn include_front_matter_is_removed_before_indent_is_applied() -> TestResult {
    let directory = TempDirectory::new()?;
    let main = directory.0.join("main.adoc");
    fs::write(&main, "= Main\n\ninclude::child.adoc[indent=2]\n")?;
    fs::write(
        directory.0.join("child.adoc"),
        "---\nchild: yes\n---\nChild content\n",
    )?;

    let parsed = parse_file(main, &options()?)?;

    assert_eq!(parsed.source(), "= Main\n\n  Child content");
    assert!(!parsed.document().attributes.contains_key("front-matter"));
    Ok(())
}
