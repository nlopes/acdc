use acdc_parser::{
    BlockMetadata, SubstitutionSpec,
    extensions::{BlockBuilder, BlockMacro, MacroExecutionContext, MacroInvocation, MacroTarget},
};
use std::{io, path::Path};

use super::{DiagramInput, Directories, OutputDirectories, recover, require_unsafe};

pub(super) fn file_macro<F>(
    label: &'static str,
    output_dir: OutputDirectories<'_>,
    renderer_arguments: &'static [&'static str],
    render: F,
) -> io::Result<BlockMacro>
where
    F: Fn(&Path, OutputDirectories<'_>, &DiagramInput<'_>) -> Result<BlockBuilder, String>
        + Clone
        + Send
        + Sync
        + 'static,
{
    let dirs = Directories::resolve(output_dir)?;
    Ok(BlockMacro::new(label.to_ascii_lowercase(), move || {
        let dirs = dirs.clone();
        let render = render.clone();
        move |call: &MacroInvocation<'_>, context: &mut MacroExecutionContext| {
            let attrs = (&call.document_attributes).into();
            require_unsafe(label, attrs)?;
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
                metadata.substitutions = Some(SubstitutionSpec::Source(
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
                Ok((path, _)) => {
                    render(path.parent().unwrap_or(path), dirs.roots(), &input).map(|image| {
                        let attributes = call.named_arguments_source(|name| {
                            !matches!(
                                name,
                                "target" | "format" | "on-error" | "scale" | "subs" | "svg-type"
                            ) && !renderer_arguments.contains(&name)
                        });
                        if attributes.is_empty() {
                            image
                        } else {
                            image.with_attributes(attributes)
                        }
                    })
                }
                Err(error) => Err(format!("{label} source '{}': {error}", call.target)),
            };
            recover(result, label, &input, |msg| context.warn(msg)).map(Into::into)
        }
    })
    .with_target(MacroTarget::Required)
    .with_positional_attributes(["format"]))
}
