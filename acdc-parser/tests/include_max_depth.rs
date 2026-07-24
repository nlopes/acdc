use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{AttributeValue, Block, InlineNode, Options, ParseResult, parse_file};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct IncludeTree {
    directory: PathBuf,
    main: PathBuf,
    one: PathBuf,
    two: PathBuf,
}

impl IncludeTree {
    fn new(main: &str, one: &str, two: &str) -> io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "acdc-parser-max-include-depth-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&directory)?;
        let tree = Self {
            main: directory.join("main.adoc"),
            one: directory.join("one.adoc"),
            two: directory.join("two.adoc"),
            directory,
        };
        fs::write(&tree.main, main)?;
        fs::write(&tree.one, one)?;
        fs::write(&tree.two, two)?;
        Ok(tree)
    }

    /// A tree whose included files the test never reaches.
    fn main_only(main: &str) -> io::Result<Self> {
        Self::new(main, "unused", "unused")
    }

    fn chain(main_prefix: &str) -> io::Result<Self> {
        Self::new(
            &format!(
                "{main_prefix}depth={{max-include-depth}}\n\nMAIN BEFORE\n\ninclude::one.adoc[]\n\nMAIN AFTER"
            ),
            "ONE BEFORE\n\ninclude::two.adoc[]\n\nONE AFTER",
            "TWO BODY",
        )
    }
}

impl Drop for IncludeTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn options(max_depth: &str) -> Options<'_> {
    Options::builder()
        .with_attribute("max-include-depth", max_depth)
        .build()
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

/// How far the `IncludeTree::chain` fixture expanded before the limit stopped it.
#[derive(Clone, Copy)]
enum Expansion {
    /// Both levels included.
    Full,
    /// `main.adoc`'s own directive was left literal.
    BlockedAtMain,
    /// `one.adoc` was included, but its directive was left literal.
    BlockedAtOne,
}

/// Assert the paragraph sequence a `chain` fixture produces, where `depth` is the
/// visible `{max-include-depth}` value substituted into its first paragraph.
fn assert_chain(result: &ParseResult, depth: &str, expansion: Expansion) -> TestResult {
    let first = format!("depth={depth}");
    let mut expected = vec![first.as_str(), "MAIN BEFORE"];
    match expansion {
        Expansion::Full => expected.extend(["ONE BEFORE", "TWO BODY", "ONE AFTER"]),
        Expansion::BlockedAtMain => expected.push("include::one.adoc[]"),
        Expansion::BlockedAtOne => {
            expected.extend(["ONE BEFORE", "include::two.adoc[]", "ONE AFTER"]);
        }
    }
    expected.push("MAIN AFTER");

    assert_eq!(paragraph_texts(result)?, expected);
    Ok(())
}

fn assert_max_depth(result: &ParseResult, expected: &str) {
    assert_eq!(
        result
            .document()
            .attributes
            .get_string("max-include-depth")
            .as_deref(),
        Some(expected)
    );
}

fn assert_depth_warning(result: &ParseResult, max: usize, file: &Path, line: u32) -> TestResult {
    let [warning] = result.warnings() else {
        return Err(format!("expected one depth warning, got {:?}", result.warnings()).into());
    };
    assert_eq!(
        warning.kind.to_string(),
        format!("maximum include depth of {max} exceeded")
    );
    let Some(location) = warning.source_location() else {
        return Err("expected depth warning to have a source location".into());
    };
    assert_eq!(location.file.as_deref(), Some(file));
    assert_eq!(location.location.start.line, line);
    Ok(())
}

#[test]
fn default_depth_is_visible_and_allows_a_two_level_chain() -> TestResult {
    let tree = IncludeTree::chain("")?;

    let result = parse_file(&tree.main, &Options::default())?;

    assert_max_depth(&result, "64");
    assert_chain(&result, "64", Expansion::Full)?;
    assert!(result.warnings().is_empty());
    Ok(())
}

#[test]
fn default_depth_is_defined_for_conditionals_without_being_explicit() -> TestResult {
    let tree =
        IncludeTree::chain("ifdef::max-include-depth[]\nDEFAULT DEPTH IS DEFINED\nendif::[]\n\n")?;

    let result = parse_file(&tree.main, &Options::default())?;

    assert_max_depth(&result, "64");
    assert_eq!(
        paragraph_texts(&result)?,
        [
            "DEFAULT DEPTH IS DEFINED",
            "depth=64",
            "MAIN BEFORE",
            "ONE BEFORE",
            "TWO BODY",
            "ONE AFTER",
            "MAIN AFTER",
        ]
    );
    assert!(result.warnings().is_empty());
    Ok(())
}

