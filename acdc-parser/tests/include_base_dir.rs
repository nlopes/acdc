use std::{
    error::Error,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{Block, InlineNode, Options, SafeMode, parse, parse_file, parse_from_reader};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const ANCESTOR_RECOVERY_WARNING: &str =
    "include file has illegal reference to ancestor of jail; recovering automatically";

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> std::io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "acdc-parser-include-base-dir-{}-{sequence}",
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

struct CurrentDirectoryFile {
    path: PathBuf,
    name: String,
}

impl CurrentDirectoryFile {
    fn new(content: &str) -> std::io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            "acdc-parser-include-base-dir-{}-{sequence}.adoc",
            std::process::id()
        );
        let path = std::env::current_dir()?.join(&name);
        fs::write(&path, content)?;
        Ok(Self { path, name })
    }
}

impl Drop for CurrentDirectoryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn write(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)
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

#[test]
fn string_and_reader_input_default_to_current_directory() -> TestResult {
    let target = CurrentDirectoryFile::new("CURRENT DIRECTORY")?;
    let input = format!("include::{}[]", target.name);

    let string_result = parse(&input, &Options::default())?;
    let reader_result = parse_from_reader(Cursor::new(input), &Options::default())?;

    assert_eq!(paragraph_text(&string_result)?, "CURRENT DIRECTORY");
    assert_eq!(paragraph_text(&reader_result)?, "CURRENT DIRECTORY");
    Ok(())
}

#[test]
fn string_and_reader_input_resolve_includes_against_base_dir() -> TestResult {
    let directory = TempDirectory::new()?;
    write(&directory.0.join("part.adoc"), "INCLUDED")?;
    let options = Options::builder().with_base_dir(&directory.0).build();

    let string_result = parse("include::part.adoc[]", &options)?;
    let reader_result = parse_from_reader(Cursor::new("include::part.adoc[]"), &options)?;

    assert_eq!(paragraph_text(&string_result)?, "INCLUDED");
    assert_eq!(paragraph_text(&reader_result)?, "INCLUDED");
    assert!(string_result.warnings().is_empty());
    assert!(reader_result.warnings().is_empty());
    Ok(())
}

#[test]
fn file_input_base_override_controls_entry_resolution() -> TestResult {
    let directory = TempDirectory::new()?;
    let entry_dir = directory.0.join("entry");
    let base_dir = directory.0.join("base");
    let main = entry_dir.join("main.adoc");
    write(&main, "include::chapter.adoc[]")?;
    write(
        &base_dir.join("chapter.adoc"),
        "include::nested/content.adoc[]",
    )?;
    write(&base_dir.join("nested/content.adoc"), "NESTED")?;

    for safe_mode in [SafeMode::Unsafe, SafeMode::Safe, SafeMode::Server] {
        let options = Options::builder()
            .with_safe_mode(safe_mode)
            .with_base_dir(&base_dir)
            .build();
        let result = parse_file(&main, &options)?;

        assert_eq!(paragraph_text(&result)?, "NESTED", "{safe_mode:?}");
        assert!(result.warnings().is_empty(), "{safe_mode:?}");
    }
    Ok(())
}

#[test]
fn safe_and_server_confinement_use_overridden_base() -> TestResult {
    let directory = TempDirectory::new()?;
    let main = directory.0.join("entry/main.adoc");
    let base_dir = directory.0.join("base");
    write(&main, "include::../outside.adoc[]")?;
    write(&directory.0.join("outside.adoc"), "REAL OUTSIDE")?;
    write(&base_dir.join("outside.adoc"), "REBASED BASE OUTSIDE")?;

    let unsafe_options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_base_dir(&base_dir)
        .build();
    let unsafe_result = parse_file(&main, &unsafe_options)?;
    assert_eq!(paragraph_text(&unsafe_result)?, "REAL OUTSIDE");
    assert!(unsafe_result.warnings().is_empty());

    for safe_mode in [SafeMode::Safe, SafeMode::Server] {
        let options = Options::builder()
            .with_safe_mode(safe_mode)
            .with_base_dir(&base_dir)
            .build();
        let result = parse_file(&main, &options)?;

        assert_eq!(
            paragraph_text(&result)?,
            "REBASED BASE OUTSIDE",
            "{safe_mode:?}"
        );
        let [warning] = result.warnings() else {
            return Err(format!("unexpected warnings: {:?}", result.warnings()).into());
        };
        assert_eq!(warning.kind.to_string(), ANCESTOR_RECOVERY_WARNING);
        assert!(warning.source_location().is_none());
    }
    Ok(())
}

#[test]
fn missing_include_recovery_uses_the_entry_basename_outside_the_base() -> TestResult {
    let directory = TempDirectory::new()?;
    let main = directory.0.join("entry/main.adoc");
    let base_dir = directory.0.join("base");
    write(&main, "include::missing.adoc[]")?;
    fs::create_dir(&base_dir)?;

    let options = Options::builder().with_base_dir(&base_dir).build();
    let result = parse_file(&main, &options)?;

    assert_eq!(
        paragraph_text(&result)?,
        "Unresolved directive in main.adoc - include::missing.adoc[]"
    );
    let [warning] = result.warnings() else {
        return Err(format!("unexpected warnings: {:?}", result.warnings()).into());
    };
    let Some(location) = warning.source_location() else {
        return Err("expected a located warning".into());
    };
    assert_eq!(location.file.as_deref(), Some(main.as_path()));
    Ok(())
}

#[test]
fn nested_includes_are_relative_to_the_containing_file_after_base_override() -> TestResult {
    let directory = TempDirectory::new()?;
    let main = directory.0.join("entry/main.adoc");
    let base_dir = directory.0.join("base");
    write(&main, "include::chapters/one.adoc[]")?;
    write(&base_dir.join("chapters/one.adoc"), "include::two.adoc[]")?;
    write(&base_dir.join("chapters/two.adoc"), "RELATIVE")?;

    let options = Options::builder().with_base_dir(&base_dir).build();
    let result = parse_file(&main, &options)?;

    assert_eq!(paragraph_text(&result)?, "RELATIVE");
    Ok(())
}
