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

fn location() -> SourceLocation {
    SourceLocation::at_location(None, acdc_parser::Location::default())
}

fn block_at(name: &str) -> CommandBlock {
    CommandBlock::new(id(name), format!("echo \"{name}\""), None, location())
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

/// Collect selected command ids in execution order.
fn order(plan: &ExecutionPlan<'_>) -> Vec<String> {
    plan.commands()
        .map(|b| b.metadata.id.as_str().to_string())
        .collect()
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
    let block = CommandBlock::new(id("build"), "cargo build".into(), None, location());
    assert_eq!(block.metadata.id, id("build"));
    assert_eq!(block.metadata.interpreter, "sh");
    assert_eq!(block.script, "cargo build");
}

#[test]
fn new_stores_explicit_interpreter() {
    let block = CommandBlock::new(id("test"), String::new(), Some("bash".into()), location());
    assert_eq!(block.metadata.interpreter, "bash");
}

#[test]
fn new_preserves_multiline_script() {
    let script = "set -e\ncargo build\necho done\n".to_string();
    let block = CommandBlock::new(id("build"), script.clone(), None, location());
    assert_eq!(block.script, script);
}

#[test]
fn clone_is_independent() {
    let block = CommandBlock::new(
        id("deploy"),
        "echo deploy".into(),
        Some("bash".into()),
        location(),
    );
    let mut cloned = block.clone();
    cloned.script.push_str("\necho done\n");
    assert_eq!(block.script, "echo deploy");
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
    assert_eq!(built.plan_all().len(), 0);
}

#[test]
fn build_single_command() {
    assert_eq!(order(&graph(&[("build", &[])]).plan_all()), ["build"]);
}

#[test]
fn build_independent_commands_keep_document_order() {
    // Kahn's algorithm with the smallest insertion index first preserves the
    // order commands were added in when no dependencies force another order.
    let built = graph(&[("c", &[]), ("a", &[]), ("b", &[])]);
    assert_eq!(order(&built.plan_all()), ["c", "a", "b"]);
}

#[test]
fn build_resolves_forward_referenced_dep() {
    // `build` depends on `gen`, which is added *after* it.
    let built = graph(&[("build", &["gen"]), ("gen", &[])]);
    let ids = order(&built.plan_all());
    assert!(pos(&ids, "gen") < pos(&ids, "build"));
}

#[test]
fn build_orders_chain_dependencies() {
    let built = graph(&[("c", &["b"]), ("b", &["a"]), ("a", &[])]);
    assert_eq!(order(&built.plan_all()), ["a", "b", "c"]);
}

#[test]
fn build_orders_diamond_dependencies() {
    let built = graph(&[("d", &["b", "c"]), ("b", &["a"]), ("c", &["a"]), ("a", &[])]);
    let ids = order(&built.plan_all());
    assert!(pos(&ids, "a") < pos(&ids, "b"));
    assert!(pos(&ids, "a") < pos(&ids, "c"));
    assert!(pos(&ids, "b") < pos(&ids, "d"));
    assert!(pos(&ids, "c") < pos(&ids, "d"));
}

#[test]
fn build_dedupes_duplicate_dep() {
    let built = graph(&[("b", &["a", "a"]), ("a", &[])]);
    assert_eq!(order(&built.plan_all()), ["a", "b"]);
}

// --------------------------------------------------------------------------
// `CommandGraphBuilder::build` — errors
// --------------------------------------------------------------------------

#[test]
fn build_rejects_duplicate_id() {
    match build(&[("build", &[]), ("build", &[])]) {
        Err(BuildError::DuplicateId { id: got, .. }) => assert_eq!(got, id("build")),
        other => panic!("expected DuplicateId, got {other:?}"),
    }
}

#[test]
fn build_rejects_unknown_dep() {
    match build(&[("build", &["missing"])]) {
        Err(BuildError::UnknownDep { dep: got, .. }) => assert_eq!(got, id("missing")),
        other => panic!("expected UnknownDep, got {other:?}"),
    }
}

#[test]
fn build_rejects_self_dependency() {
    match build(&[("a", &["a"])]) {
        Err(BuildError::Cycle { command, dep, .. }) => {
            assert_eq!(command, id("a"));
            assert_eq!(dep, id("a"));
        }
        other => panic!("expected Cycle, got {other:?}"),
    }
}

#[test]
fn build_rejects_two_node_cycle() {
    match build(&[("a", &["b"]), ("b", &["a"])]) {
        Err(BuildError::Cycle { command, dep, .. }) => {
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
        Err(BuildError::Cycle { command, dep, .. }) => {
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

#[test]
fn build_error_display() {
    let duplicate = build(&[("build", &[]), ("build", &[])]).unwrap_err();
    assert_eq!(duplicate.to_string(), "duplicate command id: build");
    let missing = build(&[("build", &["gen"])]).unwrap_err();
    assert_eq!(
        missing.to_string(),
        "command `build` declares unknown dependency: gen"
    );
    let cycle = build(&[("a", &["a"])]).unwrap_err();
    assert_eq!(cycle.to_string(), "dependency cycle includes: a -> a");
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
// `CommandGraph::plan_for`
// --------------------------------------------------------------------------

#[test]
fn plan_for_unknown_id_errors() {
    let built = graph(&[("a", &[])]);
    match built.plan_for(&[id("nope")]) {
        Err(UnknownCommand(got)) => assert_eq!(got, id("nope")),
        Ok(_) => panic!("expected UnknownCommand"),
    }
}

#[test]
fn plan_for_empty_ids_is_empty() {
    let built = graph(&[("a", &[]), ("b", &[])]);
    let queue = built.plan_for(&[]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(queue.len(), 0);
}

#[test]
fn plan_for_single_command_without_deps() {
    let built = graph(&[("a", &[]), ("b", &[])]);
    let queue = built.plan_for(&[id("a")]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(order(&queue), ["a"]);
}

#[test]
fn plan_for_includes_transitive_deps_in_order() {
    let built = graph(&[("c", &["b"]), ("b", &["a"]), ("a", &[]), ("unused", &[])]);
    let queue = built.plan_for(&[id("c")]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(order(&queue), ["a", "b", "c"]);
}

#[test]
fn plan_for_diamond_dedupes_shared_ancestor() {
    let built = graph(&[("d", &["b", "c"]), ("b", &["a"]), ("c", &["a"]), ("a", &[])]);
    let queue = built.plan_for(&[id("d")]).unwrap_or_else(|e| panic!("{e}"));
    let ids = order(&queue);
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
fn plan_for_duplicate_input_ids_dedupes() {
    let built = graph(&[("a", &[]), ("b", &["a"])]);
    let queue = built
        .plan_for(&[id("b"), id("b")])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(order(&queue), ["a", "b"]);
}

#[test]
fn plan_for_multiple_targets_dedupes() {
    // `b` and `c` are independent targets that both depend on `a`.
    let built = graph(&[("a", &[]), ("b", &["a"]), ("c", &["a"])]);
    let queue = built
        .plan_for(&[id("b"), id("c")])
        .unwrap_or_else(|e| panic!("{e}"));
    let mut ids = order(&queue);
    ids.sort();
    assert_eq!(ids, ["a", "b", "c"]);
}

#[test]
fn plan_for_target_that_is_ancestor_of_another_target() {
    // `a` is itself a prerequisite of `c`; requesting both must not duplicate `a`.
    let built = graph(&[("a", &[]), ("b", &["a"]), ("c", &["b"])]);
    let queue = built
        .plan_for(&[id("c"), id("a")])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(order(&queue), ["a", "b", "c"]);
}

#[test]
fn plan_commands_report_exact_len() {
    let built = graph(&[("a", &[]), ("b", &["a"]), ("c", &["b"])]);
    let plan = built.plan_for(&[id("c")]).unwrap();
    assert_eq!(plan.len(), 3);
    let mut commands = plan.commands();
    assert_eq!(commands.len(), 3);
    commands.next();
    assert_eq!(commands.len(), 2);
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
// `CommandGraph::plan_all`
// --------------------------------------------------------------------------

#[test]
fn plan_all_yields_every_command_topologically() {
    let built = graph(&[("a", &[]), ("b", &["a"]), ("c", &[]), ("d", &["b"])]);
    let ids = order(&built.plan_all());
    assert_eq!(ids.len(), 4);
    assert!(pos(&ids, "a") < pos(&ids, "b"));
    assert!(pos(&ids, "b") < pos(&ids, "d"));
}

// --------------------------------------------------------------------------
// Execution
// --------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn execute_succeeds_on_zero_exit() {
    let block = CommandBlock::new(id("ok"), "true".into(), None, location());
    block.execute().unwrap_or_else(|e| panic!("{e}"));
}

#[cfg(unix)]
#[test]
fn execute_fails_on_nonzero_exit() {
    let block = CommandBlock::new(id("bad"), "exit 3".into(), None, location());
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
        location(),
    );
    assert!(matches!(block.execute(), Err(ExecError::Process(_))));
}

#[rstest]
#[case("")]
#[case("printf hello")]
#[case("printf hello\r\n\r\n")]
#[case("printf hello\n\n  ")]
fn constructor_preserves_script_bytes(#[case] script: &str) {
    let block = CommandBlock::new(id("exact"), script.to_owned(), None, location());
    assert_eq!(block.script.as_bytes(), script.as_bytes());
}

#[test]
fn plans_borrow_the_graphs_command_storage() {
    let built = graph(&[("first", &[]), ("last", &["first"])]);
    let first = built.plan_all();
    let second = built.plan_for(&[id("last")]).unwrap();
    for (original, selected) in first.commands().zip(second.commands()) {
        assert!(std::ptr::eq(original, selected));
    }
}

#[test]
fn graph_errors_keep_resolved_source_locations() {
    let first =
        SourceLocation::at_position(Some("first.adoc".into()), acdc_parser::Position::new(3, 1));
    let second = SourceLocation::at_position(
        Some("included/second.adoc".into()),
        acdc_parser::Position::new(7, 1),
    );
    let command = |location| CommandBlock::new(id("build"), String::new(), None, location);
    let mut builder = CommandGraphBuilder::new();
    builder.add(command(first.clone()), Vec::new());
    builder.add(command(second.clone()), Vec::new());
    let error = builder.build().unwrap_err();
    assert_eq!(error.source_location(), &second);
    assert_eq!(error.related_location(), Some(&first));

    let mut builder = CommandGraphBuilder::new();
    builder.add(command(second.clone()), vec![id("missing")]);
    let error = builder.build().unwrap_err();
    assert_eq!(error.source_location(), &second);
    assert!(matches!(error, BuildError::UnknownDep { command, dep, .. }
        if command == id("build") && dep == id("missing")));
}

#[cfg(unix)]
mod process_tests {
    use std::{fs, os::unix::fs::PermissionsExt};

    use super::*;

    fn script(name: &str, body: &str) -> CommandBlock {
        CommandBlock::new(id(name), body.to_owned(), None, location())
    }

    fn execution_graph(specs: &[(&str, &[&str], &str)]) -> CommandGraph {
        let mut builder = CommandGraphBuilder::new();
        for (name, dependencies, body) in specs {
            builder.add(
                script(name, body),
                dependencies
                    .iter()
                    .map(|dependency| id(dependency))
                    .collect(),
            );
        }
        builder.build().unwrap()
    }

    fn options(directory: &Path) -> ExecutionOptions {
        ExecutionOptions {
            process: ProcessOptions {
                current_dir: Some(directory.to_owned()),
                env: Vec::new(),
            },
            ..ExecutionOptions::default()
        }
    }

    #[test]
    fn failed_prerequisite_skips_transitive_dependents_and_runs_independent_commands() {
        let directory = tempfile::tempdir().unwrap();
        let graph = execution_graph(&[
            ("build", &[], "exit 7"),
            ("test", &["build"], "touch test"),
            ("deploy", &["test"], "touch deploy"),
            ("independent", &[], "touch independent"),
        ]);
        let report = graph.plan_all().execute(&options(directory.path()));
        assert!(!report.is_success());
        assert!(matches!(report.outcomes(), [
            CommandOutcome { state: CommandState::Failed(ExecError::Failed(status)), .. },
            CommandOutcome { state: CommandState::Skipped(SkipReason::DependencyFailed { dependency: first }), .. },
            CommandOutcome { state: CommandState::Skipped(SkipReason::DependencyFailed { dependency: second }), .. },
            CommandOutcome { state: CommandState::Succeeded, .. },
        ] if status.code() == Some(7) && first.as_str() == "build" && second.as_str() == "test"));
        assert!(!directory.path().join("test").exists());
        assert!(!directory.path().join("deploy").exists());
        assert!(directory.path().join("independent").exists());
    }

    #[test]
    fn shared_prerequisite_runs_once_before_both_branches() {
        let directory = tempfile::tempdir().unwrap();
        let graph = execution_graph(&[
            ("join", &["left", "right"], "printf join >> order"),
            ("left", &["first"], "printf 'left ' >> order"),
            ("right", &["first"], "printf 'right ' >> order"),
            ("first", &[], "printf 'first ' >> order"),
        ]);
        let report = graph
            .plan_for(&[id("join")])
            .unwrap()
            .execute(&options(directory.path()));
        assert!(report.is_success());
        assert_eq!(
            fs::read_to_string(directory.path().join("order")).unwrap(),
            "first left right join"
        );
    }

    #[test]
    fn diamond_failure_skips_both_branches_and_their_join() {
        let directory = tempfile::tempdir().unwrap();
        let graph = execution_graph(&[
            ("first", &[], "exit 3"),
            ("left", &["first"], "touch left"),
            ("right", &["first"], "touch right"),
            ("join", &["right", "left"], "touch join"),
        ]);
        let report = graph.plan_all().execute(&options(directory.path()));
        assert!(matches!(report.outcomes().last(), Some(CommandOutcome {
            state: CommandState::Skipped(SkipReason::DependencyFailed { dependency }), ..
        }) if dependency.as_str() == "left"));
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn exit_on_failure_skips_independent_commands() {
        let directory = tempfile::tempdir().unwrap();
        let graph = execution_graph(&[("bad", &[], "exit 1"), ("after", &[], "touch after")]);
        let mut options = options(directory.path());
        options.exit_on_failure = true;
        let report = graph.plan_all().execute(&options);
        assert!(matches!(report.outcomes().last(), Some(CommandOutcome {
            state: CommandState::Skipped(SkipReason::StoppedAfterFailure { command }), ..
        }) if command.as_str() == "bad"));
        assert!(!directory.path().join("after").exists());
    }

    #[test]
    fn missing_interpreter_stops_all_commands() {
        let directory = tempfile::tempdir().unwrap();
        let mut builder = CommandGraphBuilder::new();
        let mut bad = script("bad", "true");
        bad.metadata.interpreter = directory
            .path()
            .join("missing-interpreter")
            .display()
            .to_string();
        builder.add(bad, Vec::new());
        builder.add(script("dependent", "touch dependent"), vec![id("bad")]);
        builder.add(script("independent", "touch independent"), Vec::new());
        let graph = builder.build().unwrap();
        let report = graph.plan_all().execute(&options(directory.path()));
        assert!(matches!(
            report.outcomes().first(),
            Some(CommandOutcome {
                state: CommandState::Failed(ExecError::Process(_)),
                ..
            })
        ));
        assert!(report.outcomes().iter().skip(1).all(|outcome| matches!(
            outcome.state,
            CommandState::Skipped(SkipReason::StoppedAfterFailure { .. })
        )));
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn signal_termination_stops_independent_commands() {
        let directory = tempfile::tempdir().unwrap();
        let graph = execution_graph(&[
            ("signal", &[], "kill -TERM $$"),
            ("after", &[], "touch after"),
        ]);
        let report = graph.plan_all().execute(&options(directory.path()));
        assert!(matches!(report.outcomes().first(), Some(CommandOutcome {
            state: CommandState::Failed(ExecError::Failed(status)), ..
        }) if status.code().is_none()));
        assert!(matches!(
            report.outcomes().last(),
            Some(CommandOutcome {
                state: CommandState::Skipped(SkipReason::StoppedAfterFailure { .. }),
                ..
            })
        ));
        assert!(!directory.path().join("after").exists());
    }

    #[test]
    fn child_options_do_not_change_the_parent() {
        let directory = tempfile::tempdir().unwrap();
        let parent_directory = std::env::current_dir().unwrap();
        let parent_value = std::env::var_os("ACDC_EXECUTE_TEST_VALUE");
        let options = ProcessOptions {
            current_dir: Some(directory.path().to_owned()),
            env: vec![
                ("ACDC_EXECUTE_TEST_VALUE".into(), "first".into()),
                ("ACDC_EXECUTE_TEST_VALUE".into(), "last=kept".into()),
            ],
        };
        script("child", "printf '%s' \"$ACDC_EXECUTE_TEST_VALUE\" > value")
            .execute_with(&options)
            .unwrap();
        assert_eq!(
            fs::read_to_string(directory.path().join("value")).unwrap(),
            "last=kept"
        );
        assert_eq!(std::env::current_dir().unwrap(), parent_directory);
        assert_eq!(std::env::var_os("ACDC_EXECUTE_TEST_VALUE"), parent_value);
    }

    #[test]
    fn interpreter_receives_exact_script_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let interpreter = directory.path().join("copy script");
        fs::write(
            &interpreter,
            "#!/bin/sh\ncat \"$1\" > \"$ACDC_SCRIPT_COPY\"\n",
        )
        .unwrap();
        fs::set_permissions(&interpreter, fs::Permissions::from_mode(0o700)).unwrap();
        let options = ProcessOptions {
            env: vec![(
                "ACDC_SCRIPT_COPY".into(),
                directory.path().join("copy").into_os_string(),
            )],
            ..ProcessOptions::default()
        };
        for source in ["", "printf hello", "one\r\ntwo\r\n\r\n", "trailing\n\n  "] {
            let command = CommandBlock::new(
                id("exact"),
                source.into(),
                Some(interpreter.display().to_string()),
                location(),
            );
            command.execute_with(&options).unwrap();
            assert_eq!(
                fs::read(directory.path().join("copy")).unwrap(),
                source.as_bytes()
            );
        }
    }

    #[test]
    fn explicit_relative_interpreter_uses_the_callers_directory() {
        let caller = std::env::current_dir().unwrap();
        let directory = tempfile::tempdir_in(&caller).unwrap();
        let interpreter = directory.path().join("interpreter");
        fs::write(&interpreter, "#!/bin/sh\n/bin/sh \"$1\"\n").unwrap();
        fs::set_permissions(&interpreter, fs::Permissions::from_mode(0o700)).unwrap();
        let child_directory = directory.path().join("child");
        fs::create_dir(&child_directory).unwrap();
        let relative = interpreter.strip_prefix(&caller).unwrap();
        let command = CommandBlock::new(
            id("relative"),
            "touch marker".into(),
            Some(relative.display().to_string()),
            location(),
        );
        command
            .execute_with(&ProcessOptions {
                current_dir: Some(child_directory.clone()),
                env: Vec::new(),
            })
            .unwrap();
        assert!(child_directory.join("marker").exists());
        assert_eq!(std::env::current_dir().unwrap(), caller);
    }
}
