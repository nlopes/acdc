//! `UMLet` diagrams, which are always run out of a jar.

use crate::{
    Format,
    converters::{
        ConverterOptions, DiagramConverter, Generated,
        java::{self, Launcher},
    },
    error::Result,
    generate,
    source::DiagramSource,
};

/// `UMLet`.
pub(crate) struct Umlet;

impl DiagramConverter for Umlet {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg, Format::Png, Format::Pdf, Format::Gif]
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        _options: &ConverterOptions,
    ) -> Result<Generated> {
        let launcher: Launcher = java::resolve(
            source,
            &["umlet"],
            &["umletjar", "umlet"],
            "DIAGRAM_UMLET_CLASSPATH",
        )?;

        generate::file_to_file(
            launcher.program(),
            "uxf",
            format.as_str(),
            source.code().as_bytes(),
            |_, input, output| {
                launcher
                    .spec()
                    .arg("-action=convert")
                    .arg(format!("-format={format}"))
                    .arg(format!("-filename={}", generate::native(input)))
                    .arg(format!("-output={}", generate::native(output)))
                    .chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
