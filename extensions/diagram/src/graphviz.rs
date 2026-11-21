//! Graphviz's stdin-to-file adapter, ported from acdc-diagram (PR #451).

use std::{fs, io, path::Path, process::Command};

use acdc_parser::{
    BlockMetadata, DocumentAttributeValue,
    extensions::{
        BlockBuilder, BlockMacro, BlockStyle, MacroExecutionContext, MacroInvocation, MacroTarget,
    },
};

use super::{
    DiagramInput, Directories, OutputDirectories,
    asset::{self, Format},
    diagram_attribute, diagram_source, executable, image, recover, require_unsafe, style,
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
    let dirs = Directories::resolve(output_dir.into())?;
    Ok(BlockMacro::new("graphviz", move || {
        let dirs = dirs.clone();
        let dot = dot.clone();
        move |call: &MacroInvocation<'_>, context: &mut MacroExecutionContext| {
            let attrs = (&call.document_attributes).into();
            require_unsafe("Graphviz", attrs)?;
            let mut metadata = BlockMetadata::new();
            metadata.attributes = call.attributes.clone();
            let options = call
                .attributes
                .get_string("opts")
                .or_else(|| call.attributes.get_string("options"));
            if let Some(options) = options.as_deref() {
                metadata = metadata.with_options(options.split(',').map(str::trim).collect());
            }
            if let Some(subs) = call.attributes.get_string("subs") {
                metadata.substitutions = Some(acdc_parser::SubstitutionSpec::Source(
                    subs.split(',').map(|part| part.trim().to_owned()).collect(),
                ));
            }
            let file = context.read_local_target();
            let input = DiagramInput {
                source_file: file.as_ref().ok().map(|(path, _)| path.as_path()),
                body: file
                    .as_ref()
                    .map_or(call.source.as_ref(), |(_, text)| text.as_str()),
                metadata: &metadata,
                document_attributes: attrs,
            };
            let result = match &file {
                Ok((path, _)) => render(&dot, path.parent().unwrap_or(path), dirs.roots(), &input)
                    .map(|image| {
                        let attributes = call.named_arguments_source(|name| {
                            !matches!(
                                name,
                                "target"
                                    | "format"
                                    | "layout"
                                    | "on-error"
                                    | "scale"
                                    | "subs"
                                    | "svg-type"
                                    | "dot"
                                    | "graphvizdot"
                            )
                        });
                        if attributes.is_empty() {
                            image
                        } else {
                            image.with_attributes(attributes)
                        }
                    }),
                Err(error) => Err(format!("Graphviz source '{}': {error}", call.target)),
            };
            recover(result, "Graphviz", &input, |msg| context.warn(msg)).map(Into::into)
        }
    })
    .with_target(MacroTarget::Required)
    .with_positional_attributes(["format"]))
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
        |scratch, output| {
            let input_path = scratch.join("input.dot");
            fs::write(&input_path, source.as_bytes())?;
            let mut cmd = Command::new(dot);
            cmd.arg("-o")
                .arg(output)
                .arg(format!("-T{}", format.extension()));
            if let Some(layout) = layout {
                cmd.arg(format!("-K{layout}"));
            }
            asset::run_with_input(
                &mut cmd,
                working_dir,
                "dot",
                fs::File::open(input_path)?.into(),
            )
        },
    )
}
