//! Syntrax railroad (syntax) diagrams.

use crate::{
    Format,
    converters::{
        ConverterOptions, DiagramConverter, Generated,
        java::{self, Launcher},
        option, set_option,
    },
    error::Result,
    generate,
    source::DiagramSource,
};

/// `syntrax`, or the `JSyntrax` jar.
pub(crate) struct Syntrax;

impl DiagramConverter for Syntrax {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Svg]
    }

    fn native_scaling(&self) -> bool {
        true
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        let mut options = ConverterOptions::new();
        set_option(&mut options, "heading", source.attr(&["heading"]));
        set_option(&mut options, "scale", source.attr(&["scale"]));
        set_option(&mut options, "transparent", source.attr(&["transparent"]));
        set_option(
            &mut options,
            "style",
            source
                .attr(&["style-file"])
                .or_else(|| source.doc_attr(&format!("{}-style", source.diagram_type()))),
        );
        Ok(options)
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let launcher: Launcher = java::resolve(
            source,
            &["syntrax", "jsyntrax"],
            &["jsyntraxjar", "syntrax"],
            "DIAGRAM_JSYNTRAX_CLASSPATH",
        )?;

        generate::file_to_file(
            launcher.program(),
            "spec",
            format.as_str(),
            source.code().as_bytes(),
            |_, input, output| {
                let mut spec = launcher.spec().args([
                    "-i".to_string(),
                    generate::native(input),
                    "-o".to_string(),
                    generate::native(output),
                ]);
                if let Some(title) = option(options, "heading") {
                    spec = spec.args(["--title", title]);
                }
                if let Some(scale) = option(options, "scale") {
                    spec = spec.args(["--scale", scale]);
                }
                if option(options, "transparent") == Some("true") {
                    spec = spec.arg("--transparent");
                }
                if let Some(style) = option(options, "style") {
                    spec = spec.args(["--style", style]);
                }
                spec.chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
