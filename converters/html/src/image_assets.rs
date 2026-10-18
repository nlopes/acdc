//! Image embedding shared by block and inline HTML images.

use std::{
    io::{Read as _, Write as _},
    path::Path,
};

use acdc_converters_core::TraversalContext;
use acdc_parser::{Image, SafeMode};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use quick_xml::{
    Reader, Writer,
    events::{BytesStart, Event},
};

use crate::{
    Error, HtmlVariant, HtmlVisitor,
    image_helpers::{escape_image_text, write_dimension_attributes},
    inlines::escape_attribute,
};

// Apply the same bound to local files and decoded HTTP response bodies.
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

struct ImageData {
    mime: Option<String>,
    bytes: Vec<u8>,
}

impl<'a, W: std::io::Write> HtmlVisitor<'a, '_, W> {
    pub(crate) fn image_markup(
        &mut self,
        traversal: &TraversalContext<'a>,
        image: &Image<'_>,
        inline: bool,
    ) -> Result<String, Error> {
        let alt = acdc_converters_core::media::image_alt(image, inline);
        let target =
            acdc_converters_core::media::resolve_target(&image.source.to_string(), traversal);
        let svg = image.metadata.attributes.get_string("format").as_deref() == Some("svg")
            || target.contains(".svg");
        let embedding_allowed = self.processor.options.safe_mode() < SafeMode::Secure;
        if svg && embedding_allowed && image.metadata.options.contains(&"inline") {
            let result = self
                .read_image(&target, traversal)
                .and_then(|data| inline_svg(&data.bytes, image));
            return Ok(match result {
                Ok(svg) => svg,
                Err(error) => {
                    self.diagnostics
                        .warn(format!("could not embed SVG {target}: {error}"));
                    format!("<span class=\"alt\">{}</span>", escape_image_text(&alt))
                }
            });
        }

        let mut attrs = Vec::new();
        write_dimension_attributes(&mut attrs, &image.metadata)?;
        if inline
            && let Some(acdc_parser::AttributeValue::String(title)) =
                image.metadata.attributes.get("title")
        {
            write!(attrs, " title=\"{}\"", escape_image_text(title))?;
        }
        if self.processor.variant() == HtmlVariant::Semantic
            && let Some(loading) = image.metadata.attributes.get_string("loading")
        {
            write!(attrs, " loading=\"{}\"", escape_attribute(&loading))?;
        }
        let attrs = String::from_utf8(attrs)?;
        let source = escape_attribute(&self.image_uri(&target, traversal));
        let alt = escape_image_text(&alt);
        if svg && embedding_allowed && image.metadata.options.contains(&"interactive") {
            let fallback = if let Some(fallback) = image.metadata.attributes.get_string("fallback")
            {
                let fallback = acdc_converters_core::media::resolve_target(&fallback, traversal);
                let fallback = escape_attribute(&self.image_uri(&fallback, traversal));
                format!("<img src=\"{fallback}\" alt=\"{alt}\"{attrs}>")
            } else {
                format!("<span class=\"alt\">{alt}</span>")
            };
            Ok(format!(
                "<object type=\"image/svg+xml\" data=\"{source}\"{attrs}>{fallback}</object>"
            ))
        } else {
            Ok(format!("<img src=\"{source}\" alt=\"{alt}\"{attrs}>"))
        }
    }

    fn image_uri(&mut self, target: &str, traversal: &TraversalContext<'_>) -> String {
        let remote = target.starts_with("https://") || target.starts_with("http://");
        if self.processor.options.safe_mode() >= SafeMode::Secure
            || traversal.get("data-uri").is_none()
            || target.starts_with("data:")
            || (remote && traversal.get("allow-uri-read").is_none())
        {
            return target.to_owned();
        }
        let path = target.split(['?', '#']).next().unwrap_or(target);
        let extension = Path::new(path).extension().and_then(|value| value.to_str());
        let mut mime = match extension {
            Some("svg") => "image/svg+xml".to_owned(),
            Some(extension) => format!("image/{extension}"),
            None => "application/octet-stream".to_owned(),
        };
        let bytes = match self.read_image(target, traversal) {
            Ok(data) => {
                if let Some(content_type) = data.mime {
                    mime = content_type;
                }
                data.bytes
            }
            Err(error) => {
                self.diagnostics
                    .warn(format!("could not embed image {target}: {error}"));
                if remote {
                    return target.to_owned();
                }
                Vec::new()
            }
        };
        format!("data:{mime};base64,{}", STANDARD.encode(bytes))
    }

    fn read_image(
        &self,
        target: &str,
        traversal: &TraversalContext<'_>,
    ) -> Result<ImageData, String> {
        if target.starts_with("https://") || target.starts_with("http://") {
            if traversal.get("allow-uri-read").is_none() {
                return Err("remote image reads require allow-uri-read".into());
            }
            return read_remote_image(target);
        }
        let base = self
            .render_options
            .source_dir
            .as_deref()
            .unwrap_or_else(|| Path::new("."));
        let path = if target.starts_with("file:") {
            url::Url::parse(target)
                .map_err(|error| error.to_string())?
                .to_file_path()
                .map_err(|()| "not a local file URL")?
        } else if target.contains("://") || target.starts_with("//") || target.starts_with("data:")
        {
            return Err("unsupported image source".into());
        } else {
            let decoded = percent_encoding::percent_decode_str(target)
                .decode_utf8()
                .map_err(|error| error.to_string())?;
            base.join(decoded.as_ref())
        };
        let path = path.canonicalize().map_err(|error| error.to_string())?;
        if self.processor.options.safe_mode() >= SafeMode::Safe {
            let base = base.canonicalize().map_err(|error| error.to_string())?;
            if !path.starts_with(base) {
                return Err("image is outside the source document directory".into());
            }
        }
        if !path
            .metadata()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("image source is not a regular file".into());
        }
        let bytes = read_capped(std::fs::File::open(path).map_err(|error| error.to_string())?)?;
        Ok(ImageData { mime: None, bytes })
    }
}

