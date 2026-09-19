//! Penrose diagrams, which need a substance, a style and a domain file.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, option, set_option},
    error::{Error, Result},
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `roger`, the Penrose command-line renderer.
pub(crate) struct Penrose;

impl DiagramConverter for Penrose {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        let mut options = ConverterOptions::new();
        set_option(&mut options, "domain", source.attr(&["domain_file"]));
        set_option(&mut options, "style", source.attr(&["style_file"]));
        set_option(&mut options, "variation", source.attr(&["variation"]));
        Ok(options)
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let domain = option(options, "domain")
            .ok_or_else(|| Error::config("a penrose diagram needs a `domain_file` attribute"))?;
        let style = option(options, "style")
            .ok_or_else(|| Error::config("a penrose diagram needs a `style_file` attribute"))?;
        let domain = source.resolve_path(domain, None);
        let style = source.resolve_path(style, None);
        let variation = option(options, "variation").map(str::to_string);

        let tool = source.find_command(&CommandLookup::new(&["roger"]))?;
        generate::file_to_file(
            &tool,
            "substance",
            format.as_str(),
            source.code().as_bytes(),
            |tool, input, output| {
                let mut spec = CommandSpec::new(tool).args([
                    "trio".to_string(),
                    "-o".to_string(),
                    generate::native(output),
                ]);
                if let Some(variation) = &variation {
                    spec = spec.args(["-v", variation]);
                }
                spec.args(["--path", "/", "--trio"])
                    .args([
                        generate::native(input),
                        generate::native(&domain),
                        generate::native(&style),
                    ])
                    .arg("--")
                    .chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
