//! State machine cat.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, collect_named, option},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `smcat`.
pub(crate) struct Smcat;

impl DiagramConverter for Smcat {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        Ok(collect_named(source, &["direction", "engine"]))
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["smcat"]))?;
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
                if let Some(direction) = option(options, "direction") {
                    spec = spec.args(["-d", direction]);
                }
                if let Some(engine) = option(options, "engine") {
                    spec = spec.args(["-E", engine]);
                }
                spec.arg("-").chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
