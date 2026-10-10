use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use acdc_parser::{
    IncludeLoader, IncludeSource, IncludeSourceError, IncludeSourceErrorKind,
    IncludeSourceProvider, IncludeSourceTarget,
};
use tower_lsp_server::ls_types::Uri;

use super::{ADOC_EXTENSIONS, Workspace};
use crate::limits::MAX_INDEXABLE_FILE_BYTES;

impl Workspace {
    pub(super) fn update_with_includes(&self, uri: Uri, text: String, version: i32) -> Vec<Uri> {
        let diagnostic_uri = uri.clone();
        let mut affected = self.index_with_includes(uri, text, version);
        affected.extend(self.refresh_document_diagnostics(&diagnostic_uri));
        affected
    }

    pub(super) fn index_with_includes(&self, uri: Uri, text: String, version: i32) -> Vec<Uri> {
        let mut options = self.parser_profiles.get(self.backend_for(&uri)).clone();
        self.include_dependencies.remove(&uri);
        if u64::try_from(text.len()).unwrap_or(u64::MAX) > MAX_INDEXABLE_FILE_BYTES {
            return self.index_document_with_options(uri, text, version, &options);
        }
        let Some(path) = file_path(&uri) else {
            return self.index_document_with_options(uri, text, version, &options);
        };
        let Some(directory) = path.parent() else {
            return self.index_document_with_options(uri, text, version, &options);
        };

        // Snapshot text before parsing. The provider must not keep workspace
        // locks while the parser reads nested includes.
        let mut buffers: HashMap<_, _> = self
            .documents
            .iter()
            .filter_map(|entry| {
                let path = file_path(entry.key())?;
                Some((resolve_path(&path).ok()?, entry.parsed.text_snapshot()))
            })
            .collect();
        if let Ok(path) = resolve_path(&path) {
            buffers.insert(path, Arc::new(text.clone()));
        }
        let provider = Arc::new(EditorIncludeProvider {
            directory: directory.to_path_buf(),
            buffers,
            dependencies: Mutex::new(HashSet::new()),
        });
        options.base_dir = Some(directory.to_path_buf());
        let source_provider = Arc::clone(&provider);
        options.include_loader =
            IncludeLoader::custom(move |target: &IncludeSourceTarget| source_provider.open(target));
        let affected = self.index_document_with_options(uri.clone(), text, version, &options);
        self.include_dependencies.insert(
            uri,
            provider
                .dependencies
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
        );
        affected
    }

    /// Reparse each affected parent once. Each parse records nested targets too.
    pub(super) fn refresh_include_dependents(
        &self,
        changed: &[Uri],
        updated: Option<&Uri>,
    ) -> Vec<Uri> {
        let paths: HashSet<_> = changed
            .iter()
            .filter_map(file_path)
            .flat_map(|path| {
                let resolved = resolve_path(&path).ok();
                std::iter::once(path.into_owned()).chain(resolved)
            })
            .collect();
        let parents: Vec<_> = self
            .include_dependencies
            .iter()
            .filter(|entry| {
                updated != Some(entry.key())
                    && entry
                        .value()
                        .iter()
                        .any(|dependency| paths.iter().any(|path| dependency.starts_with(path)))
            })
            .map(|entry| entry.key().clone())
            .collect();
        let mut affected = Vec::new();
        for uri in parents {
            let snapshot = self
                .documents
                .get(&uri)
                .map(|document| (document.text().to_owned(), document.version));
            if let Some((text, version)) = snapshot {
                affected.extend(self.update_with_includes(uri, text, version));
            }
        }
        affected
    }

    /// Apply disk changes without replacing text held by an open editor buffer.
    pub(crate) fn files_changed(&self, uris: &[Uri]) -> Vec<Uri> {
        for uri in uris {
            if !self.has_document(uri)
                && file_path(uri).is_some_and(|path| {
                    path.extension().is_some_and(|extension| {
                        ADOC_EXTENSIONS.contains(&extension.to_string_lossy().as_ref())
                    })
                })
            {
                self.reindex_file_from_disk(uri);
            }
        }
        let mut affected = self.refresh_include_dependents(uris, None);
        affected.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        affected.dedup();
        affected
    }
}

