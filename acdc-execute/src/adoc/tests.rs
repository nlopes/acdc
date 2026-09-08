//! Unit tests for discovery: building a [`CommandGraph`] from a parsed `AsciiDoc` document.

use acdc_parser::{Options, ParseResult, SafeMode};
use rstest::rstest;

use super::Error;
use crate::command::{BuildError, CommandBlock, CommandGraph};

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

fn parse(src: &str) -> ParseResult {
    acdc_parser::parse(src, &Options::default()).unwrap_or_else(|e| panic!("parse failed: {e}"))
}

fn try_graph(src: &str) -> Result<CommandGraph, Error> {
    CommandGraph::try_from(parse(src).document())
}

fn graph(src: &str) -> CommandGraph {
    try_graph(src).unwrap_or_else(|e| panic!("graph should build: {e}"))
}

fn err(src: &str) -> Error {
    try_graph(src).expect_err("graph build should fail")
}

/// Command ids in execution (topological) order.
fn ids(built: CommandGraph) -> Vec<String> {
    built
        .into_iter()
        .map(|b| b.metadata.id.as_str().to_string())
        .collect()
}

/// The single command block with `id`, failing the test if absent.
fn find(built: CommandGraph, id: &str) -> CommandBlock {
    built
        .into_iter()
        .find(|b| b.metadata.id.as_str() == id)
        .unwrap_or_else(|| panic!("command {id:?} not found"))
}

/// Index of `name` within an ordered id list.
fn pos(ids: &[String], name: &str) -> usize {
    ids.iter()
        .position(|s| s == name)
        .unwrap_or_else(|| panic!("{name:?} missing from {ids:?}"))
}

/// Render a single command block. `deps` and `lang` are optional; `body` is the script.
fn cmd(id: &str, deps: Option<&str>, lang: Option<&str>, body: &str) -> String {
    let deps = deps.map(|d| format!(", deps=\"{d}\"")).unwrap_or_default();
    let lang = lang.map(|l| format!("[source, {l}]\n")).unwrap_or_default();
    format!("[.command, id={id}{deps}]\n{lang}----\n{body}\n----\n")
}

/// Wrap `body` in a level-1 section titled `name`.
fn section(name: &str, body: &str) -> String {
    format!("== {name}\n\n{body}")
}

// --------------------------------------------------------------------------
// Discovery: where command blocks are found
// --------------------------------------------------------------------------

#[test]
fn empty_document_yields_empty_graph() {
    assert_eq!(ids(graph("= Doc\n")), Vec::<String>::new());
}

#[test]
fn document_with_no_commands_yields_empty_graph() {
    let src = "= Doc\n\nSome prose.\n\n[source, bash]\n----\necho not-a-command\n----\n";
    assert_eq!(ids(graph(src)), Vec::<String>::new());
}

// upstream acdc-parser bug: block attributes inside open blocks (`--`) are parsed as
// literal paragraph text rather than applied to the following block, due to a delimiter
// conflict between `--` and `----`. Commands inside open blocks cannot be discovered;
// the `DelimitedOpen` arm in `nested_blocks` is kept for traversal only.

#[rstest]
#[case::top_level(cmd("build", None, None, "echo hi"))]
#[case::nested_in_section(format!(
    "= Doc\n\n{}",
    section("Build", &cmd("build", None, None, "echo hi"))
))]
#[case::nested_subsection(format!(
    "= Doc\n\n== Top\n\n{}",
    section("Build", &cmd("build", None, None, "echo hi"))
))]
#[case::example_block(format!("= Doc\n\n====\n{}====\n", cmd("build", None, None, "echo hi")))]
#[case::sidebar_block(format!("= Doc\n\n****\n{}****\n", cmd("build", None, None, "echo hi")))]
#[case::quote_block(format!("= Doc\n\n____\n{}____\n", cmd("build", None, None, "echo hi")))]
#[case::admonition_block(format!(
    "= Doc\n\n[NOTE]\n====\n{}====\n",
    cmd("build", None, None, "echo hi")
))]
#[case::ordered_list_item(format!("= Doc\n\n. Build it\n+\n{}", cmd("build", None, None, "echo hi")))]
#[case::unordered_list_item(format!("= Doc\n\n* Build it\n+\n{}", cmd("build", None, None, "echo hi")))]
#[case::description_list_item(format!(
    "= Doc\n\nBuild:: Compile the project\n+\n{}",
    cmd("build", None, None, "echo hi")
))]
fn single_command_is_discovered(#[case] src: String) {
    assert_eq!(ids(graph(&src)), ["build"]);
}

