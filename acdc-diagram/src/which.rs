//! Locating diagram tools on disk.
//!
//! Mirrors asciidoctor-diagram's two-step lookup: a document attribute may
//! point straight at an executable, and otherwise the tool is searched for on
//! `PATH` under its own name and any alternative names it is packaged under
//! (`svgbob_cli` for `svgbob`, `blockdiag3` for `blockdiag`, …).

use std::path::{Path, PathBuf};

use crate::platform::executable_extensions;

/// Whether `path` names a file the current process can execute.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Whether `path` names a file the current process can execute.
#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.is_file())
}

/// Whether an explicit path from a document attribute can be run.
///
/// Attribute values are trusted to name the tool directly, so a bare existing
/// file counts even when the executable bit is not set — the failure then
/// surfaces from the tool invocation with a far more useful message.
pub(crate) fn is_usable_command_path(path: &Path) -> bool {
    is_executable(path) || path.is_file()
}

/// Search `extra_paths` and then `PATH` for an executable named `command`.
pub(crate) fn which(command: &str, extra_paths: &[PathBuf]) -> Option<PathBuf> {
    // An explicit relative or absolute path bypasses the search entirely, the
    // same way a shell would treat it.
    if command.contains('/') || command.contains('\\') {
        let direct = PathBuf::from(command);
        return is_usable_command_path(&direct).then_some(direct);
    }

    let extensions = executable_extensions();
    let path_var = std::env::var_os("PATH");
    let search = extra_paths.iter().cloned().chain(
        path_var
            .as_ref()
            .map(|paths| std::env::split_paths(paths).collect::<Vec<_>>())
            .unwrap_or_default(),
    );

    for dir in search {
        for extension in &extensions {
            let candidate = dir.join(format!("{command}{extension}"));
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}
