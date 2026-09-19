//! VHS terminal recordings, written as `[tape]` blocks.
//!
//! A tape script runs whatever shell commands it contains, so it is only
//! generated when the document is processed in unsafe mode.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated},
    error::{Error, Result},
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `vhs`.
pub(crate) struct Vhs;

impl DiagramConverter for Vhs {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Gif, Format::Svg]
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        _options: &ConverterOptions,
    ) -> Result<Generated> {
        if !source.unsafe_mode() {
            return Err(Error::SafeMode {
                diagram: source.diagram_type().to_string(),
            });
        }

        let tool = source.find_command(&CommandLookup::new(&["vhs"]))?;
        generate::file_to_file(
            &tool,
            "tape",
            format.as_str(),
            source.code().as_bytes(),
            |tool, input, output| {
                CommandSpec::new(tool)
                    .args([
                        generate::native(input),
                        "-o".to_string(),
                        generate::native(output),
                    ])
                    .chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
