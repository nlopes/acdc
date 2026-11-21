//! Caption a local image using an explicitly configured `ImageMagick` executable.
use super::{
    DiagramInput, Directories, OutputDirectories,
    asset::{self, Format},
    diagram_attribute, executable, image, recover, require_unsafe,
};
use acdc_parser::{
    BlockMetadata,
    extensions::{BlockBuilder, BlockMacro, MacroExecutionContext, MacroInvocation, MacroTarget},
};
use std::{
    borrow::Cow,
    io,
    path::{Path, PathBuf},
    process::Command,
};

/// Register `meme::background.jpg[top,bottom,target,format]` with `ImageMagick`.
///
/// # Errors
/// Returns an error if the program or configured output directories cannot be resolved.
pub fn meme<'a>(tool: &Path, output: impl Into<OutputDirectories<'a>>) -> io::Result<BlockMacro> {
    let tool = executable(std::path::absolute(tool)?)?;
    let dirs = Directories::resolve(output.into())?;
    Ok(BlockMacro::new("meme", move || {
        let tool = tool.clone();
        let dirs = dirs.clone();
        move |call: &MacroInvocation<'_>, context: &mut MacroExecutionContext| {
            let attrs = (&call.document_attributes).into();
            require_unsafe("meme", attrs)?;
            let mut metadata = BlockMetadata::new();
            metadata.attributes = call.attributes.clone();
            // Anonymous memes sharing a background can have different captions.
            metadata
                .attributes
                .insert_if_absent("target".into(), "".into());
            let input = DiagramInput {
                metadata: &metadata,
                source_file: None,
                body: call.source.as_ref(),
                document_attributes: attrs,
            };
            let result = render(&tool, dirs.roots(), call.target.as_ref(), &input).map(|block| {
                let attributes = call.named_arguments_source(|k| {
                    !matches!(
                        k,
                        "format" | "target" | "top" | "bottom" | "on-error" | "background"
                    ) && !CAPTION_ATTRIBUTES.contains(&k)
                });
                if attributes.is_empty() {
                    block
                } else {
                    block.with_attributes(attributes)
                }
            });
            recover(result, "meme", &input, |m| context.warn(m)).map(Into::into)
        }
    })
    .with_target(MacroTarget::Required)
    .with_positional_attributes(["top", "bottom", "target", "format"]))
}
const CAPTION_ATTRIBUTES: &[&str] = &[
    "font",
    "fillcolor",
    "fill-color",
    "strokecolor",
    "stroke-color",
    "strokewidth",
    "stroke-width",
    "top-height",
    "bottom-height",
    "height-fraction",
];
fn render(
    tool: &Path,
    roots: OutputDirectories<'_>,
    background: &str,
    input: &DiagramInput<'_>,
) -> Result<BlockBuilder, String> {
    for key in ["magick", "convert", "identify"] {
        if input.metadata.attributes.get(key).is_some()
            || input.document_attributes.get(key).is_some()
        {
            return Err("set the ImageMagick executable when registering the diagram".into());
        }
    }
    let format = match diagram_attribute(input, "meme", "format").as_deref() {
        None | Some("png") => Format::Png,
        Some("gif") => Format::Gif,
        Some(other) => {
            return Err(format!(
                "unsupported meme format '{other}' (expected png or gif)"
            ));
        }
    };
    let background_file = background_path(roots.embedded, background, input)?;
    let options: Vec<_> = ["top", "bottom"]
        .into_iter()
        .chain(CAPTION_ATTRIBUTES.iter().copied())
        .filter_map(|k| diagram_attribute(input, "meme", k).map(|v| (k, v)))
        .collect();
    let flags = input
        .metadata
        .attributes
        .get_string("opts")
        .or_else(|| input.metadata.attributes.get_string("options"));
    let noupcase = flags
        .as_deref()
        .is_some_and(|v| v.split(',').any(|s| s.trim() == "noupcase"));
    let identity = [
        background,
        "\0",
        if noupcase { "literal" } else { "uppercase" },
        "\0",
    ]
    .into_iter()
    .chain(
        options
            .iter()
            .flat_map(|(k, v)| [*k, "\0", v.as_ref(), "\0"]),
    );
    let resolved = DiagramInput {
        source_file: Some(&background_file),
        ..*input
    };
    image(
        "meme",
        roots,
        &resolved,
        format,
        identity,
        false,
        |scratch, output| {
            compose_captions(
                tool,
                roots.embedded,
                &background_file,
                scratch,
                output,
                &options,
                noupcase,
            )
        },
    )
}

