use std::path::Path;

use acdc_execute::{CommandGraph, ExecutionOptions, ExecutionPlan};

#[cfg(unix)]
mod filter;
#[cfg(unix)]
mod session;
#[cfg(unix)]
mod terminal;

pub(super) fn run(
    graph: &CommandGraph,
    plan: &ExecutionPlan<'_>,
    options: &ExecutionOptions,
    path: &Path,
    document: &Path,
) -> miette::Result<()> {
    #[cfg(unix)]
    {
        session::run(graph, plan, options, path, document)
    }
    #[cfg(not(unix))]
    {
        let _ = (graph, plan, options, path, document);
        Err(miette::miette!(
            "--record requires a Unix terminal; recording is not supported on this platform"
        ))
    }
}