// Uri::to_file_path converts paths without checking their scheme.
pub(super) fn file_path(uri: &Uri) -> Option<Cow<'_, Path>> {
    if !uri.scheme().as_str().eq_ignore_ascii_case("file")
        || uri.authority().is_some_and(|authority| {
            !authority.host().is_empty() && !authority.host().eq_ignore_ascii_case("localhost")
        })
    {
        return None;
    }
    let path = uri.to_file_path().filter(|path| path.is_absolute())?;
    #[cfg(windows)]
    {
        // Match the parser's lexical normalization before checking containment.
        // URI decoding can leave a canonical Windows prefix with forward slashes.
        std::path::absolute(&path).ok().map(Cow::Owned)
    }
    #[cfg(not(windows))]
    Some(path)
}

struct EditorIncludeProvider {
    directory: PathBuf,
    buffers: HashMap<PathBuf, Arc<String>>,
    dependencies: Mutex<HashSet<PathBuf>>,
}

impl IncludeSourceProvider for EditorIncludeProvider {
    fn open(&self, target: &IncludeSourceTarget) -> Result<IncludeSource, IncludeSourceError> {
        let IncludeSourceTarget::File(path) = target else {
            return Err(IncludeSourceError::new(
                IncludeSourceErrorKind::Unsupported,
                "Remote includes are disabled in acdc-lsp",
            ));
        };
        // Record attempts before opening. A missing target can appear in a
        // buffer or on disk later and must then refresh its parents.
        let mut dependencies = self
            .dependencies
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        dependencies.insert(path.clone());
        let resolved = resolve_path(path).map_err(|error| source_error(&error))?;
        dependencies.insert(resolved.clone());
        drop(dependencies);

        // The parser confines paths lexically. Also check their resolved paths
        // so a symlink cannot read outside the entry document's directory.
        let directory = resolve_path(&self.directory).map_err(|error| source_error(&error))?;
        if !path.starts_with(&self.directory) || !resolved.starts_with(directory) {
            return Err(IncludeSourceError::new(
                IncludeSourceErrorKind::Unavailable,
                "Include target is outside the document directory",
            ));
        }
        if let Some(text) = self.buffers.get(&resolved) {
            return Ok(IncludeSource::from_reader(BufferReader {
                text: Arc::clone(text),
                offset: 0,
            }));
        }
        if !resolved
            .metadata()
            .map_err(|error| source_error(&error))?
            .is_file()
        {
            return Err(IncludeSourceError::new(
                IncludeSourceErrorKind::Unavailable,
                "Include target is not a regular file",
            ));
        }
        let file = File::open(resolved).map_err(|error| source_error(&error))?;
        // Leave selection and the 10 MiB selected-text cap to the parser.
        // A small line selection from a large file must still work.
        Ok(IncludeSource::from_reader(file))
    }
}

/// Resolve existing ancestors too, so unsaved files work beneath symlinked folders.
fn resolve_path(path: &Path) -> io::Result<PathBuf> {
    match path.canonicalize() {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
                return Err(error);
            };
            Ok(resolve_path(parent)?.join(name))
        }
        Err(error) => Err(error),
    }
}

fn source_error(error: &io::Error) -> IncludeSourceError {
    let kind = if error.kind() == io::ErrorKind::NotFound {
        IncludeSourceErrorKind::NotFound
    } else {
        IncludeSourceErrorKind::Unavailable
    };
    IncludeSourceError::new(kind, error.to_string())
}

struct BufferReader {
    text: Arc<String>,
    offset: usize,
}

impl Read for BufferReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut remaining = self.text.as_bytes().get(self.offset..).unwrap_or_default();
        let count = remaining.read(buffer)?;
        self.offset += count;
        Ok(count)
    }
}

#[cfg(test)]
mod tests;
