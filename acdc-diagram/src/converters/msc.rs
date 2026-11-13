//! Message sequence charts.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, collect_named, option},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `mscgen` / `mscgen_js`.
pub(crate) struct Mscgen;

impl DiagramConverter for Mscgen {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Svg]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        Ok(collect_named(source, &["font"]))
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["mscgen", "mscgen_js"]))?;
        generate::stdin_to_file(
            &tool,
            format.as_str(),
            source.code().as_bytes(),
            |tool, output| {
                let mut spec = CommandSpec::new(tool).args([
                    "-o",
                    &generate::native(output),
                    "-T",
                    format.as_str(),
                ]);
                if let Some(font) = option(options, "font") {
                    spec = spec.args(["-F", font]);
                }
                spec.arg("-").chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
