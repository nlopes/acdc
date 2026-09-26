//! Command construction and execution.

use std::borrow::Borrow;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fmt;
use std::io::Write as _;

use acdc_parser::Location;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::prelude::Direction;
use petgraph::visit::{Dfs, EdgeRef, Reversed};

const DEFAULT_INTERPRETER: &str = "sh";

/// A unique identifier for a command.
///
/// Ids are restricted to ASCII alphanumerics, `-`, and `_`, so they are safe to use unquoted on
/// the command line and as `AsciiDoc` element ids. Construct via [`CommandId::new`] or
/// [`str::parse`]; the inner string is validated on construction and never exposed for mutation.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct CommandId(String);

impl CommandId {
    /// Validate `id` and wrap it.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidCommandId`] if `id` is empty, starts with `-`, or contains whitespace.
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
    /// Where the command block starts in the document. Positions retain the
    /// `include::` chain for content that came from an included file.
    pub location: Location,
}

impl CommandBlock {
    /// Construct a [`CommandBlock`], defaulting `interpreter` to `"sh"` when absent.
    #[must_use]
    pub fn new(
        id: CommandId,
        script: String,
        interpreter: Option<String>,
        location: Location,
    ) -> Self {
        let mut script = script;
        if !script.ends_with('\n') {
            script.push('\n');
        }
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

    /// Execute the script.
    ///
    /// The script is written to a freshly created temporary file with owner-only permissions, and
    /// the configured interpreter is invoked with that file as its argument. The interpreter is
    /// invoked explicitly rather than through a shebang and the OS loader: shebangs are honored
    /// only on Unix and require the executable bit, which would ignore
    /// [`CommandMetadata::interpreter`]. Explicit invocation is portable across platforms and
    /// interpreters.
    ///
    /// The command runs in the current working directory of the calling process and inherits its
    /// environment. The temporary file is deleted once the child exits; on Windows a file held
    /// open by another process cannot be unlinked, so the child's exit is the earliest safe
    /// deletion point.
    ///
    /// # Errors
    ///
    /// Returns [`ExecError::TempFile`] if the temporary file cannot be created or written,
    /// [`ExecError::Spawn`] if the interpreter cannot be spawned, or [`ExecError::Failed`] if the
    /// process exits with a non-zero status.
    pub fn execute(&self) -> Result<(), ExecError> {
        use std::process::Command;

        let mut tmp = tempfile::NamedTempFile::new().map_err(ExecError::TempFile)?;
        tmp.write_all(self.script.as_bytes())
            .map_err(ExecError::TempFile)?;
        tmp.flush().map_err(ExecError::TempFile)?;

        let status = Command::new(&self.metadata.interpreter)
            .arg(tmp.path())
            .status()
            .map_err(ExecError::Spawn)?;

        drop(tmp);

        if status.success() {
            Ok(())
        } else {
            Err(ExecError::Failed(status))
        }
    }
}

/// Error returned when a [`CommandBlock`] fails to execute.
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    /// The temporary script file could not be created or written.
    #[error("could not write temporary script file: {0}")]
    TempFile(#[source] std::io::Error),
    /// The interpreter could not be spawned.
    #[error("could not spawn interpreter: {0}")]
    Spawn(#[source] std::io::Error),
    /// The interpreter ran but exited with a non-zero status.
    #[error("script exited with {0}")]
    Failed(std::process::ExitStatus),
}

/// Error returned when building a [`CommandGraph`] from a [`CommandGraphBuilder`] fails.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// Two commands share the same id.
    #[error("duplicate command id: {0}")]
    DuplicateId(CommandId),
    /// A declared dependency does not name any added command.
    #[error("unknown dependency: {0}")]
    UnknownDep(CommandId),
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
    },
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

    fn collect_queue(&self, filter: &HashSet<NodeIndex>) -> CommandQueue {
        // Ordering is load-bearing: `order` is topological and filtering by membership
        // preserves it. Do not iterate `filter`.
        let queue: Vec<CommandBlock> = self
            .order
            .iter()
            .filter(|idx| filter.contains(idx))
            .map(|idx| self.graph[*idx].clone())
            .collect();
        CommandQueue(queue.into_iter())
    }

