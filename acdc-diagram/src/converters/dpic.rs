//! dpic, a PIC implementation that emits SVG directly.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `dpic`.
pub(crate) struct Dpic;

impl DiagramConverter for Dpic {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg]
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        _options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["dpic"]))?;

        // dpic expects a troff picture, so supply the delimiters when the
        // author has not written them.
        let mut code = String::with_capacity(source.code().len() + 8);
        if !source.code().starts_with(".PS") {
            code.push_str(".PS\n");
        }
        code.push_str(source.code());
        if !source.code().trim_end().ends_with("\n.PE") {
            code.push_str("\n.PE");
        }

        generate::file_to_stdout(&tool, format.as_str(), code.as_bytes(), |tool, input| {
            CommandSpec::new(tool)
                .args(["-v", "-z", &generate::native(input)])
                .chdir(source.base_dir())
        })
        .map(Generated::from)
    }
}
