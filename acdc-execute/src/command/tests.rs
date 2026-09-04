//! Unit tests for the command model, graph construction, and execution ordering.

use std::collections::HashSet;

use rstest::rstest;

use super::*;

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

fn id(s: &str) -> CommandId {
    CommandId::new(s).unwrap_or_else(|e| panic!("test id {s:?} should be valid: {e}"))
}

fn block_at(name: &str) -> CommandBlock {
    CommandBlock::new(
        id(name),
        format!("echo \"{name}\""),
        None,
        Location::default(),
    )
}

/// Build a graph from `(command, [deps])` specs, expecting success.
fn graph(specs: &[(&str, &[&str])]) -> CommandGraph {
    build(specs).unwrap_or_else(|e| panic!("graph should build: {e}"))
}

/// Attempt to build a graph from `(command, [deps])` specs.
fn build(specs: &[(&str, &[&str])]) -> Result<CommandGraph, BuildError> {
    let mut builder = CommandGraphBuilder::new();
    for (name, deps) in specs {
        builder.add(block_at(name), deps.iter().map(|d| id(d)).collect());
    }
    builder.build()
}

/// Collect a queue's command ids, in order.
fn order(queue: CommandQueue) -> Vec<String> {
    queue.map(|b| b.metadata.id.as_str().to_string()).collect()
}

/// Index of `name` within an ordered id list.
fn pos(ids: &[String], name: &str) -> usize {
    ids.iter()
        .position(|s| s == name)
        .unwrap_or_else(|| panic!("{name:?} missing from {ids:?}"))
}

// --------------------------------------------------------------------------
// `CommandId` validation
// --------------------------------------------------------------------------

#[rstest]
#[case("foo")]
#[case("some-command")]
#[case("cmd_123")]
#[case("a")]
#[case("A")]
#[case("_")]
#[case("123")]
#[case("a-b_c-1")]
fn new_accepts_valid_ids(#[case] raw: &str) {
    let id = CommandId::new(raw).unwrap_or_else(|e| panic!("{raw:?} should be valid: {e}"));
    assert_eq!(id.as_str(), raw);
}

#[test]
fn new_rejects_empty() {
    assert!(matches!(CommandId::new(""), Err(InvalidCommandId::Empty)));
}

#[rstest]
#[case("-foo")]
#[case("-")]
#[case("--bar")]
fn new_rejects_leading_dash(#[case] raw: &str) {
    match CommandId::new(raw) {
        Err(InvalidCommandId::LeadingDash { id }) => assert_eq!(id, raw),
        other => panic!("expected LeadingDash, got {other:?}"),
    }
}

#[rstest]
#[case(" foo", ' ')]
#[case("foo bar", ' ')]
#[case("foo.bar", '.')]
#[case("foo/bar", '/')]
#[case("héllo", 'é')]
fn new_rejects_illegal_char(#[case] raw: &str, #[case] expected: char) {
    match CommandId::new(raw) {
        Err(InvalidCommandId::IllegalChar { id, ch }) => {
            assert_eq!(id, raw);
            assert_eq!(ch, expected);
        }
        other => panic!("expected IllegalChar, got {other:?}"),
    }
}

#[test]
fn from_str_matches_new() {
    let parsed: CommandId = "build".parse().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(parsed, id("build"));
}

#[test]
fn borrow_enables_str_lookup() {
    let mut set = HashSet::new();
    set.insert(id("build"));
    assert!(set.contains("build"));
}

#[test]
fn display_renders_inner_string() {
    assert_eq!(id("deploy").to_string(), "deploy");
}

// --------------------------------------------------------------------------
// `CommandBlock`
// --------------------------------------------------------------------------

#[test]
fn new_stores_id_script_and_default_interpreter() {
    let block = CommandBlock::new(id("build"), "cargo build".into(), None, Location::default());
    assert_eq!(block.metadata.id, id("build"));
    assert_eq!(block.metadata.interpreter, "sh");
    assert_eq!(block.script, "cargo build\n");
}

#[test]
fn new_stores_explicit_interpreter() {
    let block = CommandBlock::new(
        id("test"),
        String::new(),
        Some("bash".into()),
        Location::default(),
    );
    assert_eq!(block.metadata.interpreter, "bash");
}

#[test]
fn new_preserves_multiline_script() {
    let script = "set -e\ncargo build\necho done\n".to_string();
    let block = CommandBlock::new(id("build"), script.clone(), None, Location::default());
    assert_eq!(block.script, script);
}

#[test]
fn clone_is_independent() {
    let block = CommandBlock::new(
        id("deploy"),
        "echo deploy".into(),
        Some("bash".into()),
        Location::default(),
    );
    let mut cloned = block.clone();
    cloned.script.push_str("echo done\n");
    assert_eq!(block.script, "echo deploy\n");
    assert_eq!(cloned.script, "echo deploy\necho done\n");
}

// --------------------------------------------------------------------------
// `CommandGraphBuilder::build` — success
// --------------------------------------------------------------------------