fn read_capped(reader: impl std::io::Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("image exceeds the 20 MiB embedding limit".into());
    }
    Ok(bytes)
}

#[cfg(feature = "network")]
fn read_remote_image(target: &str) -> Result<ImageData, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build();
    let mut response = ureq::Agent::new_with_config(config)
        .get(target)
        .call()
        .map_err(|error| error.to_string())?;
    let mime = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .unwrap_or("application/octet-stream")
        .trim()
        .to_owned();
    let bytes = read_capped(response.body_mut().as_reader())?;
    Ok(ImageData {
        mime: Some(mime),
        bytes,
    })
}

#[cfg(not(feature = "network"))]
fn read_remote_image(_target: &str) -> Result<ImageData, String> {
    Err("remote image embedding requires the network feature".into())
}

fn inline_svg(bytes: &[u8], image: &Image<'_>) -> Result<String, String> {
    let source = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let mut reader = Reader::from_str(source);
    loop {
        let start = usize::try_from(reader.buffer_position()).map_err(|error| error.to_string())?;
        let event = reader.read_event().map_err(|error| error.to_string())?;
        let (tag, empty) = match event {
            Event::Start(tag) => (tag, false),
            Event::Empty(tag) => (tag, true),
            Event::Eof => return Err("missing SVG root element".into()),
            Event::End(_)
            | Event::Text(_)
            | Event::CData(_)
            | Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_)
            | Event::GeneralRef(_) => continue,
        };
        if tag.name().as_ref() != b"svg" {
            return Err("image root element is not svg".into());
        }
        let width = image.metadata.attributes.get_string("width");
        let height = image.metadata.attributes.get_string("height");
        if width.is_none() && height.is_none() {
            return Ok(source
                .get(start..)
                .unwrap_or_default()
                .trim_end()
                .to_owned());
        }
        // Either supplied dimension replaces both intrinsic dimensions and style.
        let mut replacement = BytesStart::new("svg");
        for attribute in tag.attributes() {
            let attribute = attribute.map_err(|error| error.to_string())?;
            if !matches!(attribute.key.as_ref(), b"width" | b"height" | b"style") {
                replacement.push_attribute(attribute);
            }
        }
        for (name, value) in [("width", width), ("height", height)] {
            if let Some(value) = value {
                replacement.push_attribute((name, value.as_ref()));
            }
        }
        let mut writer = Writer::new(Vec::new());
        writer
            .write_event(if empty {
                Event::Empty(replacement)
            } else {
                Event::Start(replacement)
            })
            .map_err(|error| error.to_string())?;
        let mut output =
            String::from_utf8(writer.into_inner()).map_err(|error| error.to_string())?;
        let end = usize::try_from(reader.buffer_position()).map_err(|error| error.to_string())?;
        output.push_str(source.get(end..).unwrap_or_default().trim_end());
        return Ok(output);
    }
}
