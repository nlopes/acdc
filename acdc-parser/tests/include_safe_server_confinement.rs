use std::{
    error::Error,
    fs, io,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{Block, InlineNode, Options, ParseResult, SafeMode, parse_file};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const ANCESTOR_RECOVERY_WARNING: &str =
    "include file has illegal reference to ancestor of jail; recovering automatically";
const OUTSIDE_RECOVERY_WARNING: &str = "include file is outside of jail; recovering automatically";

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new() -> io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "acdc-parser-safe-server-confinement-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct FileTree {
    _temp: TempDirectory,
    root: PathBuf,
    entry_dir: PathBuf,
    main: PathBuf,
}

impl FileTree {
    fn new(source: &str) -> io::Result<Self> {
        let temp = TempDirectory::new()?;
        let root = temp.path.clone();
        let entry_dir = root.join("entry");
        fs::create_dir(&entry_dir)?;
        let main = entry_dir.join("main.adoc");
        fs::write(&main, source)?;
        Ok(Self {
            _temp: temp,
            root,
            entry_dir,
            main,
        })
    }

    fn write(path: &Path, content: &str) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)
    }
}

fn options(safe_mode: SafeMode) -> Options<'static> {
    Options::builder().with_safe_mode(safe_mode).build()
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

fn assert_single_unlocated_warning(
    result: &ParseResult,
    expected: &str,
) -> Result<(), Box<dyn Error>> {
    let [warning] = result.warnings() else {
        return Err(format!("expected one warning, got {:?}", result.warnings()).into());
    };
    assert_eq!(warning.kind.to_string(), expected);
    assert!(warning.source_location().is_none());
    Ok(())
}

fn recovered_absolute_path(entry_dir: &Path, target: &Path) -> PathBuf {
    let mut recovered = entry_dir.to_path_buf();
    for component in target.components() {
        if let Component::Normal(segment) = component {
            recovered.push(segment);
        }
    }
    recovered
}

#[test]
fn ancestor_traversal_is_moved_inside_the_entry_directory() -> TestResult {
    let tree = FileTree::new("include::../outside.adoc[]")?;
    FileTree::write(&tree.root.join("outside.adoc"), "REAL OUTSIDE")?;
    FileTree::write(
        &tree.entry_dir.join("outside.adoc"),
        "REBASED ENTRY OUTSIDE",
    )?;

    let unsafe_result = parse_file(&tree.main, &options(SafeMode::Unsafe))?;
    assert_eq!(paragraph_texts(&unsafe_result)?, ["REAL OUTSIDE"]);
    assert!(unsafe_result.warnings().is_empty());

    for safe_mode in [SafeMode::Safe, SafeMode::Server] {
        let result = parse_file(&tree.main, &options(safe_mode))?;
        assert_eq!(paragraph_texts(&result)?, ["REBASED ENTRY OUTSIDE"]);
        assert_single_unlocated_warning(&result, ANCESTOR_RECOVERY_WARNING)?;
    }

    Ok(())
}

#[test]
fn absolute_outside_targets_are_moved_inside_the_entry_directory() -> TestResult {
    let tree = FileTree::new("")?;
    let outside = tree.root.join("absolute-outside.adoc");
    let inside = tree.entry_dir.join("absolute-inside.adoc");
    let recovered = recovered_absolute_path(&tree.entry_dir, &outside);
    FileTree::write(&outside, "REAL ABSOLUTE OUTSIDE")?;
    FileTree::write(&inside, "ABSOLUTE INSIDE")?;
    FileTree::write(&recovered, "REBASED ABSOLUTE OUTSIDE")?;
    fs::write(
        &tree.main,
        format!(
            "include::{}[]\n\ninclude::{}[]",
            outside.display(),
            inside.display()
        ),
    )?;

    for safe_mode in [SafeMode::Safe, SafeMode::Server] {
        let result = parse_file(&tree.main, &options(safe_mode))?;
        assert_eq!(
            paragraph_texts(&result)?,
            ["REBASED ABSOLUTE OUTSIDE", "ABSOLUTE INSIDE"]
        );
        assert_single_unlocated_warning(&result, OUTSIDE_RECOVERY_WARNING)?;
    }

    Ok(())
}

#[test]
fn nested_includes_keep_the_entry_directory_boundary() -> TestResult {
    let tree = FileTree::new("include::sub/inner.adoc[]")?;
    FileTree::write(&tree.root.join("outside.adoc"), "REAL OUTSIDE")?;
    FileTree::write(&tree.entry_dir.join("outside.adoc"), "ENTRY OUTSIDE")?;
    FileTree::write(
        &tree.entry_dir.join("sub/inner.adoc"),
        "INNER START\n\ninclude::../../outside.adoc[]\n\nINNER END",
    )?;

    for safe_mode in [SafeMode::Safe, SafeMode::Server] {
        let result = parse_file(&tree.main, &options(safe_mode))?;
        assert_eq!(
            paragraph_texts(&result)?,
            ["INNER START", "ENTRY OUTSIDE", "INNER END"]
        );
        assert_single_unlocated_warning(&result, ANCESTOR_RECOVERY_WARNING)?;
    }

    Ok(())
}

#[test]
fn optional_missing_recovered_target_keeps_only_the_recovery_warning() -> TestResult {
    let tree = FileTree::new("include::../outside.adoc[opts=optional]")?;
    FileTree::write(&tree.root.join("outside.adoc"), "REAL OUTSIDE")?;

    for safe_mode in [SafeMode::Safe, SafeMode::Server] {
        let result = parse_file(&tree.main, &options(safe_mode))?;
        assert!(result.document().blocks.is_empty());
        assert_single_unlocated_warning(&result, ANCESTOR_RECOVERY_WARNING)?;
    }

    Ok(())
}

#[cfg(unix)]
#[test]
fn in_boundary_symlinks_can_point_to_outside_files() -> TestResult {
    use std::os::unix::fs::symlink;

    let tree = FileTree::new("include::linked.adoc[]")?;
    let outside = tree.root.join("outside.adoc");
    FileTree::write(&outside, "SYMLINK OUTSIDE")?;
    symlink(&outside, tree.entry_dir.join("linked.adoc"))?;

    for safe_mode in [SafeMode::Unsafe, SafeMode::Safe, SafeMode::Server] {
        let result = parse_file(&tree.main, &options(safe_mode))?;
        assert_eq!(paragraph_texts(&result)?, ["SYMLINK OUTSIDE"]);
        assert!(result.warnings().is_empty());
    }

    Ok(())
}
