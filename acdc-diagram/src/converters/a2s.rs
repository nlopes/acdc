//! `ASCIIToSVG`: ASCII art turned into SVG.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, option, set_option},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `a2s`.
pub(crate) struct A2s;

impl DiagramConverter for A2s {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg, Format::Txt]
    }

    fn native_scaling(&self) -> bool {
        true
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        let mut options = ConverterOptions::new();
        set_option(&mut options, "scalex", source.attr(&["scalex"]));
        set_option(&mut options, "scaley", source.attr(&["scaley"]));
        set_option(&mut options, "scale", source.attr(&["scale"]));
        set_option(&mut options, "font", source.attr(&["fontfamily"]));
        if source.attr(&["noblur"]).as_deref() == Some("true") {
            set_option(&mut options, "noblur", Some("true".to_string()));
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
        let tool = source.find_command(&CommandLookup::new(&["a2s"]))?;
        generate::stdin_to_file(
            &tool,
            format.as_str(),
            source.code().as_bytes(),
            |tool, output| {
                let mut spec = CommandSpec::new(tool).args(["-o", &generate::native(output)]);
                match (
                    option(options, "scalex"),
                    option(options, "scaley"),
                    option(options, "scale"),
                ) {
                    (Some(sx), Some(sy), _) => spec = spec.args(["-s", &format!("{sx},{sy}")]),
                    (_, _, Some(scale)) => spec = spec.args(["-s", &format!("{scale},{scale}")]),
                    _ => {}
                }
                if option(options, "noblur").is_some() {
                    spec = spec.arg("-b");
                }
                if let Some(font) = option(options, "font") {
                    spec = spec.args(["-f", font]);
                }
                spec.chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
