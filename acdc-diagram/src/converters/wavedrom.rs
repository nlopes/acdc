//! `WaveDrom` digital timing diagrams.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `wavedrom-cli`.
pub(crate) struct Wavedrom;

impl DiagramConverter for Wavedrom {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Svg]
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        _options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(
            &CommandLookup::new(&["wavedrom-cli", "wavedrom"]).with_attributes(&["wavedrom"]),
        )?;
        generate::file_to_file(
            &tool,
            "wvd",
            format.as_str(),
            source.code().as_bytes(),
            |tool, input, output| {
                CommandSpec::new(tool)
                    .args([
                        "--input".to_string(),
                        generate::native(input),
                        format!("--{format}"),
                        generate::native(output),
                    ])
                    .chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
