//! Pintora text-to-diagram rendering.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, collect_named, option},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `pintora`.
pub(crate) struct Pintora;

impl DiagramConverter for Pintora {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Svg]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        Ok(collect_named(
            source,
            &["width", "theme", "background-color", "pixel-ratio"],
        ))
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["pintora"]))?;
        generate::file_to_file(
            &tool,
            "pintora",
            format.as_str(),
            source.code().as_bytes(),
            |tool, input, output| {
                let mut spec = CommandSpec::new(tool).args([
                    "render".to_string(),
                    "-i".to_string(),
                    generate::native(input),
                    "-o".to_string(),
                    generate::native(output),
                ]);
                for (flag, name) in [
                    ("-w", "width"),
                    ("-t", "theme"),
                    ("-p", "pixel-ratio"),
                    ("-b", "background-color"),
                ] {
                    if let Some(value) = option(options, name) {
                        spec = spec.args([flag, value]);
                    }
                }
                spec.chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
