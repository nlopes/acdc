//! Render byte-field diagrams with `bytefield-svg`.

use std::{io, path::Path, process::Command};

use acdc_parser::extensions::{BlockBuilder, BlockMacro, BlockStyle};

use super::{DiagramInput, OutputDirectories, asset, executable, file_macro, render_svg, style};

/// Register the `bytefield` listing and literal block style with a caller-selected executable.
///
/// # Errors
/// Returns an error if the executable or an output directory cannot be resolved.
pub fn bytefield<'a>(
    tool: &Path,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockStyle> {
    let tool = executable(std::path::absolute(tool)?)?;
    style("bytefield", output_dir.into(), move |roots, input| {
        render(&tool, roots.embedded, roots, input)
    })
}

/// Register `bytefield::file.edn[]` using the parser's local source loader.
///
/// # Errors
/// Returns an error if the executable or an output directory cannot be resolved.
pub fn bytefield_file<'a>(
    tool: &Path,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockMacro> {
    let tool = executable(std::path::absolute(tool)?)?;
    file_macro(
        "bytefield",
        output_dir.into(),
        &["bytefield-svg"],
        move |dir, roots, input| render(&tool, dir, roots, input),
    )
}

fn render(
    tool: &Path,
    dir: &Path,
    roots: OutputDirectories<'_>,
    input: &DiagramInput<'_>,
) -> Result<BlockBuilder, String> {
    render_svg(
        "bytefield",
        "bytefield-svg",
        roots,
        input,
        &[],
        |source, scratch, output| {
            asset::run_with_source(
                Command::new(tool).arg("--output").arg(output),
                dir,
                "bytefield-svg",
                scratch,
                source,
                None,
            )
        },
    )
}
