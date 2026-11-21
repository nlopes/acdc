//! Render explicitly registered diagram styles into ordinary acdc image blocks.

use std::{
    borrow::Cow,
    fs, io,
    path::{Component, Path, PathBuf},
};

use acdc_parser::{
    DocumentAttributeValue, SafeMode, Substitution, VERBATIM,
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
    imagesoutdir_base: &'a Path,
}

impl<'a> OutputDirectories<'a> {
    /// Linked images resolve from the converted file. Embedded HTML images
    /// are read relative to the input document.
    #[must_use]
    pub const fn new(linked: &'a Path, embedded: &'a Path) -> Self {
        Self {
            linked,
            embedded,
            imagesoutdir_base: linked,
        }
    }

    /// Resolve a document's relative `imagesoutdir` under this existing directory.
    /// By default, it resolves under the linked output directory.
    #[must_use]
    pub const fn with_imagesoutdir_base(mut self, base: &'a Path) -> Self {
        self.imagesoutdir_base = base;
        self
    }
}

impl<'a, P: AsRef<Path> + ?Sized> From<&'a P> for OutputDirectories<'a> {
    fn from(dir: &'a P) -> Self {
        Self::new(dir.as_ref(), dir.as_ref())
    }
}

/// Register the `wavedrom` style for listing and literal blocks with a trusted renderer and an
/// existing output directory (the directory containing the converted document).
/// The caller installs the returned style in its registry. Rendering requires
/// [`SafeMode::Unsafe`]. The body is verbatim unless its `subs` setting enables attributes.
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
    let alternate_root = |path: &Path| {
        (path != output_dir.linked)
            .then(|| directory(path))
            .transpose()
    };
    let embedded = alternate_root(output_dir.embedded)?;
    let staging = alternate_root(output_dir.imagesoutdir_base)?;
    Ok(BlockStyle::raw(
        label.to_ascii_lowercase(),
        [RawBlockContext::Listing, RawBlockContext::Literal],
        move || {
            let linked = linked.clone();
            let embedded = embedded.clone();
            let staging = staging.clone();
            let render = render.clone();
            move |mut input: RawBlockInput<'_>| {
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
                let roots = OutputDirectories::new(&linked, embedded.as_deref().unwrap_or(&linked))
                    .with_imagesoutdir_base(staging.as_deref().unwrap_or(&linked));
                match render(roots, &input) {
                    Ok(image) => Ok(Some(image)),
                    Err(error) if abort_on_error(&input, label) => Err(error),
                    Err(error) => {
                        let message = format!("Failed to generate image: {error}");
                        // The renderer already received its requested substitutions.
                        // Display that source with ordinary listing escaping.
                        let listing =
                            BlockBuilder::listing(format!("{message}\n{}", diagram_source(&input)))
                                .without_attribute("subs");
                        input.warn(message);
                        Ok(Some(listing))
                    }
                }
            }
        },
    )
    .with_positional_attributes(["target", "format"]))
}

fn abort_on_error(input: &RawBlockInput<'_>, label: &str) -> bool {
    let attrs = input.document_attributes;
    input
        .metadata
        .attributes
        .get_string("on-error")
        .as_deref()
        .or_else(|| {
            attrs
                .get(&format!("{}-on-error", label.to_ascii_lowercase()))
                .and_then(DocumentAttributeValue::text)
        })
        .or_else(|| {
            attrs
                .get("diagram-on-error")
                .and_then(DocumentAttributeValue::text)
        })
        == Some("abort")
}

fn diagram_source<'a>(input: &RawBlockInput<'a>) -> Cow<'a, str> {
    if input
        .metadata
        .uses_substitution(&Substitution::Attributes, VERBATIM)
    {
        let attrs = input.document_attributes;
        attrs.substitute_attributes(input.body, |name| attrs.get(name))
    } else {
        Cow::Borrowed(input.body)
    }
}

fn diagram_attribute<'a>(
    input: &RawBlockInput<'a>,
    renderer: &str,
    key: &str,
) -> Option<Cow<'a, str>> {
    let attrs = input.document_attributes;
    input.metadata.attributes.get_string(key).or_else(|| {
        attrs
            .get(&format!("{renderer}-{key}"))
            .or_else(|| attrs.get(&format!("diagram-{key}")))
            .map(|value| Cow::Borrowed(value.text().unwrap_or_default()))
    })
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
    let format = match diagram_attribute(input, "wavedrom", "format").as_deref() {
        Some("png") => Format::Png,
        None | Some("svg") => Format::Svg,
        Some(other) => {
            return Err(format!(
                "unsupported WaveDrom format '{other}' (expected svg or png)"
            ));
        }
    };
    let source = diagram_source(input);
    image(
        "wavedrom",
        root,
        input,
        format,
        &source,
        |scratch, output| wavedrom_cli::render(tool, &source, format, scratch, output),
    )
}