#[test]
fn zero_disables_built_in_includes_without_a_diagnostic() -> TestResult {
    let tree = IncludeTree::chain("")?;

    let result = parse_file(&tree.main, &options("0"))?;

    assert_max_depth(&result, "0");
    assert_chain(&result, "0", Expansion::BlockedAtMain)?;
    assert!(result.warnings().is_empty());
    Ok(())
}

#[test]
fn positive_limit_preserves_the_blocked_directive_and_continues() -> TestResult {
    let tree = IncludeTree::chain("")?;

    let result = parse_file(&tree.main, &options("1"))?;

    assert_max_depth(&result, "1");
    assert_chain(&result, "1", Expansion::BlockedAtOne)?;
    assert_depth_warning(&result, 1, &tree.one, 3)?;
    Ok(())
}

#[test]
fn include_like_block_macros_are_not_include_directives() -> TestResult {
    let tree = IncludeTree::main_only("includes::x[]\n\ninclude-foo::bar[]")?;

    let result = parse_file(&tree.main, &Options::default())?;

    assert_eq!(
        paragraph_texts(&result)?,
        ["includes::x[]", "include-foo::bar[]"]
    );
    assert!(result.warnings().is_empty());
    Ok(())
}

#[test]
fn include_like_block_macros_do_not_trigger_depth_warnings() -> TestResult {
    let tree = IncludeTree::new(
        "include::one.adoc[]",
        "includes::x[]\n\ninclude-foo::bar[]\n\ninclude::two.adoc[]",
        "unused",
    )?;

    let result = parse_file(&tree.main, &options("1"))?;

    assert_eq!(
        paragraph_texts(&result)?,
        ["includes::x[]", "include-foo::bar[]", "include::two.adoc[]"]
    );
    assert_depth_warning(&result, 1, &tree.one, 5)?;
    Ok(())
}

#[test]
fn caller_limit_two_allows_a_two_level_chain() -> TestResult {
    let tree = IncludeTree::chain("")?;

    let result = parse_file(&tree.main, &options("2"))?;

    assert_max_depth(&result, "2");
    assert_chain(&result, "2", Expansion::Full)?;
    assert!(result.warnings().is_empty());
    Ok(())
}

#[test]
fn caller_string_values_use_the_leading_signed_decimal_for_the_policy() -> TestResult {
    for value in ["2junk", " 2", "18446744073709551616"] {
        let tree = IncludeTree::chain("")?;

        let result = parse_file(&tree.main, &options(value))?;

        assert_max_depth(&result, value);
        assert_chain(&result, value, Expansion::Full)?;
        assert!(result.warnings().is_empty());
    }
    Ok(())
}

#[test]
fn negative_caller_value_disables_built_in_includes() -> TestResult {
    let tree = IncludeTree::chain("")?;

    let result = parse_file(&tree.main, &options("-1"))?;

    assert_max_depth(&result, "-1");
    assert_chain(&result, "-1", Expansion::BlockedAtMain)?;
    assert!(result.warnings().is_empty());
    Ok(())
}

#[test]
fn boolean_true_disables_includes_without_crashing() -> TestResult {
    let tree = IncludeTree::chain("")?;
    let options = Options::builder()
        .with_attribute("max-include-depth", true)
        .build();

    let result = parse_file(&tree.main, &options)?;

    assert_eq!(
        result.document().attributes.get("max-include-depth"),
        Some(&AttributeValue::Bool(true))
    );
    // A boolean has no string form, so the reference substitutes to nothing.
    assert_chain(&result, "", Expansion::BlockedAtMain)?;
    assert!(result.warnings().is_empty());
    Ok(())
}

