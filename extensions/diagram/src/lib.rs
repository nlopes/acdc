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

mod asset;
mod tikz;
mod wavedrom_cli;

use asset::Format;
pub use tikz::tikz;

/// Directories used by a converter to resolve relative image references.
#[derive(Clone, Copy, Debug)]
pub struct OutputDirectories<'a> {
    linked: &'a Path,
    embedded: &'a Path,
}

impl<'a> OutputDirectories<'a> {
    /// Linked images resolve from the converted file. Embedded HTML images
    /// are read relative to the input document.
    #[must_use]
    pub const fn new(linked: &'a Path, embedded: &'a Path) -> Self {
        Self { linked, embedded }
    }
}

impl<'a, P: AsRef<Path> + ?Sized> From<&'a P> for OutputDirectories<'a> {
    fn from(dir: &'a P) -> Self {
        Self::new(dir.as_ref(), dir.as_ref())
    }
}

/// Register the `wavedrom` literal-block style for a trusted renderer and an
/// existing output directory (the directory containing the converted document).
/// The caller installs the returned style in its registry. Rendering requires
/// [`SafeMode::Unsafe`]. The body is passed verbatim to the executable.
///
/// # Errors
/// Returns an error if either path cannot be resolved or has the wrong file type.
pub fn wavedrom<'a>(
    tool: &Path,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockStyle> {
    let tool = executable(tool.canonicalize()?)?;
    style("WaveDrom", output_dir.into(), move |root, input| {
        render(&tool, root, input)
    })
}

fn executable(tool: PathBuf) -> io::Result<PathBuf> {
    if !tool.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("expected an executable file at {}", tool.display()),
        ));
    }
    Ok(tool)
}

fn style<F>(
    label: &'static str,
    output_dir: OutputDirectories<'_>,
    render: F,
) -> io::Result<BlockStyle>
where
    F: Fn(OutputDirectories<'_>, &RawBlockInput<'_>) -> Result<BlockBuilder, String>
        + Clone
        + Send
        + Sync
        + 'static,
{
    let directory = |path: &Path| {
        let root = path.canonicalize()?;
        if root.is_dir() {
            Ok(root)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("expected an output directory at {}", path.display()),
            ))
        }
    };
    let linked = directory(output_dir.linked)?;
    let embedded = (output_dir.embedded != output_dir.linked)
        .then(|| directory(output_dir.embedded))
        .transpose()?;
    Ok(BlockStyle::raw(
        label.to_ascii_lowercase(),
        [RawBlockContext::Literal],
        move || {
            let linked = linked.clone();
            let embedded = embedded.clone();
            let render = render.clone();
            move |input: RawBlockInput<'_>| {
                if input
                    .document_attributes
                    .get("safe-mode-name")
                    .and_then(DocumentAttributeValue::text)
                    != Some(SafeMode::Unsafe.name())
                {
                    return Err(format!(
                        "{label} requires unsafe mode to run the configured executable"
                    ));
                }
                render(
                    OutputDirectories::new(&linked, embedded.as_deref().unwrap_or(&linked)),
                    &input,
                )
                .map(Some)
            }
        },
    )
    .with_positional_attributes(["target", "format"]))
}

fn render(
    tool: &Path,
    root: OutputDirectories<'_>,
    input: &RawBlockInput<'_>,
) -> Result<BlockBuilder, String> {
    let attrs = input.document_attributes;
    let args = &input.metadata.attributes;
    if args.get("wavedrom").is_some() || attrs.get("wavedrom").is_some() {
        return Err("set the WaveDrom executable when registering the style".into());
    }
    let requested = args.get_string("format");
    let format = match requested.as_deref().or_else(|| {
        attrs
            .get("wavedrom-format")
            .and_then(DocumentAttributeValue::text)
    }) {
        Some("png") => Format::Png,
        None | Some("svg") => Format::Svg,
        Some(other) => {
            return Err(format!(
                "unsupported WaveDrom format '{other}' (expected svg or png)"
            ));
        }
    };
    image(
        "wavedrom",
        root,
        input,
        format,
        input.body,
        |scratch, output| wavedrom_cli::render(tool, input.body, format, scratch, output),
    )
}

fn image(
    name: &str,
    roots: OutputDirectories<'_>,
    input: &RawBlockInput<'_>,
    format: Format,
    source: &str,
    generate: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> Result<BlockBuilder, String> {
    let attrs = input.document_attributes;
    let args = &input.metadata.attributes;
    if attrs.get("imagesoutdir").is_some() {
        return Err(format!(
            "{name} writes relative to the caller's output directory; imagesoutdir is unsupported"
        ));
    }
    let target = args.get_string("target").unwrap_or_default();
    let filename = if target.is_empty() {
        let digest = Sha256::digest(source.as_bytes());
        format!(
            "{name}-{}.{}",
            URL_SAFE_NO_PAD.encode(digest),
            format.extension()
        )
    } else {
        if !target
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err(format!(
                "{name} target must use letters, digits, hyphens, or underscores"
            ));
        }
        format!("{target}.{}", format.extension())
    };
    let images = attrs
        .get("imagesdir")
        .and_then(DocumentAttributeValue::text)
        .unwrap_or_default();
    let embedded = attrs
        .get("backend")
        .and_then(DocumentAttributeValue::text)
        .is_some_and(|name| name.starts_with("html"))
        && (attrs.get("data-uri").is_some()
            || (format == Format::Svg
                && (&input.metadata.options)
                    .into_iter()
                    .any(|opt| *opt == "inline")));
    let root = if embedded {
        roots.embedded
    } else {
        roots.linked
    };
    let dir = image_directory(root, images).map_err(|e| format!("{name} imagesdir: {e}"))?;
    asset::publish(&dir, &filename, format, generate).map_err(|e| format!("{name}: {e}"))?;
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
