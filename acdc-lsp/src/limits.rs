//! Size guards for LSP disk-side indexing.
//!
//! The parser pre-sizes its arena to the input length, so parsing a 1 GB
//! file costs multi-GB of RSS. Use [`read_bounded`] instead of reading files directly.

use std::path::Path;

/// Upper bound on files the LSP will read from disk for indexing.
pub(crate) const MAX_INDEXABLE_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// Read a file iff its on-disk size is at most [`MAX_INDEXABLE_FILE_BYTES`].
///
/// Returns `None` and logs a warning for oversized files; the metadata
/// stat avoids slurping the file just to reject it.
pub(crate) fn read_bounded(path: &Path) -> Option<String> {
    let size = std::fs::metadata(path).ok()?.len();
    if size > MAX_INDEXABLE_FILE_BYTES {
        tracing::warn!(
            ?path,
            size,
            limit = MAX_INDEXABLE_FILE_BYTES,
            "skipping file: exceeds LSP indexing size limit"
        );
        return None;
    }
    std::fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn tmp_file_with(contents: &[u8], tag: &str) -> Result<std::path::PathBuf, std::io::Error> {
        let mut path = std::env::temp_dir();
        path.push(format!("acdc_limits_{tag}_{}.tmp", std::process::id()));
        std::fs::write(&path, contents)?;
        Ok(path)
    }

    #[test]
    fn read_bounded_accepts_small_file() -> TestResult {
        let path = tmp_file_with(b"hello\nworld\n", "small")?;
        let result = read_bounded(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(result.as_deref(), Some("hello\nworld\n"));
        Ok(())
    }

    #[test]
    fn read_bounded_rejects_oversized_file() -> TestResult {
        let mut path = std::env::temp_dir();
        path.push(format!("acdc_limits_big_{}.tmp", std::process::id()));
        std::fs::File::create(&path)?.set_len(MAX_INDEXABLE_FILE_BYTES + 1)?;
        let result = read_bounded(&path);
        let _ = std::fs::remove_file(&path);
        assert!(result.is_none(), "files above the limit must be skipped");
        Ok(())
    }
}
