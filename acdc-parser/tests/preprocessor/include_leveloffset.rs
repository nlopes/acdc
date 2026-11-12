use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{BlockKind, BlockRef, InlineKind},
    parse, parse_file,
};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> std::io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "acdc-parser-include-leveloffset-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn write(&self, relative: &str, content: &str) -> std::io::Result<PathBuf> {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, content)?;
        Ok(path)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn collect_sections<'doc>(
    blocks: impl Iterator<Item = BlockRef<'doc>>,
    sections: &mut Vec<(String, u8)>,
) -> Result<(), Box<dyn Error>> {
    for block in blocks {
        if let BlockKind::Section(section) = block.kind() {
            let title = section
                .title()
                .ok_or("missing title")?
                .inlines()
                .map(|node| {
                    let InlineKind::Plain(text) = node.kind() else {
                        return Err("expected plain title");
                    };
                    Ok(text.text())
                })
                .collect::<Result<String, _>>()?;
            sections.push((title, section.level()));
            collect_sections(section.blocks(), sections)?;
        }
    }
    Ok(())
}

fn section_levels(path: &Path) -> Result<Vec<(String, u8)>, Box<dyn Error>> {
    let result = parse_file(
        path,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let mut sections = Vec::new();
    collect_sections(result.document().blocks(), &mut sections)?;
    Ok(sections)
}

#[test]
fn unsigned_include_offsets_replace_and_signed_offsets_adjust_current_state() -> TestResult {
    let directory = TempDirectory::new()?;
    directory.write("child.adoc", "== Child\n\n=== Grand")?;

    for (offset, child, grand) in [("0", 1, 2), ("1", 2, 3), ("+1", 3, 4), ("-1", 1, 2)] {
        let main = directory.write(
            "main.adoc",
            &format!(":leveloffset: 1\n\ninclude::child.adoc[leveloffset={offset}]\n\n== After"),
        )?;

        assert_eq!(
            section_levels(&main)?,
            [
                ("Child".to_string(), child),
                ("Grand".to_string(), grand),
                ("After".to_string(), 2)
            ],
            "{offset}"
        );
    }
    Ok(())
}

#[test]
fn nested_include_offsets_push_and_restore_dynamic_state() -> TestResult {
    let directory = TempDirectory::new()?;
    directory.write("inner.adoc", "== Inner\n\n=== Inner Grand")?;
    directory.write(
        "outer.adoc",
        "== Outer\n\ninclude::inner.adoc[leveloffset=+1]\n\n=== Outer Tail",
    )?;
    let main = directory.write(
        "main.adoc",
        "== Host\n\ninclude::outer.adoc[leveloffset=+1]\n\n== After",
    )?;

    assert_eq!(
        section_levels(&main)?,
        [
            ("Host".to_string(), 1),
            ("Outer".to_string(), 2),
            ("Inner".to_string(), 3),
            ("Inner Grand".to_string(), 4),
            ("Outer Tail".to_string(), 3),
            ("After".to_string(), 1),
        ]
    );
    Ok(())
}

#[test]
fn included_attribute_changes_affect_nested_content_but_not_the_parent() -> TestResult {
    let directory = TempDirectory::new()?;
    directory.write("inner.adoc", "== Inner\n\n=== Inner Grand")?;
    directory.write(
        "outer.adoc",
        "== Before Change\n\n:leveloffset: +1\n\n== After Change\n\ninclude::inner.adoc[]\n\n:leveloffset: -1\n\n== After Decrease",
    )?;
    let main = directory.write(
        "main.adoc",
        "include::outer.adoc[leveloffset=+1]\n\n== After",
    )?;

    assert_eq!(
        section_levels(&main)?,
        [
            ("Before Change".to_string(), 2),
            ("After Change".to_string(), 3),
            ("Inner".to_string(), 3),
            ("Inner Grand".to_string(), 4),
            ("After Decrease".to_string(), 2),
            ("After".to_string(), 1),
        ]
    );
    Ok(())
}

#[test]
fn filtering_precedes_include_and_document_leveloffset_changes() -> TestResult {
    let directory = TempDirectory::new()?;
    directory.write(
        "filtered.adoc",
        "== Excluded\n\n// tag::pick[]\n:leveloffset: +1\n== Picked\n// end::pick[]",
    )?;
    let main = directory.write(
        "main.adoc",
        "include::filtered.adoc[leveloffset=+1,tag=pick]\n\n== After",
    )?;

    assert_eq!(
        section_levels(&main)?,
        [("Picked".to_string(), 3), ("After".to_string(), 1)]
    );
    Ok(())
}

#[test]
fn included_leveloffset_change_leaks_without_an_explicit_include_offset() -> TestResult {
    let directory = TempDirectory::new()?;
    directory.write("child.adoc", ":leveloffset: +1\n\n== Included")?;
    let main = directory.write(
        "main.adoc",
        "== Before\n\ninclude::child.adoc[]\n\n== Following",
    )?;

    let result = parse_file(
        &main,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let mut sections = Vec::new();
    collect_sections(result.document().blocks(), &mut sections)?;

    assert_eq!(
        sections,
        [
            ("Before".to_string(), 1),
            ("Included".to_string(), 2),
            ("Following".to_string(), 2),
        ]
    );
    Ok(())
}

#[rstest::rstest]
fn authored_scope_control_cannot_override_a_caller_offset(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_compatibility_mode(mode)
        .with_attribute("leveloffset", "1")
        .build()?;
    let parsed = parse(
        ":__acdc_parser_internal_leveloffset: 0\n\n== Protected",
        &options,
    )?;
    let mut sections = Vec::new();
    collect_sections(parsed.document().blocks(), &mut sections)?;
    assert_eq!(sections, [("Protected".to_string(), 2)]);
    Ok(())
}

#[rstest::rstest]
fn literal_leveloffset_declarations_do_not_change_following_sections(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("----", "....", "++++")] delimiter: &str,
    #[values(":leveloffset: +2", ":leveloffset: +2 \\\n+2")] declaration: &str,
    #[values(false, true)] conditional: bool,
) -> TestResult {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let prefix = if conditional {
        "ifdef::missing[]\nIgnored.\nendif::[]\n"
    } else {
        ""
    };
    let expected = format!("{delimiter}\n{declaration}\n{delimiter}\n\n== Real section");
    let source = format!("{prefix}{expected}");
    let result = parse(&source, &options)?;
    let mut sections = Vec::new();
    collect_sections(result.document().blocks(), &mut sections)?;
    assert_eq!(sections, [("Real section".into(), 1)]);
    assert_eq!(result.preprocessed_source(), expected);
    assert_eq!(result.warnings(), []);
    Ok(())
}

#[test]
fn synthetic_start_and_primary_source_end_remap_independently() -> TestResult {
    let directory = TempDirectory::new()?;
    directory.write("child.adoc", "Child.")?;
    let source = "include::child.adoc[leveloffset=+1]\n\nTail.";
    let main = directory.write("main.adoc", source)?;
    let result = parse_file(
        &main,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let location = result.document().location();
    assert_eq!(location.start().line(), 1);
    assert_eq!(
        location
            .start()
            .include_chain()
            .ok_or("missing include chain")?
            .collect::<Vec<_>>(),
        ["child.adoc"]
    );
    assert_eq!(location.end().line(), 3);
    assert_eq!(location.end().column(), 5);
    assert!(location.end().include_chain().is_none());
    assert_eq!(location.absolute_end(), source.len() - 1);
    Ok(())
}

#[rstest::rstest]
fn document_boundaries_skip_generated_include_scope_controls(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(false, true)] nested: bool,
) -> TestResult {
    let directory = TempDirectory::new()?;
    directory.write("child.adoc", "Child.")?;
    directory.write("outer.adoc", "include::child.adoc[leveloffset=+1]")?;
    let target = if nested { "outer.adoc" } else { "child.adoc" };
    let main = directory.write("main.adoc", &format!("include::{target}[leveloffset=+1]"))?;
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_compatibility_mode(mode)
        .build()?;
    let result = parse_file(&main, &options)?;
    let location = result.document().location();
    let chain = if nested {
        vec!["outer.adoc", "child.adoc"]
    } else {
        vec!["child.adoc"]
    };
    assert_eq!(location.start().line(), 1);
    assert_eq!(location.start().column(), 1);
    assert_eq!(location.end().line(), 1);
    assert_eq!(location.end().column(), 6);
    assert_eq!(location.absolute_start(), 0);
    assert_eq!(location.absolute_end(), 5);
    for boundary in [location.start(), location.end()] {
        assert_eq!(
            boundary
                .include_chain()
                .ok_or("missing include chain")?
                .collect::<Vec<_>>(),
            chain
        );
    }
    Ok(())
}
