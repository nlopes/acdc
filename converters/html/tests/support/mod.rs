use std::path::{Path, PathBuf};

use acdc_parser::ast::DocumentRef;

pub(crate) fn has_highlighter(document: DocumentRef<'_>) -> bool {
    document.attributes().get("source-highlighter").is_some()
}

pub(crate) fn expected_fixture_path(
    directory: &Path,
    stem: &str,
    document: DocumentRef<'_>,
) -> PathBuf {
    let extension = if !cfg!(feature = "highlighting") && has_highlighter(document) {
        "no-highlighting.html"
    } else {
        "html"
    };
    directory.join(stem).with_extension(extension)
}