#[test]
fn commands_across_multiple_sections_are_all_found() {
    let src = format!(
        "= Doc\n\n{}\n{}",
        section("Build", &cmd("build", None, None, "echo build")),
        section("Test", &cmd("test", Some("build"), None, "echo test")),
    );
    let ids = ids(graph(&src));
    assert_eq!(ids.len(), 2);
    assert!(pos(&ids, "build") < pos(&ids, "test"));
}

#[test]
fn commands_across_distinct_list_items_are_all_found() {
    let src = format!(
        "= Doc\n\n. First\n+\n{}\n. Second\n+\n{}",
        cmd("build", None, None, "echo build"),
        cmd("test", Some("build"), None, "echo test"),
    );
    let ids = ids(graph(&src));
    assert_eq!(ids.len(), 2);
    assert!(pos(&ids, "build") < pos(&ids, "test"));
}

// --------------------------------------------------------------------------
// Non-commands are ignored
// --------------------------------------------------------------------------

#[test]
fn listing_without_command_role_is_ignored() {
    let src = "[source, bash]\n----\necho hi\n----\n";
    assert!(ids(graph(src)).is_empty());
}

#[test]
fn command_role_without_id_errors() {
    // A `command` block with no id is a typo, not a no-op: surface it.
    let src = "[.command]\n----\necho hi\n----\n";
    assert!(matches!(err(src), Error::MissingId { .. }));
}

#[test]
fn command_role_on_non_listing_block_errors() {
    // An example block carrying the command role is not a script; it cannot be run.
    let src = "[.command, id=x]\n====\nsome content\n====\n";
    assert!(matches!(err(src), Error::NotAScript { ref id, .. } if id == "x"));
}

#[test]
fn non_delimited_block_with_command_role_is_silently_dropped() {
    // A paragraph with the command role is not a listing block and has no id. Paragraphs
    // nest no blocks, so `child_blocks` returns nothing for them and collect_commands
    // moves on without error. This documents that the drop is intentional.
    let src = "[.command]\nThis is a paragraph.\n";
    assert!(ids(graph(src)).is_empty());
}

// --------------------------------------------------------------------------
// Discovery across included files
// --------------------------------------------------------------------------

#[test]
fn command_in_included_file_is_discovered() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let main = dir.path().join("main.adoc");
    let included = dir.path().join("commands.adoc");
    std::fs::write(&main, "= Doc\n\ninclude::commands.adoc[]\n").unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(&included, cmd("build", None, None, "echo hi"))
        .unwrap_or_else(|e| panic!("{e}"));

    let parsed = acdc_parser::parse_file(&main, &Options::default())
        .unwrap_or_else(|e| panic!("parse_file failed: {e}"));
    let built = CommandGraph::try_from(parsed.document()).unwrap_or_else(|e| panic!("{e}"));

    assert_eq!(ids(built), ["build"]);
}

// --------------------------------------------------------------------------
// Shell / language
// --------------------------------------------------------------------------

#[test]
fn default_interpreter_is_sh_when_no_language() {
    let block = find(graph(&cmd("build", None, None, "echo hi")), "build");
    assert_eq!(block.metadata.interpreter, "sh");
}

#[test]
fn source_block_with_no_language_defaults_to_sh() {
    // `[source]` with no language: style=="source" but no None-valued attribute key.
    // Different code path from having no [source,...] annotation at all.
    let src = "[.command, id=build]\n[source]\n----\necho hi\n----\n";
    let block = find(graph(src), "build");
    assert_eq!(block.metadata.interpreter, "sh");
}

