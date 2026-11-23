//! Render `GoAT` ASCII diagrams as SVG.

use std::{io, path::Path, process::Command};

use acdc_parser::extensions::{BlockBuilder, BlockMacro, BlockStyle};

use super::{
    DiagramInput, OutputDirectories, asset, diagram_attribute, executable, file_macro, render_svg,
    style,
};

/// Register the `goat` listing and literal block style with a caller-selected executable.
///
/// # Errors
/// Returns an error if the executable or an output directory cannot be resolved.
pub fn goat<'a>(
    tool: &Path,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockStyle> {
    let tool = executable(std::path::absolute(tool)?)?;
    style("goat", output_dir.into(), move |roots, input| {
        render(&tool, roots.embedded, roots, input)
    })
}

/// Register `goat::file.txt[]` using the parser's local source loader.
///
/// # Errors
/// Returns an error if the executable or an output directory cannot be resolved.
pub fn goat_file<'a>(
    tool: &Path,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockMacro> {
    let tool = executable(std::path::absolute(tool)?)?;
    file_macro(
        "goat",
        output_dir.into(),
        &["goat", "svg-color-dark-scheme", "svg-color-light-scheme"],
        move |dir, roots, input| render(&tool, dir, roots, input),
    )
}

fn render(
    tool: &Path,
    dir: &Path,
    roots: OutputDirectories<'_>,
    input: &DiagramInput<'_>,
) -> Result<BlockBuilder, String> {
    let dark = diagram_attribute(input, "goat", "svg-color-dark-scheme");
    let light = diagram_attribute(input, "goat", "svg-color-light-scheme");
    let mut args = Vec::new();
    if let Some(color) = dark.as_deref() {
        args.extend(["-sds", color]);
    }
    if let Some(color) = light.as_deref() {
        args.extend(["-sls", color]);
    }
    render_svg(
        "goat",
        "goat",
        roots,
        input,
        &args,
        |source, scratch, output| {
            asset::run_with_source(
                Command::new(tool).args(&args),
                dir,
                "goat",
                scratch,
                source,
                Some(output),
            )
        },
    )
}