#[test]
fn build_empty_yields_empty_graph() {
    let built = CommandGraphBuilder::new()
        .build()
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(built.into_iter().count(), 0);
}

#[test]
fn build_single_command() {
    assert_eq!(order(graph(&[("build", &[])]).into_iter()), ["build"]);
}

#[test]
fn build_independent_commands_keep_document_order() {
    // Kahn's algorithm with the smallest insertion index first preserves the
    // order commands were added in when no dependencies force another order.
    let built = graph(&[("c", &[]), ("a", &[]), ("b", &[])]);
    assert_eq!(order(built.into_iter()), ["c", "a", "b"]);
}

#[test]
fn build_resolves_forward_referenced_dep() {
    // `build` depends on `gen`, which is added *after* it.
    let built = graph(&[("build", &["gen"]), ("gen", &[])]);
    let ids = order(built.into_iter());
    assert!(pos(&ids, "gen") < pos(&ids, "build"));
}

#[test]
fn build_orders_chain_dependencies() {
    let built = graph(&[("c", &["b"]), ("b", &["a"]), ("a", &[])]);
    assert_eq!(order(built.into_iter()), ["a", "b", "c"]);
}

#[test]
fn build_orders_diamond_dependencies() {
    let built = graph(&[("d", &["b", "c"]), ("b", &["a"]), ("c", &["a"]), ("a", &[])]);
    let ids = order(built.into_iter());
    assert!(pos(&ids, "a") < pos(&ids, "b"));
    assert!(pos(&ids, "a") < pos(&ids, "c"));
    assert!(pos(&ids, "b") < pos(&ids, "d"));
    assert!(pos(&ids, "c") < pos(&ids, "d"));
}

#[test]
fn build_dedupes_duplicate_dep() {
    let built = graph(&[("b", &["a", "a"]), ("a", &[])]);
    assert_eq!(order(built.into_iter()), ["a", "b"]);
}

// --------------------------------------------------------------------------
// `CommandGraphBuilder::build` — errors
// --------------------------------------------------------------------------

#[test]
fn build_rejects_duplicate_id() {
    match build(&[("build", &[]), ("build", &[])]) {
        Err(BuildError::DuplicateId(got)) => assert_eq!(got, id("build")),
        other => panic!("expected DuplicateId, got {other:?}"),
    }
}

#[test]
fn build_rejects_unknown_dep() {
    match build(&[("build", &["missing"])]) {
        Err(BuildError::UnknownDep(got)) => assert_eq!(got, id("missing")),
        other => panic!("expected UnknownDep, got {other:?}"),
    }
}

#[test]
fn build_rejects_self_dependency() {
    match build(&[("a", &["a"])]) {
        Err(BuildError::Cycle { command, dep }) => {
            assert_eq!(command, id("a"));
            assert_eq!(dep, id("a"));
        }
        other => panic!("expected Cycle, got {other:?}"),
    }
}

#[test]
fn build_rejects_two_node_cycle() {
    match build(&[("a", &["b"]), ("b", &["a"])]) {
        Err(BuildError::Cycle { command, dep }) => {
            assert!(
                (command == id("a") && dep == id("b")) || (command == id("b") && dep == id("a")),
                "cycle edge must name two distinct cycle members, got {command} -> {dep}"
            );
        }
        other => panic!("expected Cycle, got {other:?}"),
    }
}

#[test]
fn build_rejects_longer_cycle() {
    match build(&[("a", &["c"]), ("b", &["a"]), ("c", &["b"])]) {
        Err(BuildError::Cycle { command, dep }) => {
            assert_ne!(command, dep);
            assert!(
                (command == id("a") && dep == id("c"))
                    || (command == id("b") && dep == id("a"))
                    || (command == id("c") && dep == id("b")),
                "reported edge must belong to the cycle: {dep} -> {command}"
            );
        }
        other => panic!("expected Cycle, got {other:?}"),
    }
}

#[rstest]
#[case(BuildError::DuplicateId(id("build")), "duplicate command id: build")]
#[case(BuildError::UnknownDep(id("gen")), "unknown dependency: gen")]
#[case(
    BuildError::Cycle { command: id("b"), dep: id("a") },
    "dependency cycle includes: a -> b"
)]
fn build_error_display(#[case] err: BuildError, #[case] expected: &str) {
    assert_eq!(err.to_string(), expected);
}

#[test]
fn invalid_id_display() {
    assert_eq!(
        InvalidCommandId::Empty.to_string(),
        "command id must not be empty"
    );
    assert_eq!(UnknownCommand(id("x")).to_string(), "unknown command: x");
}

// --------------------------------------------------------------------------
// `CommandGraph::queue_for`
// --------------------------------------------------------------------------

#[test]
fn queue_for_unknown_id_errors() {
    let built = graph(&[("a", &[])]);
    match built.queue_for(&[id("nope")]) {
        Err(UnknownCommand(got)) => assert_eq!(got, id("nope")),
        Ok(_) => panic!("expected UnknownCommand"),
    }
}

