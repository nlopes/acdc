//! PIC diagrams rendered by dpic in its safe SVG mode.

use std::{
    fs,
    io::{self, Write as _},
    path::Path,
    process::Command,
};

use acdc_parser::extensions::{BlockBuilder, BlockMacro, BlockStyle};

use super::{DiagramInput, OutputDirectories, asset, executable, file_macro, render_svg, style};

/// Register the `dpic` listing and literal block style with a caller-selected executable.
///
/// # Errors
/// Returns an error if the executable or an output directory cannot be resolved.
pub fn dpic<'a>(
    tool: &Path,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockStyle> {
    let tool = executable(std::path::absolute(tool)?)?;
    style("dpic", output_dir.into(), move |roots, input| {
        render(&tool, roots.embedded, roots, input)
    })
}

/// Register the `dpic::file.pic[]` macro using the parser's local source loader.
///
/// # Errors
/// Returns an error if the executable or an output directory cannot be resolved.
pub fn dpic_file<'a>(
    tool: &Path,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockMacro> {
    let tool = executable(std::path::absolute(tool)?)?;
    file_macro(
        "dpic",
        output_dir.into(),
        &["dpic"],
        move |dir, roots, input| render(&tool, dir, roots, input),
    )
}

fn render(
    tool: &Path,
    dir: &Path,
    roots: OutputDirectories<'_>,
    input: &DiagramInput<'_>,
) -> Result<BlockBuilder, String> {
    render_svg("dpic", roots, input, |source, scratch, output| {
        let path = scratch.join("input.pic");
        write_picture(&path, source)?;
        asset::run_to_file(
            Command::new(tool).args(["-v", "-z"]).arg(path),
            dir,
            "dpic",
            output,
        )
    })
}

fn write_picture(path: &Path, source: &str) -> io::Result<()> {
    let mut file = fs::File::create_new(path)?;
    let mut lines = source.lines().filter(|line| !line.trim().is_empty());
    let first = lines.next().unwrap_or_default();
    let is_directive = |line: &str, directive: &str| {
        line.strip_prefix(directive)
            .is_some_and(|tail| tail.is_empty() || tail.starts_with(char::is_whitespace))
    };
    if !is_directive(first, ".PS") {
        file.write_all(b".PS\n")?;
    }
    file.write_all(source.as_bytes())?;
    if !source.ends_with('\n') {
        file.write_all(b"\n")?;
    }
    if !is_directive(lines.next_back().unwrap_or(first), ".PE") {
        file.write_all(b".PE\n")?;
    }
    Ok(())
}