#[test]
fn source_interpreter_ignores_block_options() {
    let src = "[.command, id=build]\n[source, bash, %linenums]\n----\necho hi\n----\n";
    let block = find(graph(src), "build");
    assert_eq!(block.metadata.interpreter, "bash");
}

#[rstest]
#[case("bash")]
#[case("zsh")]
#[case("python3")]
#[case("fish")]
fn source_interpreter_sets_interpreter(#[case] lang: &str) {
    let block = find(graph(&cmd("build", None, Some(lang), "echo hi")), "build");
    assert_eq!(block.metadata.interpreter, lang);
}

// --------------------------------------------------------------------------
// Script body
// --------------------------------------------------------------------------

#[rstest]
#[case::single_line("cargo build", "cargo build\n")]
#[case::multiline("set -e\ncargo build", "set -e\ncargo build\n")]
fn script_body_is_captured(#[case] body: &str, #[case] expected: &str) {
    let block = find(graph(&cmd("build", None, None, body)), "build");
    assert_eq!(block.script, expected);
}

#[test]
fn script_location_reports_the_block_line() {
    // The command block opens on line 3 (after the title and a blank line).
    let src = "= Doc\n\n[.command, id=build]\n----\necho hi\n----\n";
    let block = find(graph(src), "build");
    assert_eq!(block.location.start.line, 3);
}

// --------------------------------------------------------------------------
// Dependencies
// --------------------------------------------------------------------------

#[test]
fn dependency_orders_prerequisite_first() {
    let src = format!(
        "{}\n{}",
        cmd("build", None, None, "echo build"),
        cmd("test", Some("build"), None, "echo test"),
    );
    assert_eq!(ids(graph(&src)), ["build", "test"]);
}

#[test]
fn forward_referenced_dependency_resolves() {
    // `test` is declared *before* the `build` it depends on.
    let src = format!(
        "{}\n{}",
        cmd("test", Some("build"), None, "echo test"),
        cmd("build", None, None, "echo build"),
    );
    assert_eq!(ids(graph(&src)), ["build", "test"]);
}

#[test]
fn multiple_dependencies_all_precede_dependent() {
    let src = format!(
        "{}\n{}\n{}",
        cmd("a", None, None, "echo a"),
        cmd("b", None, None, "echo b"),
        cmd("c", Some("a, b"), None, "echo c"),
    );
    let ids = ids(graph(&src));
    assert!(pos(&ids, "a") < pos(&ids, "c"));
    assert!(pos(&ids, "b") < pos(&ids, "c"));
}

#[rstest]
#[case("build", &["build"])]
#[case("a,b", &["a", "b"])]
#[case("a, b", &["a", "b"])]
#[case("  a ,  b  ", &["a", "b"])]
#[case("a, b, c", &["a", "b", "c"])]
fn deps_attribute_is_split_and_trimmed(#[case] deps: &str, #[case] expected: &[&str]) {
    // Provide every named prerequisite so the graph resolves, then check ordering.
    let mut src = String::new();
    for dep in expected {
        src.push_str(&cmd(dep, None, None, "echo dep"));
        src.push('\n');
    }
    src.push_str(&cmd("target", Some(deps), None, "echo target"));

    let ids = ids(graph(&src));
    for dep in expected {
        assert!(
            pos(&ids, dep) < pos(&ids, "target"),
            "{dep} should precede target"
        );
    }
}

#[test]
fn trailing_and_empty_dep_segments_are_ignored() {
    let src = format!(
        "{}\n{}",
        cmd("build", None, None, "echo build"),
        cmd("test", Some("build, ,"), None, "echo test"),
    );
    assert_eq!(ids(graph(&src)), ["build", "test"]);
}

#[test]
fn empty_deps_attribute_yields_no_dependencies() {
    // deps="" splits to [""], filtered to nothing — command has no prerequisites.
    let src = cmd("build", Some(""), None, "echo build");
    assert_eq!(ids(graph(&src)), ["build"]);
}

// --------------------------------------------------------------------------
// Errors
// --------------------------------------------------------------------------

