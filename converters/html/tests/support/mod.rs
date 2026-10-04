use std::path::{Path, PathBuf};

use acdc_parser::Document;

pub(crate) fn skip_fixture(stem: &str) -> bool {
    !cfg!(feature = "pre-spec-subs") && stem.contains("subs")
}

pub(crate) fn has_highlighter(document: &Document<'_>) -> bool {
    document.attributes.get("source-highlighter").is_some()
}

pub(crate) fn expected_fixture_path(
    directory: &Path,
    stem: &str,
    document: &Document<'_>,
) -> PathBuf {
    let extension = if !cfg!(feature = "highlighting") && has_highlighter(document) {
        "no-highlighting.html"
    } else {
        "html"
    };
    directory.join(stem).with_extension(extension)
}