fn background_path(
    root: &Path,
    background: &str,
    input: &DiagramInput<'_>,
) -> Result<PathBuf, String> {
    let relative = Path::new(background);
    if background.is_empty()
        || background.contains([':', '\\'])
        || relative.components().any(|c| {
            !matches!(
                c,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        return Err("meme background must be a local relative image path".to_owned());
    }
    let imagesdir = input
        .document_attributes
        .get("imagesdir")
        .and_then(acdc_parser::DocumentAttributeValue::text)
        .unwrap_or_default();
    let background_file = root
        .join(imagesdir)
        .join(relative)
        .canonicalize()
        .map_err(|e| format!("meme background '{background}': {e}"))?;
    if !background_file.starts_with(root) || !background_file.is_file() {
        return Err("meme background must be a local file in the source directory".into());
    }
    Ok(background_file)
}

fn dimensions(
    tool: &Path,
    background_file: &Path,
    dir: &Path,
    scratch: &Path,
) -> io::Result<(u32, u32)> {
    let dimensions = scratch.join("dimensions");
    asset::run_to_file(
        Command::new(tool)
            .arg(background_file)
            .args(["-format", "%w %h", "info:"]),
        dir,
        "ImageMagick",
        &dimensions,
    )?;
    let dimensions = asset::read_text(&dimensions)?;
    let (width, height) = dimensions
        .trim()
        .split_once(' ')
        .ok_or_else(|| io::Error::other("invalid background dimensions"))?;
    let width = width.parse::<u32>().map_err(io::Error::other)?;
    let height = height.parse::<u32>().map_err(io::Error::other)?;
    Ok((width, height))
}

fn compose_captions(
    tool: &Path,
    dir: &Path,
    background_file: &Path,
    scratch: &Path,
    output: &Path,
    options: &[(&str, Cow<'_, str>)],
    noupcase: bool,
) -> io::Result<()> {
    let get = |key: &str| {
        options
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.as_ref())
    };
    let (width, height) = dimensions(tool, background_file, dir, scratch)?;
    let mut compose = Command::new(tool);
    compose.arg(background_file);
    for (key, gravity, hkey) in [
        ("top", "north", "top-height"),
        ("bottom", "south", "bottom-height"),
    ] {
        let Some(caption) = get(key) else { continue };
        let cap_height = caption_height(
            get(hkey)
                .or_else(|| get("height-fraction"))
                .unwrap_or("20%"),
            height,
        )?;
        let label = if noupcase {
            caption.to_owned()
        } else {
            caption.to_uppercase()
        }
        .replace(" // ", "\n");
        let path = scratch.join(format!("{key}.png"));
        let mut cmd = Command::new(tool);
        cmd.args([
            "-background",
            "none",
            "-fill",
            get("fillcolor")
                .or_else(|| get("fill-color"))
                .unwrap_or("white"),
            "-stroke",
            get("strokecolor")
                .or_else(|| get("stroke-color"))
                .unwrap_or("black"),
            "-strokewidth",
            get("strokewidth")
                .or_else(|| get("stroke-width"))
                .unwrap_or("2"),
            "-font",
            get("font").unwrap_or("Impact"),
            "-size",
        ])
        .arg(format!("{width}x{cap_height}"))
        .args(["-gravity", gravity])
        .arg(format!("label:{label}"))
        .arg(&path);
        asset::run(&mut cmd, dir, "ImageMagick")?;
        let offset = if key == "top" {
            0
        } else {
            i64::from(height) - i64::from(cap_height)
        };
        compose
            .arg(path)
            .arg("-geometry")
            .arg(format!("+0{offset:+}"))
            .arg("-composite");
    }
    asset::run(compose.arg(output), dir, "ImageMagick")
}
fn caption_height(value: &str, height: u32) -> io::Result<u32> {
    if let Some(percent) = value.strip_suffix('%') {
        let p = percent.parse::<u32>().map_err(io::Error::other)?;
        u32::try_from(u64::from(height) * u64::from(p) / 100).map_err(io::Error::other)
    } else {
        value
            .strip_suffix("px")
            .unwrap_or(value)
            .parse()
            .map_err(io::Error::other)
    }
}
