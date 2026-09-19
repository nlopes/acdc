//! oxdraw ASCII diagrams.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, collect_named, option},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `oxdraw`.
pub(crate) struct Oxdraw;

impl DiagramConverter for Oxdraw {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg, Format::Png]
    }

    fn native_scaling(&self) -> bool {
        true
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        Ok(collect_named(source, &["background", "scale"]))
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["oxdraw"]))?;
        let data = generate::stdin_to_file(
            &tool,
            format.as_str(),
            source.code().as_bytes(),
            |tool, output| {
                let mut spec = CommandSpec::new(tool);
                for name in ["background", "scale"] {
                    if let Some(value) = option(options, name) {
                        spec = spec.arg(format!("--{name}")).arg(value);
                    }
                }
                spec.args(["--input", "-", "--output"])
                    .arg(generate::native(output))
                    .args(["--output-format", format.as_str(), "--quiet"])
                    .chdir(source.base_dir())
            },
        )?;

        // oxdraw applies `scale` to raster output only, so the SVG root has to
        // be resized here for the declared native scaling to be honest.
        if format == Format::Svg
            && let Some(scale) =
                option(options, "scale").and_then(|value| value.parse::<f64>().ok())
        {
            return Ok(scale_svg(data, scale).into());
        }
        Ok(data.into())
    }
}

/// Multiply the `width` and `height` of an SVG root element by `scale`.
fn scale_svg(data: Vec<u8>, scale: f64) -> Vec<u8> {
    let Ok(text) = String::from_utf8(data) else {
        return Vec::new();
    };
    let mut out = text;
    for attribute in ["width", "height"] {
        let needle = format!("{attribute}=\"");
        let Some(start) = out.find(&needle) else {
            continue;
        };
        let value_start = start + needle.len();
        let Some(length) = out[value_start..].find('"') else {
            continue;
        };
        let value = &out[value_start..value_start + length];
        let Ok(number) = value.parse::<f64>() else {
            continue;
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "SVG pixel dimensions are small integers"
        )]
        let scaled = (number * scale) as i64;
        out.replace_range(value_start..value_start + length, &scaled.to_string());
    }
    out.into_bytes()
}
