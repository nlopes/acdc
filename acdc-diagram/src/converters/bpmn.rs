//! BPMN process diagrams.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, collect_named, option},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `bpmn-js`.
pub(crate) struct Bpmn;

impl DiagramConverter for Bpmn {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Svg, Format::Pdf, Format::Jpeg]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        Ok(collect_named(source, &["width", "height"]))
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["bpmn-js"]))?;
        generate::file_to_file(
            &tool,
            "bpmn",
            format.as_str(),
            source.code().as_bytes(),
            |tool, input, output| {
                let mut spec = CommandSpec::new(tool).args([
                    generate::native(input),
                    "-o".to_string(),
                    generate::native(output),
                    "-t".to_string(),
                    format.to_string(),
                ]);
                if let Some(width) = option(options, "width") {
                    spec = spec.args(["--width", width]);
                }
                if let Some(height) = option(options, "height") {
                    spec = spec.args(["--height", height]);
                }
                spec.chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
