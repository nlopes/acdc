//! Command construction and execution.

mod error;

pub use error::{BuildError, ExecError, InvalidCommandId, UnknownCommand};

use std::{
    borrow::{Borrow, Cow},
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet},
    ffi::OsString,
    fmt,
    io::Write as _,
    path::{Path, PathBuf},
    process::Command,
};

use acdc_parser::SourceLocation;
use petgraph::{
    graph::{DiGraph, NodeIndex},
    prelude::Direction,
    visit::{Dfs, EdgeRef, Reversed},
};

const DEFAULT_INTERPRETER: &str = "sh";

/// A unique identifier for a command.
///
/// IDs contain only ASCII alphanumerics, `-`, and `_`, and must not start with `-`.
/// Construct via [`CommandId::new`] or [`str::parse`].
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct CommandId(String);

impl CommandId {
    /// Validate `id` and wrap it.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidCommandId`] if `id` is empty, starts with `-`, or contains
    /// a character outside ASCII alphanumerics, `-`, and `_`.
    pub fn new(id: impl Into<String>) -> Result<Self, InvalidCommandId> {
        let id: String = id.into();
        if id.is_empty() {
            return Err(InvalidCommandId::Empty);
        }
        if id.starts_with('-') {
            return Err(InvalidCommandId::LeadingDash { id });
        }
        if let Some(ch) = id.chars().find(|&c| !Self::is_legal(c)) {
            return Err(InvalidCommandId::IllegalChar { id, ch });
        }
        Ok(Self(id))
    }

    /// The id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn is_legal(c: char) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, '-' | '_')
    }
}

impl std::str::FromStr for CommandId {
    type Err = InvalidCommandId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Borrow<str> for CommandId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Metadata for a command block.
#[derive(Clone, Debug)]
pub struct CommandMetadata {
    /// The identifier for the command, e.g. `build` or `test`.
    pub id: CommandId,
    /// The interpreter name or path to use to execute the command.
    ///
    /// This value is what might appear *last* in the *shebang* of a script, e.g. `python3` for
    /// `#!/usr/bin/env python3`, or `bash` for `#!/usr/bin/env bash`. It is passed directly to
    /// [`std::process::Command`] without an allowlist or shell-string parsing, so a document can
    /// select any executable available to the calling process. Executing a document is not a
    /// sandbox boundary.
    pub interpreter: String,
    /// A short, human-readable summary of what the command does, shown when listing commands.
    pub description: Option<String>,
    /// Plain-text title of the nearest enclosing section, or `None` outside a section.
    pub section_title: Option<String>,
}

/// A single command with its metadata.
#[derive(Clone, Debug)]
pub struct CommandBlock {
    /// The metadata associated with this command block.
    pub metadata: CommandMetadata,
    /// The script body.
    pub script: String,
    /// The resolved source file and original document location of this command.
    pub location: SourceLocation,
}

impl CommandBlock {
    /// Preserve `script` unchanged and default `interpreter` to `"sh"` when absent.
    #[must_use]
    pub fn new(
        id: CommandId,
        script: String,
        interpreter: Option<String>,
        location: SourceLocation,
    ) -> Self {
        Self {
            metadata: CommandMetadata {
                id,
                interpreter: interpreter.unwrap_or_else(|| DEFAULT_INTERPRETER.to_string()),
                description: None,
                section_title: None,
            },
            script,
            location,
        }
    }

    /// Attach a description, returning the updated block.
    #[must_use]
    pub fn with_description(mut self, description: Option<String>) -> Self {
        self.metadata.description = description;
        self
    }

    /// Execute the script with inherited working directory, environment, and stdio.
    ///
    /// # Errors
    ///
    /// Returns [`ExecError`] if preparing, running, or waiting for the script fails.
    pub fn execute(&self) -> Result<(), ExecError> {
        self.execute_with(&ProcessOptions::default())
    }