    /// Build a [`CommandQueue`] for the given commands and all of their transitive dependencies,
    /// in execution order.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownCommand`] if any id in `ids` is not in the graph.
    pub fn queue_for(&self, ids: &[CommandId]) -> Result<CommandQueue, UnknownCommand> {
        let rev = Reversed(&self.graph);
        let mut dfs = Dfs::empty(rev);
        let mut relevant: HashSet<NodeIndex> = HashSet::new();

        // Reuse one DFS so shared ancestors are visited once; `move_to` resets the stack while
        // retaining the visitor's visited set.
        for id in ids {
            let &target = self
                .index
                .get(id)
                .ok_or_else(|| UnknownCommand(id.clone()))?;

            dfs.move_to(target);
            while let Some(node) = dfs.next(rev) {
                relevant.insert(node);
            }
        }

        Ok(self.collect_queue(&relevant))
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
            if graph.index.contains_key::<str>(id.borrow()) {
                return Err(BuildError::DuplicateId(id));
            }
            let node = graph.graph.add_node(block);
            graph.index.insert(id, node);
            edges.push((node, deps));
        }

        // Pass 2: wire edges.
        for (node, deps) in edges {
            let mut seen: HashSet<NodeIndex> = HashSet::new();
            for dep in deps {
                let dep_node = *graph
                    .index
                    .get::<str>(dep.borrow())
                    .ok_or_else(|| BuildError::UnknownDep(dep.clone()))?;
                if dep_node == node {
                    return Err(BuildError::Cycle {
                        command: graph.graph[node].metadata.id.clone(),
                        dep,
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
        // without dependencies keep document order. Leftover indegree means the
        // dependencies are cyclic; report the lowest-indexed cycle member with
        // one of its prerequisites.
        let count = graph.graph.node_count();
        let mut indegree: HashMap<usize, usize> = HashMap::new();
        for edge in graph.graph.edge_references() {
            *indegree.entry(edge.target().index()).or_default() += 1;
        }
        let mut ready: BinaryHeap<Reverse<usize>> = (0..count)
            .filter(|i| !indegree.contains_key(i))
            .map(Reverse)
            .collect();
        graph.order = Vec::with_capacity(count);
        while let Some(Reverse(i)) = ready.pop() {
            let node = NodeIndex::new(i);
            for successor in graph.graph.neighbors(node) {
                match indegree.remove(&successor.index()) {
                    Some(1) => ready.push(Reverse(successor.index())),
                    // `Some(count)` is at least 2 here; the decrement keeps it positive.
                    Some(count) => {
                        indegree.insert(successor.index(), count - 1);
                    }
                    None => {}
                }
            }
            graph.order.push(node);
        }
        if graph.order.len() != count {
            let (command, dep) = cycle_edge(&graph.graph, &indegree);
            return Err(BuildError::Cycle {
                command: graph.graph[command].metadata.id.clone(),
                dep: graph.graph[dep].metadata.id.clone(),
            });
        }

        Ok(graph)
    }
}

fn cycle_edge(
    graph: &DiGraph<CommandBlock, ()>,
    remaining_indegree: &HashMap<usize, usize>,
) -> (NodeIndex, NodeIndex) {
    let start = remaining_indegree
        .keys()
        .copied()
        .min()
        .map_or_else(|| NodeIndex::new(0), NodeIndex::new);
    let mut visited = HashSet::new();
    let mut command = start;

    loop {
        visited.insert(command);
        let dep = graph
            .neighbors_directed(command, Direction::Incoming)
            .filter(|node| remaining_indegree.contains_key(&node.index()))
            .min()
            .unwrap_or(command);
        if visited.contains(&dep) {
            return (command, dep);
        }
        command = dep;
    }
}

impl IntoIterator for CommandGraph {
    type Item = CommandBlock;
    type IntoIter = CommandQueue;

    fn into_iter(mut self) -> Self::IntoIter {
        let (nodes, _edges) = self.graph.into_nodes_edges();
        let mut blocks: Vec<Option<CommandBlock>> =
            nodes.into_iter().map(|n| Some(n.weight)).collect();
        // `order` holds every node index exactly once, so every slot is filled.
        let queue: Vec<CommandBlock> = self
            .order
            .drain(..)
            .filter_map(|idx| blocks.get_mut(idx.index()).and_then(Option::take))
            .collect();
        CommandQueue(queue.into_iter())
    }
}

/// A queue of commands in execution order, produced by [`CommandGraph::queue_for`] or by
/// iterating a [`CommandGraph`].
#[derive(Debug)]
pub struct CommandQueue(std::vec::IntoIter<CommandBlock>);

impl Iterator for CommandQueue {
    type Item = CommandBlock;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl ExactSizeIterator for CommandQueue {}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
