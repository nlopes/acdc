//! DBML database schema diagrams.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `dbml-renderer`.
pub(crate) struct Dbml;

impl DiagramConverter for Dbml {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg]
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        _format: Format,
        _options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["dbml-renderer"]))?;
        generate::stdin_to_stdout(&tool, source.code().as_bytes(), |tool| {
            CommandSpec::new(tool).chdir(source.base_dir())
        })
        .map(Generated::from)
    }
}
