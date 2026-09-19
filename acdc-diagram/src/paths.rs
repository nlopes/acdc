//! Resolving the paths a diagram block refers to.
//!
//! Asciidoctor resolves every path a document mentions against a base
//! directory and normalises the result without touching the filesystem, so a
//! path that does not exist yet (the image about to be generated) still
//! resolves. These helpers do the same.

use std::path::{Component, Path, PathBuf};

/// Resolve `path` against `base` and normalise it lexically.
///
/// Absolute paths are returned normalised but otherwise untouched. `.` and
/// `..` components are folded away without consulting the filesystem, so this
/// works for files that do not exist yet — and, unlike canonicalisation, never
/// resolves symlinks out from under the author.
#[must_use]
pub(crate) fn resolve(path: &Path, base: &Path) -> PathBuf {
    if path.is_absolute() {
        normalize(path)
    } else {
        normalize(&base.join(path))
    }
}

/// Fold `.` and `..` components away without touching the filesystem.
#[must_use]
pub(crate) fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Only pop a real directory name: popping past the root, or
                // past a leading `..` in a relative path, would change which
                // directory the path refers to.
                let popped = out
                    .components()
                    .next_back()
                    .is_some_and(|last| matches!(last, Component::Normal(_)));
                if popped {
                    out.pop();
                } else {
                    out.push(component.as_os_str());
                }
            }
            Component::Normal(_) | Component::RootDir | Component::Prefix(_) => {
                out.push(component.as_os_str());
            }
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

/// Modification time of `path`, when it can be read.
#[must_use]
pub(crate) fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn resolves_relative_against_base() {
        assert_eq!(
            resolve(Path::new("img/a.png"), Path::new("/doc")),
            PathBuf::from("/doc/img/a.png")
        );
    }

    #[test]
    fn folds_parent_components() {
        assert_eq!(
            resolve(Path::new("../out/a.png"), Path::new("/doc/src")),
            PathBuf::from("/doc/out/a.png")
        );
    }

    #[test]
    fn keeps_absolute_paths() {
        assert_eq!(
            resolve(Path::new("/tmp/a.png"), Path::new("/doc")),
            PathBuf::from("/tmp/a.png")
        );
    }

    #[test]
    fn keeps_leading_parent_of_relative_path() {
        assert_eq!(normalize(Path::new("../a")), PathBuf::from("../a"));
    }
}