    /// Execute the exact script bytes with options applied only to the child process.
    ///
    /// The interpreter receives a temporary script file as one argument. Stdio is inherited,
    /// and the temporary file is removed after the child exits. Scripts are not sandboxed.
    /// Explicit relative interpreter paths are resolved from the caller's directory, including
    /// when a child working directory is set. Bare interpreter names use the operating system's
    /// executable search.
    ///
    /// # Errors
    ///
    /// Returns [`ExecError::TempFile`] if the script cannot be written, [`ExecError::Process`]
    /// if starting or waiting for the interpreter fails, or [`ExecError::Failed`] if the
    /// child exits unsuccessfully.
    pub fn execute_with(&self, options: &ProcessOptions) -> Result<(), ExecError> {
        let mut tmp = tempfile::NamedTempFile::new().map_err(ExecError::TempFile)?;
        tmp.write_all(self.script.as_bytes())
            .map_err(ExecError::TempFile)?;
        tmp.flush().map_err(ExecError::TempFile)?;

        let interpreter = Path::new(&self.metadata.interpreter);
        let resolved_interpreter = if options.current_dir.is_some()
            && interpreter.is_relative()
            && self.metadata.interpreter.contains(std::path::is_separator)
        {
            Some(
                std::env::current_dir()
                    .map_err(ExecError::Process)?
                    .join(interpreter),
            )
        } else {
            None
        };
        let mut child = Command::new(resolved_interpreter.as_deref().unwrap_or(interpreter));
        child
            .arg(tmp.path())
            .envs(options.env.iter().map(|(name, value)| (name, value)));
        if let Some(directory) = &options.current_dir {
            child.current_dir(directory);
        }
        let status = child.status().map_err(ExecError::Process)?;

        drop(tmp);

        if status.success() {
            Ok(())
        } else {
            Err(ExecError::Failed(status))
        }
    }
}

/// Child-process settings. Defaults inherit the caller's working directory and environment.
#[derive(Clone, Debug, Default)]
pub struct ProcessOptions {
    /// Working directory for each child; relative paths are resolved from the caller's directory.
    pub current_dir: Option<PathBuf>,
    /// Environment overrides, applied in order so the last value for a name wins.
    pub env: Vec<(OsString, OsString)>,
}

/// Policy and child-process settings for an execution plan.
#[derive(Clone, Debug, Default)]
pub struct ExecutionOptions {
    /// Stop all remaining commands after the first unsuccessful child exit.
    ///
    /// Infrastructure errors and signal termination always stop the plan.
    pub exit_on_failure: bool,
    /// Working directory and environment applied to each child.
    pub process: ProcessOptions,
}

/// A directed acyclic graph of commands, typically pulled from a single `AsciiDoc` file.
///
/// Edges run from prerequisite to dependent: an edge `A → B` means "A must run before B."
#[derive(Debug)]
pub struct CommandGraph {
    graph: DiGraph<CommandBlock, ()>,
    index: HashMap<CommandId, NodeIndex>,
    /// All nodes in topological order; the single source of iteration order.
    order: Vec<NodeIndex>,
}

impl CommandGraph {
    /// The id of every command, in topological execution order.
    pub fn ids(&self) -> impl Iterator<Item = &CommandId> {
        let graph = &self.graph;
        self.order.iter().map(move |idx| &graph[*idx].metadata.id)
    }

    /// Whether `id` names a command in this graph.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    /// Direct prerequisites of `id`, each returned once in unspecified order.
    ///
    /// Returns `None` if `id` does not name a command in this graph.
    #[must_use]
    pub fn dependencies<'graph>(
        &'graph self,
        id: &str,
    ) -> Option<impl Iterator<Item = &'graph CommandId> + use<'graph>> {
        let &node = self.index.get(id)?;
        Some(
            self.graph
                .neighbors_directed(node, Direction::Incoming)
                .map(|dependency| &self.graph[dependency].metadata.id),
        )
    }

