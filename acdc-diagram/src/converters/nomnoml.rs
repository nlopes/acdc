//! nomnoml UML sketches.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `nomnoml`.
pub(crate) struct Nomnoml;

impl DiagramConverter for Nomnoml {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg]
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        _options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["nomnoml"]))?;
        generate::file_to_file(
            &tool,
            "nomnoml",
            format.as_str(),
            source.code().as_bytes(),
            |tool, input, output| {
                CommandSpec::new(tool)
                    .args([generate::native(input), generate::native(output)])
                    .chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
