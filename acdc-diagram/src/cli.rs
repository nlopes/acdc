//! Running an external diagram tool.
//!
//! Every generator ends up here: a command line, an optional working
//! directory, extra environment variables and an optional stdin payload. A
//! non-zero exit status becomes [`Error::CommandFailed`] carrying whatever the
//! tool complained about, which is the message document authors actually need.

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::error::{Error, Result};

/// One invocation of a diagram tool.
#[derive(Debug, Clone, Default)]
pub(crate) struct CommandSpec {
    /// Executable followed by its arguments.
    pub(crate) args: Vec<String>,
    /// Extra environment variables for the child process.
    pub(crate) env: Vec<(String, String)>,
    /// Working directory for the child process.
    pub(crate) chdir: Option<PathBuf>,
    /// File the tool actually writes, when it differs from the output path we
    /// asked for (lilypond and pdflatex both rename their output).
    pub(crate) out_file: Option<PathBuf>,
}

impl CommandSpec {
    /// Start a spec from the executable path.
    pub(crate) fn new(tool: &Path) -> Self {
        Self {
            args: vec![tool.display().to_string()],
            ..Self::default()
        }
    }

    /// Append one argument.
    pub(crate) fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Append several arguments.
    pub(crate) fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set an environment variable for the child process.
    pub(crate) fn env(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((name.into(), value.into()));
        self
    }

    /// Run the child process in `dir`.
    pub(crate) fn chdir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.chdir = Some(dir.into());
        self
    }

    /// Declare that the tool writes its result to `path` rather than to the
    /// output path on the command line.
    pub(crate) fn out_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.out_file = Some(path.into());
        self
    }

    /// Basename of the executable, for error messages.
    pub(crate) fn tool_name(&self) -> String {
        self.args.first().map_or_else(String::new, |program| {
            Path::new(program).file_name().map_or_else(
                || program.clone(),
                |name| name.to_string_lossy().into_owned(),
            )
        })
    }
}

/// What a diagram tool produced.
#[derive(Debug)]
pub(crate) struct CommandOutput {
    /// Raw bytes the tool wrote to stdout.
    pub(crate) stdout: Vec<u8>,
}

/// Run `spec`, optionally feeding `stdin_data` to the child.
///
/// # Errors
///
/// Returns [`Error::Io`] when the process cannot be spawned and
/// [`Error::CommandFailed`] when it exits with a non-zero status.
pub(crate) fn run(spec: &CommandSpec, stdin_data: Option<&[u8]>) -> Result<CommandOutput> {
    let Some((program, arguments)) = spec.args.split_first() else {
        return Err(Error::config(
            "diagram tool invoked with an empty command line",
        ));
    };

    tracing::debug!(?spec, "running diagram tool");

    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(if stdin_data.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (name, value) in &spec.env {
        command.env(name, value);
    }
    if let Some(dir) = &spec.chdir {
        command.current_dir(dir);
    }

    let tool = spec.tool_name();
    let mut child = command
        .spawn()
        .map_err(|source| Error::io(format!("could not run `{tool}`"), source))?;

    if let Some(data) = stdin_data {
        // `take()` so the pipe is closed once written: tools that read to EOF
        // would otherwise block forever.
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(data).map_err(|source| {
                Error::io(format!("could not write to `{tool}` stdin"), source)
            })?;
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|source| Error::io(format!("could not read output of `{tool}`"), source))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(Error::CommandFailed {
            command: tool,
            output: if stderr.is_empty() { stdout } else { stderr },
        });
    }

    Ok(CommandOutput {
        stdout: output.stdout,
    })
}
