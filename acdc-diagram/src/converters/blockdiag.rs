//! The blockdiag family: blockdiag, seqdiag, actdiag, nwdiag, rackdiag and
//! packetdiag all share a command line and differ only in the executable name.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, option, set_option},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// One member of the blockdiag family.
pub(crate) struct BlockDiag {
    tool: &'static str,
    /// Debian packages the Python 3 builds with a `3` suffix.
    alt_tool: String,
}

impl BlockDiag {
    pub(crate) fn new(tool: &'static str) -> Self {
        Self {
            tool,
            alt_tool: format!("{tool}3"),
        }
    }
}

impl DiagramConverter for BlockDiag {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Pdf, Format::Svg]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        let mut options = ConverterOptions::new();
        set_option(&mut options, "font_path", source.attr(&["fontpath"]));
        Ok(options)
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let commands = [self.tool, self.alt_tool.as_str()];
        let tool = source.find_command(&CommandLookup::new(&commands))?;
        generate::stdin_to_file(
            &tool,
            format.as_str(),
            source.code().as_bytes(),
            |tool, output| {
                let mut spec = CommandSpec::new(tool)
                    .args(["-a", "-o", &generate::native(output)])
                    .arg(format!("-T{format}"));
                if let Some(font_path) = option(options, "font_path") {
                    spec = spec.arg(format!(
                        "-f{}",
                        generate::native(std::path::Path::new(font_path))
                    ));
                }
                spec.arg("-").chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
