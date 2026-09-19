//! Turning one diagram block into a generated image or a literal block.
//!
//! This is the port of asciidoctor-diagram's `create_image_block` and
//! `create_literal_block`: pick the output format, consult the cache, run the
//! tool when the cache misses, measure and publish the image, and work out the
//! attributes the replacement node should carry.

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    Format, Options,
    cache::{self, ImageMetadata},
    converters::{self, DiagramConverter},
    error::{Error, Result},
    image, paths,
    source::{DiagramSource, Request},
};

/// What a diagram block should be replaced by.
#[derive(Debug, PartialEq)]
pub(crate) enum Rendered {
    /// An image block pointing at a generated file.
    Image {
        /// The value of the image's `target`, as the document should see it.
        target: String,
        /// Attributes for the image node.
        attributes: BTreeMap<String, String>,
        /// The `target` attribute the author wrote, if any, which feeds the
        /// alt-text fallback.
        explicit_target: Option<String>,
    },
    /// A literal block holding generated text.
    Text {
        /// The generated text.
        content: String,
        /// Attributes for the literal node.
        attributes: BTreeMap<String, String>,
    },
}

/// Render one diagram.
///
/// # Errors
///
/// Returns an error when the tool is missing or fails, when the requested
/// format is unsupported, or when the image cannot be written.
pub(crate) fn render(options: &Options, request: Request<'_>) -> Result<Rendered> {
    let name = request.name;
    let converter = converters::lookup(name)
        .ok_or_else(|| Error::config(format!("`{name}` is not a known diagram type")))?;

    let mut source = DiagramSource::new(request, options.unsafe_mode());

    // PlantUML rewrites the code here, which has to happen before the
    // checksum is taken so that an edited `!include` invalidates the cache.
    converter.prepare(&mut source)?;

    let format = choose_format(options, converter.as_ref(), &mut source)?;

    // acdc has no figure-caption attribute of its own; consume asciidoctor's
    // so it does not end up as an HTML attribute on the image.
    source.take_attribute("caption");

    if format.is_text() {
        return render_text(converter.as_ref(), &mut source, format);
    }
    render_image(options, converter.as_ref(), &mut source, format)
}

/// Resolve the output format, honouring the block, the document, and the
/// converter's own preference order.
fn choose_format(
    options: &Options,
    converter: &dyn DiagramConverter,
    source: &mut DiagramSource<'_>,
) -> Result<Format> {
    let mut supported = converter.supported_formats().to_vec();
    // A PDF is not something a browser will display inline, so for HTML it
    // becomes the last resort rather than a tool's declared favourite.
    if options.is_html_backend()
        && let Some(position) = supported.iter().position(|format| *format == Format::Pdf)
    {
        let pdf = supported.remove(position);
        supported.push(pdf);
    }

    let requested = source
        .take_attribute("format")
        .or_else(|| source.global_attr("format"));

    let format = match requested {
        Some(value) => Format::parse(&value)?,
        None => *supported
            .first()
            .ok_or_else(|| Error::config("this diagram type declares no output formats"))?,
    };

    if !supported.contains(&format) {
        return Err(Error::UnsupportedFormat {
            diagram: source.diagram_type().to_string(),
            format: format.to_string(),
            supported: Format::list(&supported),
        });
    }
    Ok(format)
}

/// Text formats are not cached: the tool runs, and its output becomes the
/// literal block's content directly.
fn render_text(
    converter: &dyn DiagramConverter,
    source: &mut DiagramSource<'_>,
    format: Format,
) -> Result<Rendered> {
    let tool_options = converter.collect_options(source)?;
    let generated = converter.convert(source, format, &tool_options)?;
    let content = String::from_utf8(generated.data).map_err(|_| {
        Error::Image(format!(
            "{} produced text output that is not valid UTF-8",
            source.diagram_type()
        ))
    })?;
    let mut attributes = source.take_attributes();
    attributes.remove("target");
    Ok(Rendered::Text {
        content,
        attributes,
    })
}

