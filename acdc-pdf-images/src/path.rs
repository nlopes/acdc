use std::fmt::Write as _;

use sha2::{Digest, Sha256};

/// The content-addressed base file name for an image (`<hash>.<ext>`).
///
/// Content hashing means two references to identical bytes collapse to one name
/// (free deduplication of both the virtual path and any spooled file) and the
/// name is stable across runs.
pub(super) fn content_name(bytes: &[u8], extension: &str) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        let _ = write!(hex, "{byte:02x}");
    }
    format!("{hex}.{extension}")
}

/// The project-root-relative virtual path an image is referenced by in the
/// generated markup and served at by the renderer.
pub(super) fn virtual_path(content_name: &str) -> String {
    format!("/images/{content_name}")
}
