//! Spooling validated image snapshots to disk, so the resolver hands the
//! renderer a stable file path instead of retaining the bytes.

use std::{
    io::Write as _,
    path::{Path, PathBuf},
};

use crate::error::Error;

/// Write `bytes` into `dir` under the content-addressed `name`, returning the
/// path to read them back from.
///
/// The name is a content hash, so an image that resolves to identical bytes is
/// written once: if the destination already exists we reuse it. The caller owns
/// `dir` and is responsible for cleaning it up.
pub(super) fn write(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, Error> {
    let io_err = |path: &Path, source| Error::Io {
        path: path.display().to_string(),
        source,
    };
    std::fs::create_dir_all(dir).map_err(|source| io_err(dir, source))?;
    let dest = dir.join(name);
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|source| io_err(dir, source))?;
    temp.write_all(bytes)
        .map_err(|source| io_err(temp.path(), source))?;
    if let Err(error) = temp.persist_noclobber(&dest)
        && error.error.kind() != std::io::ErrorKind::AlreadyExists
    {
        return Err(io_err(&dest, error.error));
    }
    Ok(dest)
}
