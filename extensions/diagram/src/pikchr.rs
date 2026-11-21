//! Pikchr's file-to-SVG adapter from PR #451.

use std::{
    fs,
    io::{self, BufRead as _, BufReader, Seek as _, SeekFrom},
    path::Path,
    process::Command,
};

use acdc_parser::extensions::{BlockBuilder, BlockMacro, BlockStyle};

use super::{DiagramInput, OutputDirectories, asset, executable, file_macro, render_svg, style};

/// Register the `pikchr` listing and literal block style with a caller-selected executable.
///
/// # Errors
/// Returns an error if the executable or an output directory cannot be resolved.
pub fn pikchr<'a>(
    tool: &Path,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockStyle> {
    let tool = executable(std::path::absolute(tool)?)?;
    style("pikchr", output_dir.into(), move |roots, input| {
        render(&tool, roots.embedded, roots, input)
    })
}

/// Register `pikchr::file.pikchr[]` using the parser's local source loader.
///
/// # Errors
/// Returns an error if the executable or an output directory cannot be resolved.
pub fn pikchr_file<'a>(
    tool: &Path,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockMacro> {
    let tool = executable(std::path::absolute(tool)?)?;
    file_macro(
        "pikchr",
        output_dir.into(),
        &["pikchr"],
        move |dir, roots, input| render(&tool, dir, roots, input),
    )
}

fn render(
    tool: &Path,
    dir: &Path,
    roots: OutputDirectories<'_>,
    input: &DiagramInput<'_>,
) -> Result<BlockBuilder, String> {
    render_svg("pikchr", roots, input, &[], |source, scratch, output| {
        let path = scratch.join("input.pikchr");
        fs::write(&path, source)?;
        let result = asset::run_to_file(
            Command::new(tool).arg("--svg-only").arg(path),
            dir,
            "pikchr",
            output,
        );
        match (result, output_diagnostic(output)) {
            (Ok(()), Some(message)) => Err(io::Error::other(message)),
            (Err(err), Some(message)) => {
                Err(io::Error::new(err.kind(), format!("{err}: {message}")))
            }
            (result, _) => result,
        }
    })
}

// Pikchr writes parser diagnostics to stdout. Older CLIs return an HTML fragment
// with exit status zero. The shared validator handles all other non-SVG output.
fn output_diagnostic(path: &Path) -> Option<String> {
    const EXCERPT: u16 = 4096;
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let mut reader = BufReader::with_capacity(usize::from(EXCERPT), fs::File::open(path).ok()?);
    let prefix = reader.fill_buf().ok()?.trim_ascii_start();
    if prefix.starts_with(b"<") && !prefix.starts_with(b"<div") && !prefix.starts_with(b"<pre") {
        return None;
    }
    // A long source line can precede the caret and error message.
    if metadata.len() > u64::from(EXCERPT) {
        reader.seek(SeekFrom::End(-i64::from(EXCERPT))).ok()?;
    }
    let excerpt = String::from_utf8_lossy(reader.fill_buf().ok()?);
    if let Ok(doc) = roxmltree::Document::parse(excerpt.trim()) {
        let message = doc
            .descendants()
            .filter(roxmltree::Node::is_text)
            .filter_map(|node| node.text())
            .collect::<String>();
        let message = message.trim();
        return (!message.is_empty()).then(|| message.to_owned());
    }
    excerpt
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("ERROR:"))
        .map(str::to_owned)
}
