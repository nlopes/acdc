//! Render explicitly registered diagram styles into ordinary acdc image blocks.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use acdc_parser::{
    DocumentAttributeValue, SafeMode,
    extensions::{BlockBuilder, BlockStyle, RawBlockContext, RawBlockInput},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

mod wavedrom_cli;

/// Register the `wavedrom` literal-block style for a trusted renderer and an
/// existing output directory (the directory containing the converted document).
/// The caller installs the returned style in its registry. Rendering requires
/// [`SafeMode::Unsafe`]. The body is passed verbatim to the executable.
///
/// # Errors
/// Returns an error if either path cannot be resolved or has the wrong file type.
pub fn wavedrom(tool: &Path, output_dir: &Path) -> io::Result<BlockStyle> {
    let tool = tool.canonicalize()?;
    let root = output_dir.canonicalize()?;
    if !tool.is_file() || !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a WaveDrom executable and an output directory",
        ));
    }
    Ok(
        BlockStyle::raw("wavedrom", [RawBlockContext::Literal], move || {
            let tool = tool.clone();
            let root = root.clone();
            move |input: RawBlockInput<'_>| render(&tool, &root, &input).map(Some)
        })
        .with_positional_attributes(["target", "format"]),
    )
}

fn render(tool: &Path, root: &Path, input: &RawBlockInput<'_>) -> Result<BlockBuilder, String> {
    let attrs = input.document_attributes;
    if attrs
        .get("safe-mode-name")
        .and_then(DocumentAttributeValue::text)
        != Some(SafeMode::Unsafe.name())
    {
        return Err("WaveDrom requires unsafe mode to run the configured executable".into());
    }
    let args = &input.metadata.attributes;
    if args.get("wavedrom").is_some() || attrs.get("wavedrom").is_some() {
        return Err("set the WaveDrom executable when registering the style".into());
    }
    if let Some(format) = args.get_string("format")
        && format != "svg"
    {
        return Err(format!(
            "unsupported WaveDrom format '{format}' (expected svg)"
        ));
    }
    if attrs.get("imagesoutdir").is_some() {
        return Err("WaveDrom writes relative to the caller's output directory; imagesoutdir is unsupported".into());
    }
    let target = args.get_string("target").unwrap_or_default();
    let filename = if target.is_empty() {
        let digest = Sha256::digest(input.body.as_bytes());
        format!("wavedrom-{}.svg", URL_SAFE_NO_PAD.encode(digest))
    } else {
        if !target
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err("WaveDrom target must use letters, digits, hyphens, or underscores".into());
        }
        format!("{target}.svg")
    };
    let images = attrs
        .get("imagesdir")
        .and_then(DocumentAttributeValue::text)
        .unwrap_or_default();
    let dir = image_directory(root, images).map_err(|e| format!("WaveDrom imagesdir: {e}"))?;
    wavedrom_cli::render(tool, input.body, &dir, &filename)
        .map_err(|e| format!("WaveDrom: {e}"))?;
    Ok(BlockBuilder::image(filename).with_default_attributes("alt=Diagram"))
}

// The HTML image reference already includes the source-position imagesdir.
// Keep the builder target relative to it and place the asset beneath the output root.
fn image_directory(root: &Path, images: &str) -> io::Result<PathBuf> {
    let mut dir = root.to_owned();
    if images.contains([':', '\\', '#', '?', '%']) {
        return Err(io::Error::other("expected a local relative directory"));
    }
    let path = Path::new(images);
    for component in path.components() {
        match component {
            Component::CurDir => continue,
            Component::Normal(name) => dir.push(name),
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(io::Error::other("expected a local relative directory"));
            }
        }
        match fs::create_dir(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(&dir)?.file_type().is_dir() {
                    return Err(io::Error::other(
                        "imagesdir must contain only real directories",
                    ));
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(dir)
}
