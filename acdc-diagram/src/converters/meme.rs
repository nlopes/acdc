//! Memes: caption text composited onto a background image with `ImageMagick`.
//!
//! Unlike every other diagram type this one has no block body — the picture
//! comes from the `background` attribute (which the `meme::` block macro fills
//! in from its target) and the captions from `top` and `bottom`.

use std::path::{Path, PathBuf};

use crate::{
    Format,
    cli::{self, CommandSpec},
    converters::{ConverterOptions, DiagramConverter, Generated, option, set_option},
    error::{Error, Result},
    source::{CommandLookup, DiagramSource},
};

/// `ImageMagick`.
pub(crate) struct Meme;

/// The two `ImageMagick` entry points, as this installation provides them.
///
/// `ImageMagick` 7 puts everything behind a single `magick` command; `ImageMagick`
/// 6 ships `convert` and `identify` separately.
struct Magick {
    convert: CommandSpec,
    identify: CommandSpec,
}

impl DiagramConverter for Meme {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Gif]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        let background = source
            .attr(&["background"])
            .ok_or_else(|| Error::config("a meme needs a `background` attribute"))?;
        let height = source
            .attr(&["height-fraction"])
            .unwrap_or_else(|| "20%".to_string());

        let mut options = ConverterOptions::new();
        options.insert("background".to_string(), background);
        set_option(&mut options, "top", source.attr(&["top"]));
        set_option(&mut options, "bottom", source.attr(&["bottom"]));
        set_option(
            &mut options,
            "fill-color",
            source.attr(&["fillcolor", "fill-color"]),
        );
        set_option(
            &mut options,
            "stroke-color",
            source.attr(&["strokecolor", "stroke-color"]),
        );
        set_option(
            &mut options,
            "stroke-width",
            source.attr(&["strokewidth", "stroke-width"]),
        );
        set_option(&mut options, "font", source.attr(&["font"]));
        options.insert(
            "top-height".to_string(),
            source
                .attr(&["top-height"])
                .unwrap_or_else(|| height.clone()),
        );
        options.insert(
            "bottom-height".to_string(),
            source.attr(&["bottom-height"]).unwrap_or(height),
        );
        if source.opt("noupcase") {
            options.insert("noupcase".to_string(), "true".to_string());
        }
        set_option(&mut options, "imagesdir", source.doc_attr("imagesdir"));
        Ok(options)
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let magick = find_magick(source)?;

        let background = option(options, "background")
            .ok_or_else(|| Error::config("a meme needs a `background` attribute"))?;
        let images_dir = option(options, "imagesdir").map(|dir| source.resolve_path(dir, None));
        let background = source.resolve_path(background, images_dir.as_deref());

        let (width, height) = identify(&magick, &background)?;

        let dir = tempfile::Builder::new()
            .prefix("acdc-meme-")
            .tempdir()
            .map_err(|error| Error::io("could not create a temporary directory", error))?;

        let fill = option(options, "fill-color").unwrap_or("white");
        let stroke = option(options, "stroke-color").unwrap_or("black");
        let stroke_width = option(options, "stroke-width").unwrap_or("2");
        let font = option(options, "font").unwrap_or("Impact");
        let upcase = option(options, "noupcase").is_none();

        let mut composites: Vec<(PathBuf, i64)> = Vec::new();
        for (name, gravity, height_option) in [
            ("top", "north", "top-height"),
            ("bottom", "south", "bottom-height"),
        ] {
            let Some(label) = option(options, name) else {
                continue;
            };
            let label_height =
                fraction_of(height, option(options, height_option).unwrap_or("20%"))?;
            let path = dir.path().join(format!("{name}.png"));
            cli::run(
                &magick.convert.clone().args([
                    "-background".to_string(),
                    "none".to_string(),
                    "-fill".to_string(),
                    fill.to_string(),
                    "-stroke".to_string(),
                    stroke.to_string(),
                    "-strokewidth".to_string(),
                    stroke_width.to_string(),
                    "-font".to_string(),
                    font.to_string(),
                    "-size".to_string(),
                    format!("{width}x{label_height}"),
                    "-gravity".to_string(),
                    gravity.to_string(),
                    format!("label:{}", prepare_label(label, upcase)),
                    crate::platform::native_path(&path),
                ]),
                None,
            )?;
            let offset = if name == "top" {
                0
            } else {
                height - label_height
            };
            composites.push((path, offset));
        }

        let final_image = dir.path().join(format!("meme.{format}"));
        let mut spec = magick
            .convert
            .clone()
            .arg(crate::platform::native_path(&background));
        for (path, offset) in &composites {
            spec = spec.args([
                crate::platform::native_path(path),
                "-geometry".to_string(),
                format!("+0+{offset}"),
                "-composite".to_string(),
            ]);
        }
        spec = spec.arg(crate::platform::native_path(&final_image));
        cli::run(&spec, None)?;

        std::fs::read(&final_image)
            .map(Generated::from)
            .map_err(|error| {
                Error::io(
                    format!("ImageMagick did not write {}", final_image.display()),
                    error,
                )
            })
    }
}

/// Resolve the `ImageMagick` entry points for this installation.
fn find_magick(source: &DiagramSource<'_>) -> Result<Magick> {
    if let Some(magick) = source.find_command_opt(&CommandLookup::new(&["magick"])) {
        return Ok(Magick {
            convert: CommandSpec::new(&magick).arg("convert"),
            identify: CommandSpec::new(&magick).arg("identify"),
        });
    }
    Ok(Magick {
        convert: CommandSpec::new(&source.find_command(&CommandLookup::new(&["convert"]))?),
        identify: CommandSpec::new(&source.find_command(&CommandLookup::new(&["identify"]))?),
    })
}

/// Read the background image's pixel dimensions.
fn identify(magick: &Magick, image: &Path) -> Result<(i64, i64)> {
    let output = cli::run(
        &magick.identify.clone().args([
            "-format".to_string(),
            "%w %h".to_string(),
            crate::platform::native_path(image),
        ]),
        None,
    )?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut parts = text.split_whitespace();
    let width = parts.next().and_then(|value| value.parse().ok());
    let height = parts.next().and_then(|value| value.parse().ok());
    match (width, height) {
        (Some(width), Some(height)) => Ok((width, height)),
        _ => Err(Error::Image(format!(
            "could not read the dimensions of {}",
            image.display()
        ))),
    }
}

/// Resolve a caption band height, given either as a percentage of the image or
/// as an absolute pixel count.
fn fraction_of(total: i64, value: &str) -> Result<i64> {
    if let Some(percent) = value.strip_suffix('%') {
        let percent: i64 = percent
            .parse()
            .map_err(|_| Error::config(format!("`{value}` is not a valid height")))?;
        return Ok(total * percent / 100);
    }
    let pixels = value.strip_suffix("px").unwrap_or(value);
    pixels
        .parse()
        .map_err(|_| Error::config(format!("`{value}` is not a valid height")))
}

/// Upper-case the caption unless asked not to, and turn ` // ` into a line break.
fn prepare_label(label: &str, upcase: bool) -> String {
    let label = if upcase {
        label.to_uppercase()
    } else {
        label.to_string()
    };
    label.replace(" // ", "\\n")
}