#[test]
fn boolean_false_and_no_value_restore_the_default() -> TestResult {
    let options = [
        Options::builder()
            .with_attribute("max-include-depth", false)
            .build(),
        Options::builder()
            .with_attribute("max-include-depth", ())
            .build(),
    ];

    for options in options {
        let tree = IncludeTree::chain("")?;

        let result = parse_file(&tree.main, &options)?;

        assert_max_depth(&result, "64");
        assert_chain(&result, "64", Expansion::Full)?;
        assert!(result.warnings().is_empty());
    }
    Ok(())
}

#[test]
fn document_header_cannot_change_or_unset_the_trusted_limit() -> TestResult {
    let tree = IncludeTree::chain(":max-include-depth: 2\n:max-include-depth!:\n\n")?;

    let result = parse_file(&tree.main, &options("1"))?;

    assert_max_depth(&result, "1");
    assert_chain(&result, "1", Expansion::BlockedAtOne)?;
    assert_depth_warning(&result, 1, &tree.one, 3)?;
    Ok(())
}

#[test]
fn ignored_body_depth_declarations_are_absent_from_the_ast() -> TestResult {
    let tree = IncludeTree::main_only(
        "BEFORE\n\n:max-include-depth: 5\n\n== Section\n\n:max-include-depth: 6\n:max-include-depth!:\n\nAFTER {max-include-depth}",
    )?;

    let result = parse_file(&tree.main, &options("1"))?;

    assert_max_depth(&result, "1");
    let [Block::Paragraph(before), Block::Section(section)] = result.document().blocks.as_slice()
    else {
        return Err(format!("unexpected document blocks: {:?}", result.document().blocks).into());
    };
    let [InlineNode::PlainText(before)] = before.content.as_slice() else {
        return Err(format!("unexpected paragraph content: {before:?}").into());
    };
    assert_eq!(before.content, "BEFORE");

    let [Block::Paragraph(after)] = section.content.as_slice() else {
        return Err(format!("unexpected section content: {:?}", section.content).into());
    };
    let [InlineNode::PlainText(after)] = after.content.as_slice() else {
        return Err(format!("unexpected paragraph content: {after:?}").into());
    };
    assert_eq!(after.content, "AFTER 1");
    assert!(result.warnings().is_empty());
    Ok(())
}

#[test]
fn block_metadata_cannot_change_the_trusted_limit() -> TestResult {
    for metadata in [
        ".Block title\n",
        "[[depth-anchor]]\n",
        "[role=\"depth-role\"]\n",
    ] {
        let tree = IncludeTree::chain(&format!("{metadata}:max-include-depth: 2\n"))?;

        let result = parse_file(&tree.main, &options("1"))?;

        assert_max_depth(&result, "1");
        assert_chain(&result, "1", Expansion::BlockedAtOne)?;
        assert_depth_warning(&result, 1, &tree.one, 3)?;
    }
    Ok(())
}

#[test]
fn document_header_cannot_override_or_unset_the_default() -> TestResult {
    let tree = IncludeTree::chain(":max-include-depth: 0\n:max-include-depth!:\n\n")?;

    let result = parse_file(&tree.main, &Options::default())?;

    assert_max_depth(&result, "64");
    assert_chain(&result, "64", Expansion::Full)?;
    assert!(result.warnings().is_empty());
    Ok(())
}

#[test]
fn depth_limit_bounds_a_self_include_cycle() -> TestResult {
    let tree = IncludeTree::main_only("SELF BODY\n\ninclude::main.adoc[]")?;

    let result = parse_file(&tree.main, &options("2"))?;

    assert_eq!(
        paragraph_texts(&result)?,
        [
            "SELF BODY",
            "SELF BODY",
            "SELF BODY",
            "include::main.adoc[]",
        ]
    );
    assert_depth_warning(&result, 2, &tree.main, 3)?;
    Ok(())
}

#[test]
fn default_limit_bounds_a_self_include_cycle() -> TestResult {
    let tree = IncludeTree::main_only("SELF BODY\n\ninclude::main.adoc[]")?;

    let result = parse_file(&tree.main, &Options::default())?;

    let paragraphs = paragraph_texts(&result)?;
    let Some((last, bodies)) = paragraphs.split_last() else {
        return Err("expected self-include output".into());
    };
    assert_eq!(*last, "include::main.adoc[]");
    assert_eq!(bodies.len(), 65);
    assert!(bodies.iter().all(|body| *body == "SELF BODY"));
    assert_depth_warning(&result, 64, &tree.main, 3)?;
    Ok(())
}