/// Generate, cache, measure and publish an image.
fn render_image(
    options: &Options,
    converter: &dyn DiagramConverter,
    source: &mut DiagramSource<'_>,
    format: Format,
) -> Result<Rendered> {
    let image_name = format!("{}.{format}", source.image_name());
    let image_dir = image_output_dir(options, source);
    let cache_dir = cache_dir(options, source);
    let image_file = image_dir.join(&image_name);
    let metadata_file = cache::metadata_path(&cache_dir, &image_name);

    let use_cache = !source.global_opt("nocache");
    let mut metadata = if use_cache {
        ImageMetadata::load(&metadata_file)
    } else {
        ImageMetadata::default()
    };

    // With `cache-images` the generated file lives in the cache directory and
    // the output tree gets a link, so clearing the output tree is cheap.
    let cached_image_file = if use_cache && source.global_opt("cache-images") {
        cache_dir.join(&image_name)
    } else {
        image_file.clone()
    };

    let tool_options = converter.collect_options(source)?;
    let regenerate = !cached_image_file.exists()
        || source.should_process(&cached_image_file, &metadata)
        || tool_options != metadata.options;

    if regenerate {
        tracing::debug!(diagram = source.diagram_type(), image = %image_name, "generating diagram");
        let generated = converter.convert(source, format, &tool_options)?;
        let processed =
            image::post_process(format, generated.data, !source.global_opt("nooptimise"))?;

        metadata = source.image_metadata();
        metadata.options = tool_options;
        metadata.width = processed.width;
        metadata.height = processed.height;

        cache::write_file(&cached_image_file, &processed.data)?;
        for (suffix, data) in generated.extra {
            let mut companion = cached_image_file.clone().into_os_string();
            companion.push(format!(".{suffix}"));
            cache::write_file(&PathBuf::from(companion), &data)?;
        }

        if use_cache {
            metadata.store(&metadata_file)?;
        } else {
            cache::remove_if_present(&metadata_file)?;
        }
    } else {
        tracing::debug!(diagram = source.diagram_type(), image = %image_name, "reusing cached diagram");
    }

    if cached_image_file != image_file && (!image_file.exists() || regenerate) {
        cache::link_or_copy(&cached_image_file, &image_file)?;
    }

    image_node(
        options,
        converter,
        source,
        format,
        &metadata,
        &ImagePaths { image_file },
    )
}

/// Where the generated image ended up.
struct ImagePaths {
    /// Its full path, which the document's reference is derived from.
    image_file: PathBuf,
}

/// Work out the attributes the replacement image node should carry.
fn image_node(
    options: &Options,
    converter: &dyn DiagramConverter,
    source: &mut DiagramSource<'_>,
    format: Format,
    metadata: &ImageMetadata,
    paths: &ImagePaths,
) -> Result<Rendered> {
    let explicit_target = source.attr(&["target"]);
    let svg_type = source.global_attr("svg-type");
    let mut attributes = source.take_attributes();
    attributes.remove("target");

    // A tool that does not scale for us has its output scaled by adjusting the
    // dimensions we declare instead.
    let scale_factor = if converter.native_scaling() {
        1.0
    } else {
        attributes
            .get("scale")
            .and_then(|value| leading_number(value))
            .unwrap_or(1.0)
    };

    if options.is_html_backend() {
        attributes.remove("scale");
        set_dimension(&mut attributes, "width", metadata.width, scale_factor);
        set_dimension(&mut attributes, "height", metadata.height, scale_factor);
    }

    // A bare number in `scaledwidth` means a percentage.
    if let Some(scaledwidth) = attributes.get_mut("scaledwidth")
        && scaledwidth.ends_with(|c: char| c.is_ascii_digit())
    {
        scaledwidth.push('%');
    }

    if format == Format::Svg {
        apply_svg_type(&mut attributes, svg_type.as_deref())?;
    }

    let target = image_reference(options, source, &paths.image_file);

    Ok(Rendered::Image {
        target,
        attributes,
        explicit_target,
    })
}

/// How the converted document should refer to the generated image.
///
/// The backends resolve a relative image target against `imagesdir`, so the
/// reference has to be relative to *that* directory rather than to the output
/// file: the backend puts the prefix back on. With the usual `imagesdir` and
/// `imagesoutdir` pointing at the same place this is just the file name, as
/// asciidoctor emits; when they differ it is whatever path bridges them, which
/// is what makes an `imagesoutdir` that the document does not otherwise know
/// about still resolve.
///
/// Both directories are absolute, so the reference does not depend on which
/// directory the command was run from — an image generated by
/// `acdc convert doc/guide.adoc` refers to itself the same way as one
/// generated from inside `doc/`.
fn image_reference(
    options: &Options,
    source: &DiagramSource<'_>,
    image_file: &std::path::Path,
) -> String {
    let reference_dir = source.doc_attr("imagesdir").map_or_else(
        || options.output_dir().to_path_buf(),
        |dir| paths::resolve(std::path::Path::new(&dir), options.output_dir()),
    );
    paths::relative_to(image_file, &reference_dir)
        .display()
        .to_string()
}