fn svg_option(
    renderer: &str,
    input: &RawBlockInput<'_>,
    format: Format,
) -> Result<Option<&'static str>, String> {
    if format != Format::Svg {
        return Ok(None);
    }
    for option in ["inline", "interactive"] {
        if (&input.metadata.options)
            .into_iter()
            .any(|opt| *opt == option)
        {
            return Ok(Some(option));
        }
    }
    match diagram_attribute(input, renderer, "svg-type").as_deref() {
        None | Some("static") => Ok(None),
        Some("inline") => Ok(Some("inline")),
        Some("interactive") => Ok(Some("interactive")),
        Some(other) => Err(format!(
            "{renderer} unsupported SVG type '{other}' (expected static, inline or interactive)"
        )),
    }
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
    let target = args.get_string("target").unwrap_or_default();
    let image_target = if target.is_empty() {
        let digest = Sha256::digest(source.as_bytes());
        format!(
            "{name}-{}.{}",
            URL_SAFE_NO_PAD.encode(digest),
            format.extension()
        )
    } else {
        if !target.split('/').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        }) {
            return Err(format!(
                "{name} target must be a relative path with nonempty segments containing only ASCII letters, digits, hyphens, or underscores"
            ));
        }
        format!("{target}.{}", format.extension())
    };
    let backend = attrs
        .get("backend")
        .and_then(DocumentAttributeValue::text)
        .unwrap_or_default();
    let option = svg_option(name, input, format)?;
    let embedded = backend.starts_with("html")
        && (attrs.get("data-uri").is_some() || option == Some("inline"));
    let staging = attrs.get("imagesoutdir");
    let (root, attr_name, relative) = if let Some(value) = staging {
        (roots.imagesoutdir_base, "imagesoutdir", value.text())
    } else {
        let root = if embedded {
            roots.embedded
        } else {
            roots.linked
        };
        (
            root,
            "imagesdir",
            attrs
                .get("imagesdir")
                .and_then(DocumentAttributeValue::text),
        )
    };
    let dir = image_directory(root.to_owned(), relative.unwrap_or_default())
        .map_err(|e| format!("{name} {attr_name}: {e}"))?;
    let (parent, filename) = image_target.rsplit_once('/').unwrap_or(("", &image_target));
    let dir = image_directory(dir, parent).map_err(|e| format!("{name} target: {e}"))?;
    let physical_target = if staging.is_some() && (embedded || backend == "pdf") {
        Some(staged_file_url(&dir.join(filename))?)
    } else if staging.is_some() && backend == "terminal" {
        Some(
            dir.join(filename)
                .into_os_string()
                .into_string()
                .map_err(|_| format!("{name} terminal image path must be UTF-8"))?,
        )
    } else {
        None
    };
    let size =
        asset::publish(&dir, filename, format, generate).map_err(|e| format!("{name}: {e}"))?;
    let mut image = BlockBuilder::image(physical_target.unwrap_or(image_target));
    if let Some(option) = option
        && !(&input.metadata.options)
            .into_iter()
            .any(|opt| *opt == option)
    {
        image = image.with_attributes(format!("opts={option}"));
    }
    let mut defaults = Vec::new();
    if target.is_empty() {
        defaults.push("alt=Diagram".to_owned());
    }
    if backend.starts_with("html")
        && let Some(size) = size
    {
        let scale = args
            .get_string("scale")
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|scale| scale.is_finite() && *scale >= 0.0)
            .unwrap_or(1.0);
        let width = (size.width * scale).trunc();
        let height = (size.height * scale).trunc();
        if width.is_finite() && height.is_finite() {
            defaults.push(format!("width={width},height={height}"));
        }
    }
    Ok(if defaults.is_empty() {
        image
    } else {
        image.with_default_attributes(defaults.join(","))
    })
}

#[cfg(any(
    unix,
    windows,
    target_os = "redox",
    target_os = "wasi",
    target_os = "hermit"
))]
fn staged_file_url(path: &Path) -> Result<String, String> {
    url::Url::from_file_path(path)
        .map(Into::into)
        .map_err(|()| format!("cannot construct a file URL for {}", path.display()))
}

#[cfg(not(any(
    unix,
    windows,
    target_os = "redox",
    target_os = "wasi",
    target_os = "hermit"
)))]
fn staged_file_url(_: &Path) -> Result<String, String> {
    Err("local file URLs are not supported on this platform".into())
}

fn image_directory(mut dir: PathBuf, relative: &str) -> io::Result<PathBuf> {
    if relative.contains([':', '\\', '#', '?', '%']) {
        return Err(io::Error::other("expected a local relative directory"));
    }
    let path = Path::new(relative);
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
                    return Err(io::Error::other("path must contain only real directories"));
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(dir)
}
