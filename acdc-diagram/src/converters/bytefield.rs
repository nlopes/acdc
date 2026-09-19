//! Byte-field diagrams for protocol and format documentation.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `bytefield-svg`.
pub(crate) struct Bytefield;

impl DiagramConverter for Bytefield {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg]
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        _options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["bytefield-svg"]))?;
        generate::stdin_to_file(
            &tool,
            format.as_str(),
            source.code().as_bytes(),
            |tool, output| {
                CommandSpec::new(tool)
                    .args(["--output", &generate::native(output)])
                    .chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
