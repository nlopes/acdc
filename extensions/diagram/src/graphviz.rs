//! Graphviz's stdin-to-file adapter, ported from acdc-diagram (PR #451).

use std::{io, path::Path, process::Command};

use acdc_parser::{
    DocumentAttributeValue,
    extensions::{BlockBuilder, BlockMacro, BlockStyle},
};

use super::{
    DiagramInput, OutputDirectories,
    asset::{self, Format},
    diagram_attribute, diagram_source, executable, file_macro, image, style,
};

/// Register the `graphviz` style with a trusted Graphviz `dot` executable.
/// Relative graph resources resolve from the caller's source image directory.
///
/// # Errors
/// Returns an error if the executable or a configured directory cannot be resolved.
pub fn graphviz<'a>(
    dot: &Path,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockStyle> {
    // Graphviz also selects a layout from argv[0] on multi-call installations.
    let dot = executable(std::path::absolute(dot)?)?;
    style("Graphviz", output_dir.into(), move |roots, input| {
        render(&dot, roots.embedded, roots, input)
    })
}

/// Register the `graphviz::file.dot[format]` macro with a trusted Graphviz executable.
/// File targets use the parser's include loader and byte budget. Relative graph
/// resources resolve from the directory containing the DOT file.
///
/// # Errors
/// Returns an error if the executable or an output directory cannot be resolved.
pub fn graphviz_file<'a>(
    dot: &Path,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockMacro> {
    let dot = executable(std::path::absolute(dot)?)?;
    file_macro(
        "Graphviz",
        output_dir.into(),
        &["layout", "dot", "graphvizdot"],
        move |dir, roots, input| render(&dot, dir, roots, input),
    )
}

fn render(
    dot: &Path,
    working_dir: &Path,
    roots: OutputDirectories<'_>,
    input: &DiagramInput<'_>,
) -> Result<BlockBuilder, String> {
    let args = &input.metadata.attributes;
    let attrs = input.document_attributes;
    for key in ["dot", "graphvizdot"] {
        if args.get(key).is_some() || attrs.get(key).is_some() {
            return Err("set the Graphviz executable when registering the style".into());
        }
    }
    let format = match diagram_attribute(input, "graphviz", "format").as_deref() {
        None | Some("png") => Format::Png,
        Some("svg") => Format::Svg,
        Some("pdf") => Format::Pdf,
        Some(other) => {
            return Err(format!(
                "unsupported Graphviz format '{other}' (expected png, svg or pdf)"
            ));
        }
    };
    let layout = args.get_string("layout");
    let layout = layout.as_deref().or_else(|| {
        attrs
            .get("graphviz-layout")
            .and_then(DocumentAttributeValue::text)
    });
    let source = diagram_source(input);
    image(
        "graphviz",
        roots,
        input,
        format,
        [layout.unwrap_or_default(), "\0", source.as_ref()],
        false,
        |scratch, output| {
            let mut cmd = Command::new(dot);
            cmd.arg("-o")
                .arg(output)
                .arg(format!("-T{}", format.extension()));
            if let Some(layout) = layout {
                cmd.arg(format!("-K{layout}"));
            }
            asset::run_with_source(&mut cmd, working_dir, "dot", scratch, &source, None)
        },
    )
}