#[test]
fn queue_for_empty_ids_is_empty() {
    let built = graph(&[("a", &[]), ("b", &[])]);
    let queue = built.queue_for(&[]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(queue.len(), 0);
}

#[test]
fn queue_for_single_command_without_deps() {
    let built = graph(&[("a", &[]), ("b", &[])]);
    let queue = built
        .queue_for(&[id("a")])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(order(queue), ["a"]);
}

#[test]
fn queue_for_includes_transitive_deps_in_order() {
    let built = graph(&[("c", &["b"]), ("b", &["a"]), ("a", &[]), ("unused", &[])]);
    let queue = built
        .queue_for(&[id("c")])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(order(queue), ["a", "b", "c"]);
}

#[test]
fn queue_for_diamond_dedupes_shared_ancestor() {
    let built = graph(&[("d", &["b", "c"]), ("b", &["a"]), ("c", &["a"]), ("a", &[])]);
    let queue = built
        .queue_for(&[id("d")])
        .unwrap_or_else(|e| panic!("{e}"));
    let ids = order(queue);
    assert_eq!(
        ids.len(),
        4,
        "shared ancestor `a` must appear once: {ids:?}"
    );
    assert!(pos(&ids, "a") < pos(&ids, "b"));
    assert!(pos(&ids, "a") < pos(&ids, "c"));
    assert!(pos(&ids, "b") < pos(&ids, "d"));
    assert!(pos(&ids, "c") < pos(&ids, "d"));
}

#[test]
fn queue_for_duplicate_input_ids_dedupes() {
    let built = graph(&[("a", &[]), ("b", &["a"])]);
    let queue = built
        .queue_for(&[id("b"), id("b")])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(order(queue), ["a", "b"]);
}

#[test]
fn queue_for_multiple_targets_dedupes() {
    // `b` and `c` are independent targets that both depend on `a`.
    let built = graph(&[("a", &[]), ("b", &["a"]), ("c", &["a"])]);
    let queue = built
        .queue_for(&[id("b"), id("c")])
        .unwrap_or_else(|e| panic!("{e}"));
    let mut ids = order(queue);
    ids.sort();
    assert_eq!(ids, ["a", "b", "c"]);
}

#[test]
fn queue_for_target_that_is_ancestor_of_another_target() {
    // `a` is itself a prerequisite of `c`; requesting both must not duplicate `a`.
    let built = graph(&[("a", &[]), ("b", &["a"]), ("c", &["b"])]);
    let queue = built
        .queue_for(&[id("c"), id("a")])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(order(queue), ["a", "b", "c"]);
}

#[test]
fn queue_reports_exact_len() {
    let built = graph(&[("a", &[]), ("b", &["a"]), ("c", &["b"])]);
    let mut queue = built
        .queue_for(&[id("c")])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(queue.len(), 3);
    assert_eq!(queue.size_hint(), (3, Some(3)));
    queue.next();
    assert_eq!(queue.len(), 2);
}

#[test]
fn ids_iterates_in_topological_order() {
    let built = graph(&[("c", &["b"]), ("b", &["a"]), ("a", &[])]);
    assert_eq!(
        built.ids().map(CommandId::as_str).collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert!(built.contains("b"));
    assert!(!built.contains("z"));
}

// --------------------------------------------------------------------------
// `IntoIterator`
// --------------------------------------------------------------------------

#[test]
fn into_iter_yields_every_command_topologically() {
    let built = graph(&[("a", &[]), ("b", &["a"]), ("c", &[]), ("d", &["b"])]);
    let ids = order(built.into_iter());
    assert_eq!(ids.len(), 4);
    assert!(pos(&ids, "a") < pos(&ids, "b"));
    assert!(pos(&ids, "b") < pos(&ids, "d"));
}

// --------------------------------------------------------------------------
// Execution
// --------------------------------------------------------------------------

#[test]
fn execute_succeeds_on_zero_exit() {
    let block = CommandBlock::new(id("ok"), "true".into(), None, Location::default());
    block.execute().unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn execute_fails_on_nonzero_exit() {
    let block = CommandBlock::new(id("bad"), "exit 3".into(), None, Location::default());
    match block.execute() {
        Err(ExecError::Failed(status)) => {
            assert_eq!(status.code(), Some(3));
        }
        Err(e) => panic!("expected Failed, got {e:?}"),
        Ok(()) => panic!("expected failure"),
    }
}

#[test]
fn execute_reports_missing_interpreter() {
    let block = CommandBlock::new(
        id("nope"),
        "true".into(),
        Some("acdc-execute-nonexistent-interpreter".into()),
        Location::default(),
    );
    assert!(matches!(block.execute(), Err(ExecError::Spawn(_))));
}

#[test]
fn execute_uses_declared_interpreter() {
    // `false` is not an interpreter; passing a script file to `sh -c`-style interpreters
    // differs. Use `echo` to prove the script reaches the interpreter's argv.
    let block = CommandBlock::new(
        id("echo"),
        "used-by-test-marker".into(),
        Some("cat".into()),
        Location::default(),
    );
    block.execute().unwrap_or_else(|e| panic!("{e}"));
}
