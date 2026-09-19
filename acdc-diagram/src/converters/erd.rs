//! Entity-relationship diagrams: `erd` emits DOT, which `Graphviz` then renders.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `erd` / `erd-go`.
pub(crate) struct Erd;

impl DiagramConverter for Erd {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Svg]
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        _options: &ConverterOptions,
    ) -> Result<Generated> {
        let erd = source.find_command(&CommandLookup::new(&["erd", "erd-go"]))?;
        let dot =
            source.find_command(&CommandLookup::new(&["dot"]).with_attributes(&["graphvizdot"]))?;

        let dot_code =
            generate::stdin_to_file(&erd, "dot", source.code().as_bytes(), |tool, output| {
                CommandSpec::new(tool)
                    .args(["-o", &generate::native(output), "-f", "dot"])
                    .chdir(source.base_dir())
            })?;

        generate::stdin_to_file(&dot, format.as_str(), &dot_code, |tool, output| {
            CommandSpec::new(tool)
                .arg(format!("-o{}", generate::native(output)))
                .arg(format!("-T{format}"))
                .chdir(source.base_dir())
        })
        .map(Generated::from)
    }
}
