//! Symbolator HDL symbol diagrams.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `symbolator`.
pub(crate) struct Symbolator;

impl DiagramConverter for Symbolator {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Pdf, Format::Svg]
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        _options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["symbolator"]))?;
        generate::stdin_to_file(
            &tool,
            format.as_str(),
            source.code().as_bytes(),
            |tool, output| {
                CommandSpec::new(tool)
                    .arg("-i-")
                    .arg(format!("-o{}", generate::native(output)))
                    .arg(format!("-f{format}"))
                    .chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
