//! svgbob: ASCII art turned into hand-drawn-looking SVG.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// Attribute name and the flag it maps to.
const OPTIONS: &[(&str, &str)] = &[
    ("font-family", "--font-family"),
    ("font-size", "--font-size"),
    ("stroke-width", "--stroke-width"),
    ("scale", "--scale"),
];

/// `svgbob` / `svgbob_cli`.
pub(crate) struct Svgbob;

impl DiagramConverter for Svgbob {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg, Format::Txt]
    }

    fn native_scaling(&self) -> bool {
        true
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        let mut options = ConverterOptions::new();
        for (attribute, _) in OPTIONS {
            // The second lookup lets a document set defaults for every svgbob
            // block via `:svgbob-option-scale:`-style attributes.
            let value = source
                .attr(&[attribute])
                .or_else(|| source.attr(&[&format!("svgbob-option-{attribute}")]));
            crate::converters::set_option(&mut options, attribute, value);
        }
        Ok(options)
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        if format == Format::Txt {
            return Ok(source.code().as_bytes().to_vec().into());
        }
        let tool = source.find_command(&CommandLookup::new(&["svgbob", "svgbob_cli"]))?;
        generate::stdin_to_file(
            &tool,
            format.as_str(),
            source.code().as_bytes(),
            |tool, output| {
                let mut spec = CommandSpec::new(tool).args(["-o", &generate::native(output)]);
                for (attribute, flag) in OPTIONS {
                    if let Some(value) = options.get(*attribute) {
                        spec = spec.arg(*flag).arg(value.clone());
                    }
                }
                spec.chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