#[test]
fn invalid_dependency_id_errors() {
    let src = format!(
        "{}\n{}",
        cmd("build", None, None, "echo build"),
        cmd("test", Some("bad id"), None, "echo test"),
    );
    assert!(matches!(err(&src), Error::InvalidId(_)));
}

#[test]
fn invalid_command_id_errors() {
    let src = "[.command, id=bad.id]\n----\necho hi\n----\n";
    assert!(matches!(err(src), Error::InvalidId(_)));
}

#[test]
fn duplicate_command_id_errors() {
    let src = format!(
        "{}\n{}",
        cmd("build", None, None, "echo one"),
        cmd("build", None, None, "echo two"),
    );
    assert!(matches!(
        err(&src),
        Error::Build(BuildError::DuplicateId(_))
    ));
}

#[test]
fn unknown_dependency_errors() {
    let src = cmd("test", Some("missing"), None, "echo test");
    assert!(matches!(err(&src), Error::Build(BuildError::UnknownDep(_))));
}

#[test]
fn self_dependency_errors() {
    let src = cmd("loop", Some("loop"), None, "echo loop");
    assert!(matches!(err(&src), Error::Build(BuildError::Cycle { .. })));
}

#[test]
fn dependency_cycle_errors() {
    let src = format!(
        "{}\n{}",
        cmd("a", Some("b"), None, "echo a"),
        cmd("b", Some("a"), None, "echo b"),
    );
    assert!(matches!(err(&src), Error::Build(BuildError::Cycle { .. })));
}

#[test]
fn missing_id_error_reports_the_block_line() {
    let src = "= Doc\n\n[.command]\n----\necho hi\n----\n";
    let Error::MissingId { line } = err(src) else {
        panic!("expected MissingId")
    };
    assert_eq!(line, 3);
}

#[test]
fn not_a_script_error_reports_the_block_line() {
    let src = "= Doc\n\n[.command, id=x]\n====\nsome content\n====\n";
    let Error::NotAScript { line, .. } = err(src) else {
        panic!("expected NotAScript")
    };
    assert_eq!(line, 3);
}

#[rstest]
#[case(Error::MissingId { line: 7 }, "command block at line 7 is missing an id")]
#[case(
    Error::NotAScript { id: "build".into(), line: 4 },
    "command `build` at line 4 is not a listing block"
)]
fn error_display(#[case] error: Error, #[case] expected: &str) {
    assert_eq!(error.to_string(), expected);
}

// --------------------------------------------------------------------------
// Safe mode is a document-read policy, not a command sandbox
// --------------------------------------------------------------------------

#[test]
fn safe_mode_still_discovers_commands() {
    // Commands are discovered and validated identically under SECURE safe mode;
    // safe mode only limits what the document may include.
    let options = Options::builder()
        .with_safe_mode(SafeMode::Secure)
        .build()
        .unwrap_or_else(|e| panic!("{e}"));
    let src = "[.command, id=build]\n----\necho hi\n----\n";
    let parsed = acdc_parser::parse(src, &options).unwrap_or_else(|e| panic!("{e}"));
    let built = CommandGraph::try_from(parsed.document()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(ids(built), ["build"]);
}

// --------------------------------------------------------------------------
// End-to-end example
// --------------------------------------------------------------------------

#[test]
fn readme_example_builds_expected_graph() {
    let src = "= My Project\n\n== Build\n\n[.command, id=build]\n[source, bash]\n----\ncargo xtask build\n----\n\n== Tests\n\n[.command, id=test, deps=\"build\"]\n[source, bash]\n----\ncargo nextest run\n----\n";
    let built = graph(src);
    let blocks: Vec<CommandBlock> = built.into_iter().collect();

    let names: Vec<&str> = blocks.iter().map(|b| b.metadata.id.as_str()).collect();
    assert_eq!(names, ["build", "test"]);
    let first = blocks.first().unwrap_or_else(|| panic!("build missing"));
    let second = blocks.get(1).unwrap_or_else(|| panic!("test missing"));
    assert_eq!(first.metadata.interpreter, "bash");
    assert_eq!(first.script, "cargo xtask build\n");
    assert_eq!(second.script, "cargo nextest run\n");
}