/// Record `svg-type` on the generated image.
///
/// asciidoctor uses `inline` and `interactive` to decide whether to embed the
/// SVG in the page. No acdc backend embeds images yet — `data-uri` is likewise
/// not honoured — so this only carries the author's intent onto the node for a
/// backend that later grows the ability.
fn apply_svg_type(attributes: &mut BTreeMap<String, String>, svg_type: Option<&str>) -> Result<()> {
    // A block-level `inline` or `interactive` option overrides the document's
    // `svg-type`, which is only a default.
    let requested = if attributes.contains_key("inline") {
        Some("inline")
    } else if attributes.contains_key("interactive") {
        Some("interactive")
    } else {
        svg_type
    };

    match requested {
        None | Some("static") => Ok(()),
        Some(kind @ ("inline" | "interactive")) => {
            attributes.insert(kind.to_string(), String::new());
            Ok(())
        }
        Some(other) => Err(Error::config(format!("unsupported SVG type `{other}`"))),
    }
}

/// Record a measured dimension unless the author set one explicitly.
fn set_dimension(
    attributes: &mut BTreeMap<String, String>,
    name: &str,
    measured: Option<f64>,
    scale: f64,
) {
    let Some(measured) = measured else { return };
    if attributes.contains_key(name) {
        return;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "image dimensions in pixels are far below i64::MAX"
    )]
    let scaled = (measured * scale) as i64;
    attributes.insert(name.to_string(), scaled.to_string());
}

/// Read the leading decimal number out of a `scale` attribute.
fn leading_number(value: &str) -> Option<f64> {
    let start = value.find(|c: char| c.is_ascii_digit())?;
    let rest = &value[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Where generated images are written.
///
/// `imagesoutdir` names it outright; otherwise images go where the document
/// expects to find them, which is `imagesdir` under the output directory.
fn image_output_dir(options: &Options, source: &DiagramSource<'_>) -> PathBuf {
    if let Some(dir) = source.doc_attr("imagesoutdir") {
        return paths::resolve(std::path::Path::new(&dir), options.base_dir());
    }
    match source.doc_attr("imagesdir") {
        Some(dir) => paths::resolve(std::path::Path::new(&dir), options.output_dir()),
        None => paths::normalize(options.output_dir()),
    }
}

/// Where cache sidecars are written.
///
/// The two branches resolve against different directories, matching
/// asciidoctor-diagram: an explicit `:diagram-cachedir:` is a path the author
/// wrote in the document, so it is relative to the document, while the default
/// follows the output so that a build tree stays self-contained.
fn cache_dir(options: &Options, source: &DiagramSource<'_>) -> PathBuf {
    match source.global_attr("cachedir") {
        Some(dir) => paths::resolve(std::path::Path::new(&dir), options.base_dir()),
        None => paths::resolve(
            std::path::Path::new(".asciidoctor/diagram"),
            options.output_dir(),
        ),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn reads_a_leading_scale_factor() {
        assert_eq!(leading_number("2"), Some(2.0));
        assert_eq!(leading_number("1.5x"), Some(1.5));
        assert_eq!(leading_number("scale 3"), Some(3.0));
        assert_eq!(leading_number("none"), None);
    }

    #[test]
    fn scales_measured_dimensions() {
        let mut attributes = BTreeMap::new();
        set_dimension(&mut attributes, "width", Some(100.0), 1.5);
        assert_eq!(attributes.get("width").map(String::as_str), Some("150"));
    }

    #[test]
    fn keeps_an_explicit_dimension() {
        let mut attributes = BTreeMap::from([("width".to_string(), "42".to_string())]);
        set_dimension(&mut attributes, "width", Some(100.0), 1.0);
        assert_eq!(attributes.get("width").map(String::as_str), Some("42"));
    }
}