    /// Select every command in execution order without copying script bodies.
    #[must_use]
    pub fn plan_all(&self) -> ExecutionPlan<'_> {
        ExecutionPlan {
            graph: self,
            order: Cow::Borrowed(&self.order),
        }
    }

    /// Select the given commands and their transitive dependencies in execution order.
    ///
    /// Each selected command appears once. The plan borrows script bodies from this graph.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownCommand`] if any id in `ids` is not in the graph.
    pub fn plan_for(&self, ids: &[CommandId]) -> Result<ExecutionPlan<'_>, UnknownCommand> {
        let rev = Reversed(&self.graph);
        let mut dfs = Dfs::empty(rev);
        let mut relevant = vec![false; self.graph.node_count()];

        // Reuse one DFS so shared ancestors are visited once; `move_to` resets the stack while
        // retaining the visitor's visited set.
        for id in ids {
            let &target = self
                .index
                .get(id)
                .ok_or_else(|| UnknownCommand(id.clone()))?;

            dfs.move_to(target);
            while let Some(node) = dfs.next(rev) {
                if let Some(selected) = relevant.get_mut(node.index()) {
                    *selected = true;
                }
            }
        }

        let order = self
            .order
            .iter()
            .copied()
            .filter(|node| relevant.get(node.index()) == Some(&true))
            .collect();
        Ok(ExecutionPlan {
            graph: self,
            order: Cow::Owned(order),
        })
    }
}

/// Builds a [`CommandGraph`] from commands added in any order.
///
/// Commands may declare dependencies on ids that have not been added yet; all ids are resolved
/// once, at [`build`](CommandGraphBuilder::build). This lifts the ordering constraint that direct
/// insertion would impose, at the cost of deferring every validation error to build time.
#[derive(Debug, Default)]
pub struct CommandGraphBuilder {
    pending: Vec<(CommandBlock, Vec<CommandId>)>,
}

impl CommandGraphBuilder {
    /// Create an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a command and the ids of its prerequisites.
    ///
    /// Ids are not validated here; duplicates, unknown deps, and cycles are all reported by
    /// [`build`](CommandGraphBuilder::build).
    pub fn add(&mut self, block: CommandBlock, deps: Vec<CommandId>) {
        self.pending.push((block, deps));
    }

