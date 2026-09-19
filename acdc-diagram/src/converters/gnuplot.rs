//! gnuplot, which is configured through a `set term` prologue rather than
//! command-line flags.

use std::fmt::Write as _;

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, collect_named, option},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `gnuplot`.
pub(crate) struct Gnuplot;

impl DiagramConverter for Gnuplot {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Svg, Format::Gif, Format::Txt]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        Ok(collect_named(
            source,
            &[
                "width",
                "height",
                "transparent",
                "crop",
                "font",
                "fontscale",
                "background",
            ],
        ))
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["gnuplot"]))?;

        // `dumb` is gnuplot's ASCII terminal; every other format names its
        // terminal after itself.
        let terminal = if format.is_text() {
            "dumb"
        } else {
            format.as_str()
        };
        let mut code = format!("set term {terminal}");

        if let (Some(width), Some(height)) = (option(options, "width"), option(options, "height")) {
            let _ = write!(code, " size {width},{height}");
        }
        if let Some(transparent) = option(options, "transparent") {
            code.push_str(if transparent == "false" {
                " notransparent"
            } else {
                " transparent"
            });
        }
        if let Some(crop) = option(options, "crop") {
            code.push_str(if crop == "false" { " nocrop" } else { " crop" });
        }
        if let Some(font) = option(options, "font") {
            let _ = write!(code, " font \"{font}\"");
        }
        if let Some(font_scale) = option(options, "fontscale") {
            let _ = write!(code, " fontscale {font_scale}");
        }
        if let Some(background) = option(options, "background") {
            let _ = write!(code, " background \"{background}\"");
        }
        code.push('\n');
        code.push_str(source.code());
        code.push('\n');

        generate::stdin_to_stdout(&tool, code.as_bytes(), |tool| {
            CommandSpec::new(tool).chdir(source.base_dir())
        })
        .map(Generated::from)
    }
}
