//! `GoAT`: Go ASCII Art to SVG.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, option, set_option},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `goat`.
pub(crate) struct Goat;

impl DiagramConverter for Goat {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        let mut options = ConverterOptions::new();
        set_option(
            &mut options,
            "dark-scheme",
            source.attr(&["svg-color-dark-scheme"]),
        );
        set_option(
            &mut options,
            "light-scheme",
            source.attr(&["svg-color-light-scheme"]),
        );
        Ok(options)
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        _format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["goat"]))?;
        generate::stdin_to_stdout(&tool, source.code().as_bytes(), |tool| {
            let mut spec = CommandSpec::new(tool);
            if let Some(value) = option(options, "dark-scheme") {
                spec = spec.args(["-sds", value]);
            }
            if let Some(value) = option(options, "light-scheme") {
                spec = spec.args(["-sls", value]);
            }
            spec.chdir(source.base_dir())
        })
        .map(Generated::from)
    }
}
