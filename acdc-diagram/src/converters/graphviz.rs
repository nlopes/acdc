//! `Graphviz`, and the graphviz-py preprocessor that adds Python expressions.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, collect_named, option},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `dot` and friends.
pub(crate) struct Graphviz;

impl DiagramConverter for Graphviz {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Pdf, Format::Svg]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        Ok(collect_named(source, &["layout"]))
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool =
            source.find_command(&CommandLookup::new(&["dot"]).with_attributes(&["graphvizdot"]))?;
        generate::stdin_to_file(
            &tool,
            format.as_str(),
            source.code().as_bytes(),
            |tool, output| {
                let mut spec = CommandSpec::new(tool)
                    .arg(format!("-o{}", generate::native(output)))
                    .arg(format!("-T{format}"));
                if let Some(layout) = option(options, "layout") {
                    spec = spec.arg(format!("-K{layout}"));
                }
                spec.chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}

/// `graphviz-py`, which evaluates `{{ … }}` Python before calling `dot`.
pub(crate) struct GraphvizPy;

impl DiagramConverter for GraphvizPy {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Pdf, Format::Svg]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        Ok(collect_named(source, &["layout", "argument"]))
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["graphviz-py"]))?;
        generate::stdin_to_file(
            &tool,
            format.as_str(),
            source.code().as_bytes(),
            |tool, output| {
                let mut spec = CommandSpec::new(tool)
                    .arg(format!("-o{}", generate::native(output)))
                    .arg(format!("-T{format}"));
                if let Some(layout) = option(options, "layout") {
                    spec = spec.arg(format!("-K{layout}"));
                }
                if let Some(argument) = option(options, "argument") {
                    spec = spec.arg("-a").arg(argument);
                }
                spec.chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
