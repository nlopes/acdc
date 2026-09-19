//! Host-specific path handling.
//!
//! Diagram tools receive paths on their command line. On Windows most of them
//! want backslashes even though the rest of acdc happily works with forward
//! slashes, which is what `native_path` normalises.

use std::path::Path;

/// Render `path` the way the host platform's tools expect to receive it.
///
/// On Unix this is the path as-is; on Windows forward slashes become
/// backslashes.
#[must_use]
pub(crate) fn native_path(path: &Path) -> String {
    let rendered = path.display().to_string();
    if cfg!(windows) {
        rendered.replace('/', "\\")
    } else {
        rendered
    }
}

/// The executable extensions to try when searching `PATH`.
///
/// Unix has none; Windows uses `PATHEXT`, falling back to the usual set when
/// the variable is missing.
pub(crate) fn executable_extensions() -> Vec<String> {
    if cfg!(windows) {
        std::env::var("PATHEXT").map_or_else(
            |_| {
                [".COM", ".EXE", ".BAT", ".CMD"]
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect()
            },
            |pathext| pathext.split(';').map(str::to_string).collect(),
        )
    } else {
        vec![String::new()]
    }
}