    /// Resolve all commands into a [`CommandGraph`].
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::DuplicateId`] if two commands share an id, [`BuildError::UnknownDep`]
    /// if a dep names no added command, or [`BuildError::Cycle`] if the dependencies are cyclic.
    pub fn build(self) -> Result<CommandGraph, BuildError> {
        let mut graph = CommandGraph {
            graph: DiGraph::new(),
            index: HashMap::new(),
            order: Vec::new(),
        };

        // Pass 1: add every node and index its id, so deps can be resolved in any order.
        let mut edges: Vec<(NodeIndex, Vec<CommandId>)> = Vec::with_capacity(self.pending.len());
        for (block, deps) in self.pending {
            let id = block.metadata.id.clone();
            if let Some(&first) = graph.index.get::<str>(id.borrow()) {
                return Err(BuildError::DuplicateId {
                    id,
                    first_location: Box::new(graph.graph[first].location.clone()),
                    duplicate_location: Box::new(block.location),
                });
            }
            let node = graph.graph.add_node(block);
            graph.index.insert(id, node);
            edges.push((node, deps));
        }

        // Pass 2: wire edges.
        for (node, deps) in edges {
            let mut seen: HashSet<NodeIndex> = HashSet::new();
            for dep in deps {
                let dep_node = *graph.index.get::<str>(dep.borrow()).ok_or_else(|| {
                    BuildError::UnknownDep {
                        command: graph.graph[node].metadata.id.clone(),
                        dep: dep.clone(),
                        location: Box::new(graph.graph[node].location.clone()),
                    }
                })?;
                if dep_node == node {
                    return Err(BuildError::Cycle {
                        command: graph.graph[node].metadata.id.clone(),
                        dep,
                        command_location: Box::new(graph.graph[node].location.clone()),
                        dep_location: Box::new(graph.graph[node].location.clone()),
                    });
                }
                // A dep listed twice would add a parallel edge; skip the repeat.
                if !seen.insert(dep_node) {
                    continue;
                }
                graph.graph.add_edge(dep_node, node, ());
            }
        }

        // Pass 3: Kahn's algorithm, smallest insertion index first, so commands
        // without dependencies keep document order. Remaining prerequisite edges
        // after sorting contain a cycle.
        let count = graph.graph.node_count();
        let mut indegree = vec![0_usize; count];
        for edge in graph.graph.edge_references() {
            if let Some(degree) = indegree.get_mut(edge.target().index()) {
                *degree += 1;
            }
        }
        let mut ready: BinaryHeap<Reverse<usize>> = indegree
            .iter()
            .enumerate()
            .filter(|(_, degree)| **degree == 0)
            .map(|(index, _)| Reverse(index))
            .collect();
        graph.order = Vec::with_capacity(count);
        while let Some(Reverse(i)) = ready.pop() {
            let node = NodeIndex::new(i);
            for successor in graph.graph.neighbors(node) {
                if let Some(degree) = indegree.get_mut(successor.index()) {
                    *degree -= 1;
                    if *degree == 0 {
                        ready.push(Reverse(successor.index()));
                    }
                }
            }
            graph.order.push(node);
        }
        if graph.order.len() != count {
            let (command, dep) = cycle_edge(&graph.graph, &indegree);
            return Err(BuildError::Cycle {
                command: graph.graph[command].metadata.id.clone(),
                dep: graph.graph[dep].metadata.id.clone(),
                command_location: Box::new(graph.graph[command].location.clone()),
                dep_location: Box::new(graph.graph[dep].location.clone()),
            });
        }

        Ok(graph)
    }
}

fn cycle_edge(
    graph: &DiGraph<CommandBlock, ()>,
    remaining_indegree: &[usize],
) -> (NodeIndex, NodeIndex) {
    let start = remaining_indegree
        .iter()
        .position(|degree| *degree > 0)
        .map_or_else(|| NodeIndex::new(0), NodeIndex::new);
    let mut visited = HashSet::new();
    let mut command = start;

    loop {
        visited.insert(command);
        let dep = graph
            .neighbors_directed(command, Direction::Incoming)
            .filter(|node| {
                remaining_indegree
                    .get(node.index())
                    .is_some_and(|degree| *degree > 0)
            })
            .min()
            .unwrap_or(command);
        if visited.contains(&dep) {
            return (command, dep);
        }
        command = dep;
    }
}

/// Selected commands and their prerequisites, borrowing their scripts from a validated graph.
#[derive(Debug)]
pub struct ExecutionPlan<'graph> {
    graph: &'graph CommandGraph,
    order: Cow<'graph, [NodeIndex]>,
}

impl<'graph> ExecutionPlan<'graph> {
    /// Selected commands in execution order, including their prerequisites.
    #[must_use]
    pub fn commands(&self) -> impl ExactSizeIterator<Item = &'graph CommandBlock> + '_ {
        self.order.iter().map(|node| &self.graph.graph[*node])
    }

    /// Number of selected commands, including prerequisites.
    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Whether no commands were selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Run commands synchronously and return an outcome for every selected command.
    ///
    /// Commands with unsuccessful prerequisites are skipped. Independent commands continue
    /// after ordinary nonzero exits unless [`ExecutionOptions::exit_on_failure`] is set.
    /// Infrastructure errors and signal termination always stop all remaining commands.
    #[must_use]
    pub fn execute(&self, options: &ExecutionOptions) -> ExecutionReport<'graph> {
        let mut outcomes = Vec::with_capacity(self.len());
        let mut succeeded = vec![false; self.graph.graph.node_count()];
        let mut stopped_after = None;

        for &node in self.order.iter() {
            let command = &self.graph.graph[node];
            let state = if let Some(command) = stopped_after {
                CommandState::Skipped(SkipReason::StoppedAfterFailure { command })
            } else if let Some(dependency) = self
                .graph
                .graph
                .neighbors_directed(node, Direction::Incoming)
                .filter(|dependency| succeeded.get(dependency.index()) != Some(&true))
                .min()
            {
                CommandState::Skipped(SkipReason::DependencyFailed {
                    dependency: &self.graph.graph[dependency].metadata.id,
                })
            } else {
                match command.execute_with(&options.process) {
                    Ok(()) => {
                        if let Some(success) = succeeded.get_mut(node.index()) {
                            *success = true;
                        }
                        CommandState::Succeeded
                    }
                    Err(error) => {
                        let must_stop = match &error {
                            ExecError::Failed(status) => {
                                options.exit_on_failure || status.code().is_none()
                            }
                            ExecError::TempFile(_) | ExecError::Process(_) => true,
                        };
                        if must_stop {
                            stopped_after = Some(&command.metadata.id);
                        }
                        CommandState::Failed(error)
                    }
                }
            };
            outcomes.push(CommandOutcome { command, state });
        }

        ExecutionReport { outcomes }
    }
}

/// Results in plan order, including commands that were skipped.
#[derive(Debug)]
pub struct ExecutionReport<'graph> {
    outcomes: Vec<CommandOutcome<'graph>>,
}

impl<'graph> ExecutionReport<'graph> {
    /// Results in the order commands appeared in the plan.
    #[must_use]
    pub fn outcomes(&self) -> &[CommandOutcome<'graph>] {
        &self.outcomes
    }

    /// Whether every selected command succeeded. An empty plan succeeds.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.outcomes
            .iter()
            .all(|outcome| matches!(outcome.state, CommandState::Succeeded))
    }

    /// Take the outcomes, preserving structured errors for the caller.
    #[must_use]
    pub fn into_outcomes(self) -> Vec<CommandOutcome<'graph>> {
        self.outcomes
    }
}

/// A command's outcome and its source metadata.
#[derive(Debug)]
pub struct CommandOutcome<'graph> {
    /// The command that ran or was skipped.
    pub command: &'graph CommandBlock,
    /// The result of executing this command.
    pub state: CommandState<'graph>,
}

/// Execution result for one selected command.
#[derive(Debug)]
pub enum CommandState<'graph> {
    /// The child exited successfully.
    Succeeded,
    /// Preparing or running the child failed.
    Failed(ExecError),
    /// The command was not started.
    Skipped(SkipReason<'graph>),
}

/// Why a selected command was not started.
#[derive(Clone, Copy, Debug)]
pub enum SkipReason<'graph> {
    /// A prerequisite failed or was itself skipped.
    DependencyFailed {
        /// The first unsuccessful prerequisite in document order.
        dependency: &'graph CommandId,
    },
    /// Execution stopped after an earlier command failed.
    StoppedAfterFailure {
        /// The command whose failure stopped execution.
        command: &'graph CommandId,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
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
                    (command == id("a") && dep == id("b"))
                        || (command == id("b") && dep == id("a")),
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

    #[test]
    fn dependencies_return_only_unique_direct_prerequisites() {
        let built = graph(&[
            ("deploy", &["test", "build", "test"]),
            ("test", &["build"]),
            ("build", &["prepare"]),
            ("prepare", &[]),
        ]);
        let dependencies = {
            let id = String::from("deploy");
            built.dependencies(&id).unwrap()
        };
        let mut dependencies = dependencies.map(CommandId::as_str).collect::<Vec<_>>();
        dependencies.sort_unstable();
        assert_eq!(dependencies, ["build", "test"]);
        assert_eq!(built.dependencies("prepare").unwrap().count(), 0);
        assert!(built.dependencies("missing").is_none());
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
        let first = SourceLocation::at_position(
            Some("first.adoc".into()),
            acdc_parser::Position::new(3, 1),
        );
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
}
