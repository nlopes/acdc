//! ditaa: ASCII art turned into rounded, shadowed boxes.

use crate::{
    Format,
    converters::{
        ConverterOptions, DiagramConverter, Generated,
        java::{self, Launcher},
        option,
    },
    error::{Error, Result},
    generate,
    source::DiagramSource,
};

/// Attribute name, and how it becomes a ditaa flag.
enum Flag {
    /// `--name value`, when the attribute is set at all.
    Valued(&'static str),
    /// `--name`, when the attribute equals the trigger value.
    Toggle(&'static str, &'static str),
}

const OPTIONS: &[(&str, Flag)] = &[
    ("scale", Flag::Valued("--scale")),
    ("tabs", Flag::Valued("--tabs")),
    ("background", Flag::Valued("--background")),
    ("bullet-characters", Flag::Valued("--bullet-characters")),
    ("antialias", Flag::Toggle("--no-antialias", "false")),
    ("separation", Flag::Toggle("--no-separation", "false")),
    ("shadows", Flag::Toggle("--no-shadows", "false")),
    ("round-corners", Flag::Toggle("--round-corners", "true")),
    ("debug", Flag::Toggle("--debug", "true")),
    ("fixed-slope", Flag::Toggle("--fixed-slope", "true")),
    ("transparent", Flag::Toggle("--transparent", "true")),
];

/// `ditaa`.
pub(crate) struct Ditaa;

impl DiagramConverter for Ditaa {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Svg, Format::Txt]
    }

    fn native_scaling(&self) -> bool {
        true
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        let mut options = ConverterOptions::new();
        for (attribute, _) in OPTIONS {
            let value = source
                .attr(&[attribute])
                .or_else(|| source.attr(&[&format!("ditaa-option-{attribute}")]));
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
        if !matches!(format, Format::Png | Format::Svg) {
            return Err(Error::UnsupportedFormat {
                diagram: source.diagram_type().to_string(),
                format: format.to_string(),
                supported: Format::list(self.supported_formats()),
            });
        }

        let launcher: Launcher = java::resolve(
            source,
            &["ditaa"],
            &["ditaajar", "ditaa"],
            "DIAGRAM_DITAA_CLASSPATH",
        )?;

        generate::file_to_file(
            launcher.program(),
            "ditaa",
            format.as_str(),
            source.code().as_bytes(),
            |_, input, output| {
                let mut spec = launcher
                    .spec()
                    .args([generate::native(input), generate::native(output)]);
                if format == Format::Svg {
                    spec = spec.arg("--svg");
                }
                for (attribute, flag) in OPTIONS {
                    let Some(value) = option(options, attribute) else {
                        continue;
                    };
                    match flag {
                        Flag::Valued(name) => spec = spec.args([*name, value]),
                        Flag::Toggle(name, trigger) if value == *trigger => spec = spec.arg(*name),
                        Flag::Toggle(_, _) => {}
                    }
                }
                spec.chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
