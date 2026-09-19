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

/// Express `path` relative to `base`, so a generated image can be referred to
/// from the converted document rather than from wherever the command ran.
///
/// Both paths are normalised first, and `..` is used when `path` sits outside
/// `base`. If either side is relative the two cannot be compared meaningfully,
/// so `path` is returned unchanged.
#[must_use]
pub(crate) fn relative_to(path: &Path, base: &Path) -> PathBuf {
    if !path.is_absolute() || !base.is_absolute() {
        return path.to_path_buf();
    }
    let path = normalize(path);
    let base = normalize(base);

    let mut path_parts = path.components().peekable();
    let mut base_parts = base.components().peekable();
    while let (Some(left), Some(right)) = (path_parts.peek(), base_parts.peek()) {
        if left != right {
            break;
        }
        path_parts.next();
        base_parts.next();
    }

    let mut relative = PathBuf::new();
    for _ in base_parts {
        relative.push("..");
    }
    for component in path_parts {
        relative.push(component.as_os_str());
    }
    if relative.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        relative
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

    #[test]
    fn expresses_an_image_relative_to_the_output() {
        assert_eq!(
            relative_to(Path::new("/doc/images/a.svg"), Path::new("/doc")),
            PathBuf::from("images/a.svg")
        );
        assert_eq!(
            relative_to(Path::new("/doc/a.svg"), Path::new("/doc")),
            PathBuf::from("a.svg")
        );
    }

    #[test]
    fn steps_out_when_the_image_sits_outside_the_output() {
        assert_eq!(
            relative_to(Path::new("/build/img/a.svg"), Path::new("/doc/out")),
            PathBuf::from("../../build/img/a.svg")
        );
    }

    #[test]
    fn leaves_incomparable_paths_alone() {
        assert_eq!(
            relative_to(Path::new("images/a.svg"), Path::new("/doc")),
            PathBuf::from("images/a.svg")
        );
    }
}
