//! Command construction and execution.

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

/// Error returned when a string is not a valid [`CommandId`].
#[derive(Debug, thiserror::Error)]
pub enum InvalidCommandId {
    /// The id was empty.
    #[error("command id must not be empty")]
    Empty,
    /// The id started with `-`, which would collide with command-line flag parsing.
    #[error("command id {id:?} must not start with '-'")]
    LeadingDash {
        /// The rejected id.
        id: String,
    },
    /// The id contained a character outside the allowed set.
    #[error("command id {id:?} contains illegal character {ch:?}")]
    IllegalChar {
        /// The rejected id.
        id: String,
        /// The first offending character.
        ch: char,
    },
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

/// Error returned when a [`CommandBlock`] fails to execute.
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    /// The temporary script file could not be created or written.
    #[error("could not write temporary script file: {0}")]
    TempFile(#[source] std::io::Error),
    /// Starting or waiting for the interpreter failed.
    #[error("could not run interpreter: {0}")]
    Process(#[source] std::io::Error),
    /// The child exited unsuccessfully, including termination by a signal.
    #[error("exited with {0}")]
    Failed(std::process::ExitStatus),
}

/// Error returned when building a [`CommandGraph`] from a [`CommandGraphBuilder`] fails.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// Two commands share the same id.
    #[error("duplicate command id: {id}")]
    DuplicateId {
        /// The repeated id.
        id: CommandId,
        /// The first declaration.
        first_location: Box<SourceLocation>,
        /// The duplicate declaration.
        duplicate_location: Box<SourceLocation>,
    },
    /// A declared dependency does not name any added command.
    #[error("command `{command}` declares unknown dependency: {dep}")]
    UnknownDep {
        /// The command declaring the dependency.
        command: CommandId,
        /// The dependency that could not be resolved.
        dep: CommandId,
        /// The dependent command's declaration.
        location: Box<SourceLocation>,
    },
    /// A dependency edge would introduce a cycle.
    ///
    /// The reported edge runs from `dep` (the prerequisite) to `command` (the dependent) and is
    /// one edge in the cycle. A self-dependency has `dep == command`.
    #[error("dependency cycle includes: {dep} -> {command}")]
    Cycle {
        /// The dependent command.
        command: CommandId,
        /// The prerequisite that closes the cycle.
        dep: CommandId,
        /// The dependent command's declaration.
        command_location: Box<SourceLocation>,
        /// The prerequisite's declaration.
        dep_location: Box<SourceLocation>,
    },
}

impl BuildError {
    /// The declaration that caused graph validation to fail.
    #[must_use]
    pub fn source_location(&self) -> &SourceLocation {
        match self {
            Self::DuplicateId {
                duplicate_location, ..
            } => duplicate_location,
            Self::UnknownDep { location, .. } => location,
            Self::Cycle {
                command_location, ..
            } => command_location,
        }
    }

    /// The other declaration involved in a duplicate id or dependency cycle.
    #[must_use]
    pub fn related_location(&self) -> Option<&SourceLocation> {
        match self {
            Self::DuplicateId { first_location, .. } => Some(first_location),
            Self::Cycle { dep_location, .. } => Some(dep_location),
            Self::UnknownDep { .. } => None,
        }
    }
}

/// Error returned when a command id is not found in a [`CommandGraph`].
#[derive(Debug, thiserror::Error)]
#[error("unknown command: {0}")]
pub struct UnknownCommand(pub CommandId);

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
mod tests;
