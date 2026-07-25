use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{Block, InlineNode, Options, ParseResult, parse_file};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct IncludeTree {
    directory: PathBuf,
    main: PathBuf,
    nested: Option<PathBuf>,
}

impl IncludeTree {
    fn main_only(source: &str) -> io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "acdc-parser-missing-include-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&directory)?;
        let main = directory.join("main.adoc");
        fs::write(&main, source)?;
        Ok(Self {
            directory,
            main,
            nested: None,
        })
    }

    fn nested(main_source: &str, nested_source: &str) -> io::Result<Self> {
        let mut tree = Self::main_only(main_source)?;
        let chapters = tree.directory.join("chapters");
        fs::create_dir(&chapters)?;
        let nested = chapters.join("outer.adoc");
        fs::write(&nested, nested_source)?;
        tree.nested = Some(nested);
        Ok(tree)
    }
}

impl Drop for IncludeTree {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Ok(entries) = fs::read_dir(&self.directory) {
            use std::os::unix::fs::PermissionsExt;

            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|file_type| file_type.is_file()) {
                    let _ = fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o600));
                }
            }
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn paragraph_texts(result: &ParseResult) -> Result<Vec<&str>, Box<dyn Error>> {
    result
        .document()
        .blocks
        .iter()
        .map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return Err(format!("expected paragraph, got {block:?}").into());
            };
            let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
                return Err(format!("expected plain paragraph text, got {paragraph:?}").into());
            };
            Ok(text.content)
        })
        .collect()
}

fn assert_missing_warning(
    result: &ParseResult,
    message: &str,
    source: &Path,
    line: u32,
) -> TestResult {
    let [warning] = result.warnings() else {
        return Err(format!("expected one warning, got {:?}", result.warnings()).into());
    };
    assert_eq!(warning.kind.to_string(), message);
    let Some(location) = warning.source_location() else {
        return Err("expected the warning to have a source location".into());
    };
    assert_eq!(location.file.as_deref(), Some(source));
    assert_eq!(location.location.start.line, line);
    Ok(())
}

#[test]
fn missing_top_level_include_inserts_fallback_and_continues() -> TestResult {
    let tree = IncludeTree::main_only(
        "= Document\n:part: expanded\n\nBEFORE\n\ninclude::missing-{part}.adoc[lines=1..2]\n\nAFTER",
    )?;

    let result = parse_file(&tree.main, &Options::default())?;

    assert_eq!(
        paragraph_texts(&result)?,
        [
            "BEFORE",
            "Unresolved directive in main.adoc - include::missing-expanded.adoc[lines=1..2]",
            "AFTER",
        ]
    );
    let missing = tree.directory.join("missing-expanded.adoc");
    assert_missing_warning(
        &result,
        &format!("include file not found: {}", missing.display()),
        &tree.main,
        6,
    )?;
    let Some(Block::Paragraph(fallback)) = result.document().blocks.get(1) else {
        return Err("expected the fallback paragraph".into());
    };
    assert_eq!(fallback.location.start.line, 6);
    assert!(fallback.location.start.file.is_none());
    Ok(())
}

#[test]
fn missing_nested_include_is_attributed_to_its_including_source() -> TestResult {
    let tree = IncludeTree::nested(
        "MAIN BEFORE\n\ninclude::chapters/outer.adoc[]\n\nMAIN AFTER",
        "OUTER BEFORE\n\ninclude::missing.adoc[tag=x]\n\nOUTER AFTER",
    )?;
    let nested = tree
        .nested
        .as_deref()
        .ok_or("expected the nested fixture path")?;

    let result = parse_file(&tree.main, &Options::default())?;

    assert_eq!(
        paragraph_texts(&result)?,
        [
            "MAIN BEFORE",
            "OUTER BEFORE",
            "Unresolved directive in chapters/outer.adoc - include::missing.adoc[tag=x]",
            "OUTER AFTER",
            "MAIN AFTER",
        ]
    );
    let missing = tree.directory.join("chapters/missing.adoc");
    assert_missing_warning(
        &result,
        &format!("include file not found: {}", missing.display()),
        nested,
        3,
    )?;
    let Some(Block::Paragraph(fallback)) = result.document().blocks.get(2) else {
        return Err("expected the fallback paragraph".into());
    };
    assert_eq!(fallback.location.start.line, 3);
    assert_eq!(
        fallback
            .location
            .start
            .file
            .as_ref()
            .map(|chain| chain.as_slice()),
        Some(["chapters/outer.adoc".to_string()].as_slice())
    );
    Ok(())
}

#[test]
fn optional_missing_include_remains_silent_and_is_removed() -> TestResult {
    let tree = IncludeTree::main_only("BEFORE\n\ninclude::missing.adoc[opts=optional]\n\nAFTER")?;

    let result = parse_file(&tree.main, &Options::default())?;

    assert_eq!(paragraph_texts(&result)?, ["BEFORE", "AFTER"]);
    assert!(result.warnings().is_empty());
    Ok(())
}

#[cfg(unix)]
#[test]
fn unreadable_filtered_include_inserts_fallback_and_continues() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let tree = IncludeTree::main_only("BEFORE\n\ninclude::unreadable.adoc[lines=1]\n\nAFTER")?;
    let unreadable = tree.directory.join("unreadable.adoc");
    fs::write(&unreadable, "SECRET")?;
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))?;

    let result = parse_file(&tree.main, &Options::default())?;

    assert_eq!(
        paragraph_texts(&result)?,
        [
            "BEFORE",
            "Unresolved directive in main.adoc - include::unreadable.adoc[lines=1]",
            "AFTER",
        ]
    );
    assert_missing_warning(
        &result,
        &format!("include file not readable: {}", unreadable.display()),
        &tree.main,
        3,
    )?;
    Ok(())
}
