//! Message sequence charts rendered with the mscgen protocol from PR #451.

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

/// Register the `msc` listing and literal block style with a caller-selected executable.
///
/// # Errors
/// Returns an error if the executable or output directories cannot be resolved.
pub fn msc<'a>(
    tool: &Path,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockStyle> {
    let tool = executable(std::path::absolute(tool)?)?;
    style("msc", output_dir.into(), move |roots, input| {
        render(&tool, roots.embedded, roots, input)
    })
}

/// Register the `msc::file.msc[format]` macro using the parser's local target policy.
///
/// # Errors
/// Returns an error if the executable or output directories cannot be resolved.
pub fn msc_file<'a>(
    tool: &Path,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockMacro> {
    let tool = executable(std::path::absolute(tool)?)?;
    file_macro(
        "msc",
        output_dir.into(),
        &["mscgen", "mscgen_js", "font"],
        move |dir, roots, input| render(&tool, dir, roots, input),
    )
}

fn render(
    tool: &Path,
    dir: &Path,
    roots: OutputDirectories<'_>,
    input: &DiagramInput<'_>,
) -> Result<BlockBuilder, String> {
    let args = &input.metadata.attributes;
    let attrs = input.document_attributes;
    if ["mscgen", "mscgen_js"]
        .iter()
        .any(|key| args.get(key).is_some() || attrs.get(key).is_some())
    {
        return Err("set the mscgen executable when registering the diagram".into());
    }
    let format = match diagram_attribute(input, "msc", "format").as_deref() {
        None | Some("png") => Format::Png,
        Some("svg") => Format::Svg,
        Some(other) => {
            return Err(format!(
                "unsupported msc format '{other}' (expected png or svg)"
            ));
        }
    };
    let font = args.get_string("font");
    let font = font
        .as_deref()
        .or_else(|| attrs.get("msc-font").and_then(DocumentAttributeValue::text));
    let source = diagram_source(input);
    image(
        "msc",
        roots,
        input,
        format,
        [font.unwrap_or_default(), "\0", source.as_ref()],
        |scratch, output| {
            let mut cmd = Command::new(tool);
            cmd.arg("-o").arg(output).arg("-T").arg(format.extension());
            if let Some(font) = font {
                cmd.arg("-F").arg(font);
            }
            asset::run_with_source(cmd.arg("-"), dir, "mscgen", scratch, &source, None)
        },
    )
}
