//! The four shapes a diagram tool invocation takes.
//!
//! Diagram tools differ in how they want their input and where they leave
//! their output, but only in four combinations: source on stdin or in a file,
//! result on stdout or in a file. Each generator picks the matching helper and
//! supplies a closure that builds the command line once the temporary paths
//! are known.
//!
//! Every helper works inside one [`tempfile::TempDir`], so a tool that renames
//! its output, writes companion files, or leaves debris behind still cleans up
//! when the directory is dropped.

use std::path::{Path, PathBuf};

use crate::{
    cli::{self, CommandSpec},
    error::{Error, Result},
};

/// Create the scratch directory a single tool invocation works in.
fn scratch_dir(tool: &Path) -> Result<tempfile::TempDir> {
    let prefix = tool.file_stem().map_or_else(
        || "diagram".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    tempfile::Builder::new()
        .prefix(&format!("acdc-{prefix}-"))
        .tempdir()
        .map_err(|source| Error::io("could not create a temporary directory", source))
}

/// Read what the tool produced, honouring a `out_file` rename.
fn read_result(spec: &CommandSpec, target: &Path) -> Result<Vec<u8>> {
    let produced = spec.out_file.as_deref().unwrap_or(target);
    if !produced.exists() {
        return Err(Error::EmptyOutput {
            command: spec.tool_name(),
        });
    }
    let data = std::fs::read(produced)
        .map_err(|source| Error::io(format!("could not read {}", produced.display()), source))?;
    if data.is_empty() {
        return Err(Error::EmptyOutput {
            command: spec.tool_name(),
        });
    }
    Ok(data)
}

/// Reject an empty stdout the same way a missing output file is rejected.
fn non_empty_stdout(spec: &CommandSpec, stdout: Vec<u8>) -> Result<Vec<u8>> {
    if stdout.is_empty() {
        return Err(Error::EmptyOutput {
            command: spec.tool_name(),
        });
    }
    Ok(stdout)
}

/// Feed `code` to the tool on stdin and read the file it writes.
///
/// `build` receives the tool path and the output path to put on the command
/// line.
pub(crate) fn stdin_to_file<F>(
    tool: &Path,
    output_extension: &str,
    code: &[u8],
    build: F,
) -> Result<Vec<u8>>
where
    F: FnOnce(&Path, &Path) -> CommandSpec,
{
    let dir = scratch_dir(tool)?;
    let output = dir.path().join(format!("output.{output_extension}"));
    let spec = build(tool, &output);
    cli::run(&spec, Some(code))?;
    read_result(&spec, &output)
}

/// Feed `code` to the tool on stdin and capture its stdout.
pub(crate) fn stdin_to_stdout<F>(tool: &Path, code: &[u8], build: F) -> Result<Vec<u8>>
where
    F: FnOnce(&Path) -> CommandSpec,
{
    let spec = build(tool);
    let output = cli::run(&spec, Some(code))?;
    non_empty_stdout(&spec, output.stdout)
}

/// Write `code` to a temporary file and read the file the tool writes.
///
/// `build` receives the tool path, the input path and the output path.
pub(crate) fn file_to_file<F>(
    tool: &Path,
    input_extension: &str,
    output_extension: &str,
    code: &[u8],
    build: F,
) -> Result<Vec<u8>>
where
    F: FnOnce(&Path, &Path, &Path) -> CommandSpec,
{
    let dir = scratch_dir(tool)?;
    let input = dir.path().join(format!("input.{input_extension}"));
    let output = dir.path().join(format!("output.{output_extension}"));
    std::fs::write(&input, code)
        .map_err(|source| Error::io(format!("could not write {}", input.display()), source))?;
    let spec = build(tool, &input, &output);
    cli::run(&spec, None)?;
    read_result(&spec, &output)
}

/// Write `code` to a temporary file and capture the tool's stdout.
pub(crate) fn file_to_stdout<F>(
    tool: &Path,
    input_extension: &str,
    code: &[u8],
    build: F,
) -> Result<Vec<u8>>
where
    F: FnOnce(&Path, &Path) -> CommandSpec,
{
    let dir = scratch_dir(tool)?;
    let input = dir.path().join(format!("input.{input_extension}"));
    std::fs::write(&input, code)
        .map_err(|source| Error::io(format!("could not write {}", input.display()), source))?;
    let spec = build(tool, &input);
    let output = cli::run(&spec, None)?;
    non_empty_stdout(&spec, output.stdout)
}

/// Absolute path, as a string, in the form the host's tools expect.
pub(crate) fn native(path: &Path) -> String {
    crate::platform::native_path(path)
}

/// Directory a scratch file lives in, for tools that want it separately.
pub(crate) fn parent_of(path: &Path) -> PathBuf {
    path.parent().unwrap_or(Path::new(".")).to_path_buf()
}
