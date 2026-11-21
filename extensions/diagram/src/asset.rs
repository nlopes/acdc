use std::{
    fs,
    io::{self, BufReader, Read as _, Seek as _},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use svgtypes::{Length, LengthUnit, ViewBox};

pub(super) struct ImageSize {
    pub(super) width: f64,
    pub(super) height: f64,
}

impl ImageSize {
    fn from_svg_root(root: roxmltree::Node<'_, '_>) -> Option<Self> {
        let pixels = |name| {
            let length: Length = root.attribute(name)?.trim().parse().ok()?;
            let factor = match length.unit {
                LengthUnit::None | LengthUnit::Px => 1.0,
                // Match Asciidoctor Diagram's conversion of pdf2svg's TeX points.
                LengthUnit::Pt => 1.33,
                LengthUnit::In => 96.0,
                LengthUnit::Cm => 96.0 / 2.54,
                LengthUnit::Mm => 96.0 / 25.4,
                LengthUnit::Pc => 16.0,
                LengthUnit::Em | LengthUnit::Ex | LengthUnit::Percent => return None,
            };
            let value = length.number * factor;
            (value.is_finite() && value >= 0.0).then_some(value)
        };
        if let (Some(width), Some(height)) = (pixels("width"), pixels("height")) {
            return Some(Self { width, height });
        }
        let view_box: ViewBox = root.attribute("viewBox")?.parse().ok()?;
        Some(Self {
            width: view_box.w,
            height: view_box.h,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Format {
    Svg,
    Png,
    Pdf,
}

impl Format {
    pub(super) fn extension(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
            Self::Pdf => "pdf",
        }
    }

    pub(super) fn validate(self, path: &Path) -> io::Result<Option<ImageSize>> {
        const MAX_BYTES: u64 = 16 * 1024 * 1024;
        let meta = fs::symlink_metadata(path)?;
        if !meta.is_file() || meta.len() > MAX_BYTES {
            return Err(io::Error::other(
                "expected a regular diagram file of at most 16 MiB",
            ));
        }
        let mut file = fs::File::open(path)?;
        match self {
            Self::Svg => {
                let mut svg = String::new();
                file.take(MAX_BYTES + 1).read_to_string(&mut svg)?;
                if svg.len() as u64 > MAX_BYTES {
                    return Err(io::Error::other("invalid SVG output"));
                }
                let document = roxmltree::Document::parse_with_options(
                    &svg,
                    roxmltree::ParsingOptions {
                        allow_dtd: declares_svg_11(&svg),
                        ..Default::default()
                    },
                )
                .map_err(io::Error::other)?;
                let root = document.root_element();
                if !root.has_tag_name(("http://www.w3.org/2000/svg", "svg")) {
                    return Err(io::Error::other("invalid SVG output"));
                }
                Ok(ImageSize::from_svg_root(root))
            }
            Self::Png => validate_png(file).map(Some),
            Self::Pdf => {
                let mut signature = [0; 5];
                file.read_exact(&mut signature)?;
                file.seek(io::SeekFrom::Start(meta.len().saturating_sub(1024)))?;
                let mut tail = Vec::new();
                file.take(1024).read_to_end(&mut tail)?;
                if &signature != b"%PDF-" || !tail.trim_ascii_end().ends_with(b"%%EOF") {
                    return Err(io::Error::other("invalid PDF output"));
                }
                Ok(None)
            }
        }
    }
}

// Graphviz emits this external SVG 1.1 declaration. Only enable DTD parsing for
// that exact prolog (without an internal subset), with no external entity resolver.
fn declares_svg_11(svg: &str) -> bool {
    let svg = svg.trim_start_matches('\u{feff}').trim_start();
    let prolog = svg
        .strip_prefix("<?xml ")
        .and_then(|s| s.split_once("?>"))
        .map_or(svg, |(_, rest)| rest.trim_start());
    prolog.strip_prefix("<!DOCTYPE ")
        .and_then(|decl| decl.split_once('>'))
        .is_some_and(|(decl, _)| decl.split_ascii_whitespace().eq(
            r#"svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd""#
                .split_ascii_whitespace()
        ))
}

// Check the raster and its end marker before publishing, without storing a full frame.
fn validate_png(file: fs::File) -> io::Result<ImageSize> {
    let invalid = |e: png::DecodingError| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid PNG output: {e}"),
        )
    };
    let mut reader = png::Decoder::new(BufReader::new(file))
        .read_info()
        .map_err(invalid)?;
    if reader
        .output_buffer_size()
        .is_none_or(|bytes| bytes > png::Limits::default().bytes)
    {
        return Err(io::Error::other(
            "PNG raster exceeds the decoder size limit",
        ));
    }
    while reader.next_row().map_err(invalid)?.is_some() {}
    reader.finish().map_err(invalid)?;
    let (width, height) = reader.info().size();
    Ok(ImageSize {
        width: f64::from(width),
        height: f64::from(height),
    })
}

pub(super) fn publish(
    dir: &Path,
    filename: &str,
    format: Format,
    generate: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<Option<ImageSize>> {
    let mut scratch = tempfile::Builder::new();
    scratch.prefix(".acdc-diagram-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        scratch.permissions(fs::Permissions::from_mode(0o700));
    }
    let scratch = scratch.tempdir_in(dir)?;
    let output = scratch
        .path()
        .join(format!("output.{}", format.extension()));
    generate(scratch.path(), &output)?;
    let size = format.validate(&output)?;
    fs::rename(output, dir.join(filename))?;
    Ok(size)
}

pub(super) fn run(command: &mut Command, dir: &Path, tool: &str) -> io::Result<()> {
    run_with_input(command, dir, tool, Stdio::null())
}

pub(super) fn run_with_input(
    command: &mut Command,
    dir: &Path,
    tool: &str,
    stdin: Stdio,
) -> io::Result<()> {
    let mut child = command
        .current_dir(dir)
        .stdin(stdin)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let error = loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(io::Error::other(format!("{tool} exited with {status}")));
            }
            Err(error) => break error,
            Ok(None) => {}
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break io::Error::new(
                io::ErrorKind::TimedOut,
                format!("{tool} exceeded 30 seconds"),
            );
        }
        std::thread::sleep(remaining.min(Duration::from_millis(10)));
    };
    let _ = child.kill();
    child.wait()?;
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_11_declaration_with_internal_subset_is_rejected() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("graph.svg");
        fs::write(
            &path,
            r#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd" []><svg xmlns="http://www.w3.org/2000/svg"/>"#,
        )?;
        assert!(Format::Svg.validate(&path).is_err());
        Ok(())
    }
}
