//! Vega and Vega-Lite visualisations.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated},
    error::{Error, Result},
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `vg2svg` / `vg2png`, with `vl2vg` in front for Vega-Lite.
pub(crate) struct Vega {
    /// Whether the block was written as `[vegalite]`.
    lite: bool,
}

impl Vega {
    pub(crate) fn new(lite: bool) -> Self {
        Self { lite }
    }
}

impl DiagramConverter for Vega {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg, Format::Png]
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        _options: &ConverterOptions,
    ) -> Result<Generated> {
        // A Vega-Lite spec is also recognised by its `$schema`, so a plain
        // `[vega]` block holding one still works.
        let lite = self.lite
            || source.code().contains("/schema/vega-lite/")
            || source.attr(&["vegalite"]).is_some();

        let spec = if lite {
            let vl2vg = source.find_command(&CommandLookup::new(&["vl2vg"]))?;
            generate::stdin_to_stdout(&vl2vg, source.code().as_bytes(), |tool| {
                CommandSpec::new(tool).chdir(source.base_dir())
            })?
        } else {
            source.code().as_bytes().to_vec()
        };

        let renderer = match format {
            Format::Svg => "vg2svg",
            Format::Png => "vg2png",
            Format::Pdf
            | Format::Gif
            | Format::Jpeg
            | Format::Txt
            | Format::Atxt
            | Format::Utxt => {
                return Err(Error::UnsupportedFormat {
                    diagram: source.diagram_type().to_string(),
                    format: format.to_string(),
                    supported: Format::list(self.supported_formats()),
                });
            }
        };
        let tool = source.find_command(&CommandLookup::new(&[renderer]))?;
        let base_dir = source.base_dir().to_path_buf();

        generate::stdin_to_stdout(&tool, &spec, |tool| {
            let mut command = CommandSpec::new(tool).args(["--base", &generate::native(&base_dir)]);
            if format == Format::Svg {
                command = command.arg("--header");
            }
            command.chdir(&base_dir)
        })
        .map(Generated::from)
    }
}
