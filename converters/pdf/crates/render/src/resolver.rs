use std::{borrow::Cow, collections::HashMap, path::PathBuf};

use acdc_pdf_images::ImageMap;
use typst::{
    diag::{FileError, FileResult},
    foundations::Bytes,
    syntax::{FileId, Source},
};
use typst_as_lib::{conversions::IntoFileId as _, file_resolver::FileResolver};

/// A Typst file resolver that serves embedded images from their on-disk files,
/// reading each one only when the compiler asks for it.
///
/// The resolver retains paths rather than image buffers; after a read, ownership
/// of the bytes passes to Typst. Two references whose bytes are identical share
/// one content-hashed virtual path, so they collapse to a single entry.
pub(crate) struct ImageFileResolver {
    by_id: HashMap<FileId, PathBuf>,
}

impl ImageFileResolver {
    pub(crate) fn new(assets: &ImageMap) -> Self {
        let by_id = assets
            .images()
            .map(|image| {
                (
                    image.virtual_path.as_str().into_file_id(),
                    image.path.clone(),
                )
            })
            .collect();
        Self { by_id }
    }
}

impl FileResolver for ImageFileResolver {
    fn resolve_binary(&self, id: FileId) -> FileResult<Cow<'_, Bytes>> {
        let path = self
            .by_id
            .get(&id)
            .ok_or_else(|| FileError::NotFound(PathBuf::from(id.vpath().get_with_slash())))?;
        let bytes = std::fs::read(path).map_err(|error| FileError::from_io(error, path))?;
        Ok(Cow::Owned(Bytes::new(bytes)))
    }

    fn resolve_source(&self, id: FileId) -> FileResult<Cow<'_, Source>> {
        // Images are only ever resolved as binaries, never as source files.
        Err(FileError::NotFound(PathBuf::from(
            id.vpath().get_with_slash(),
        )))
    }
}
