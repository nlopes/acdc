//! D2, Terrastruct's declarative diagram language.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, set_option},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// Attribute name, D2 flag, and whether the value is a font file path.
const OPTIONS: &[(&str, &str)] = &[
    ("layout", "--layout"),
    ("theme", "--theme"),
    ("dark-theme", "--dark-theme"),
    ("pad", "--pad"),
    ("animate-interval", "--animate-interval"),
    ("font-regular", "--font-regular"),
    ("font-italic", "--font-italic"),
    ("font-bold", "--font-bold"),
];

/// `d2`.
pub(crate) struct D2;

impl DiagramConverter for D2 {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg, Format::Png, Format::Pdf]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        let mut options = ConverterOptions::new();
        for (attribute, _) in OPTIONS {
            set_option(&mut options, attribute, source.attr(&[attribute]));
        }
        set_option(&mut options, "sketch", source.attr(&["sketch"]));
        Ok(options)
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["d2"]))?;
        generate::stdin_to_file(
            &tool,
            format.as_str(),
            source.code().as_bytes(),
            |tool, output| {
                let mut spec = CommandSpec::new(tool).args(["--browser", "false"]);
                for (attribute, flag) in OPTIONS {
                    if let Some(value) = options.get(*attribute) {
                        let value = if attribute.starts_with("font") {
                            generate::native(std::path::Path::new(value))
                        } else {
                            value.clone()
                        };
                        spec = spec.arg(*flag).arg(value);
                    }
                }
                // `sketch` is a flag, so any value other than an explicit `false`
                // turns it on.
                if options.get("sketch").is_some_and(|value| value != "false") {
                    spec = spec.arg("--sketch");
                }
                spec.arg("-")
                    .arg(generate::native(output))
                    .chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}
