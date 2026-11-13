//! Shaape ASCII art diagrams.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `shaape`.
pub(crate) struct Shaape;

impl DiagramConverter for Shaape {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Svg]
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        _options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["shaape"]))?;
        generate::stdin_to_file(
            &tool,
            format.as_str(),
            source.code().as_bytes(),
            |tool, output| {
                CommandSpec::new(tool)
                    .args(["-o", &generate::native(output), "-t", format.as_str(), "-"])
                    .chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
