//! Mermaid, rendered through the `mmdc` command-line client.
//!
//! asciidoctor-diagram also carries a fallback onto the pre-`mmdc` `mermaid`
//! binary driven by `PhantomJS`. `PhantomJS` has been abandoned since 2018, so
//! acdc only drives `mmdc`.

use std::path::Path;

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, option, set_option},
    error::{Error, Result},
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `mmdc`.
pub(crate) struct Mermaid;

impl DiagramConverter for Mermaid {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Svg]
    }

    fn native_scaling(&self) -> bool {
        true
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        let mut options = ConverterOptions::new();
        set_option(&mut options, "css", source.attr(&["css"]));
        set_option(
            &mut options,
            "gantt-config",
            source.attr(&["ganttconfig", "gantt-config"]),
        );
        set_option(
            &mut options,
            "sequence-config",
            source.attr(&["sequenceconfig", "sequence-config"]),
        );
        set_option(&mut options, "width", source.attr(&["width"]));
        set_option(&mut options, "height", source.attr(&["height"]));
        set_option(&mut options, "theme", source.attr(&["theme"]));
        set_option(&mut options, "background", source.attr(&["background"]));
        set_option(&mut options, "config", source.attr(&["config"]));
        set_option(
            &mut options,
            "puppeteer-config",
            source.attr(&["puppeteerconfig", "puppeteer-config"]),
        );

        if let Some(scale) = source.attr(&["scale"]) {
            if !scale.chars().all(|c| c.is_ascii_digit()) || scale.is_empty() {
                return Err(Error::config(format!(
                    "mermaid only supports integer scale factors, not `{scale}`"
                )));
            }
            options.insert("scale".to_string(), scale);
        }
        Ok(options)
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["mmdc"]))?;
        let resolve =
            |name: &str| option(options, name).map(|value| source.resolve_path(value, None));

        let css = resolve("css");
        let config = resolve("config");
        let puppeteer = resolve("puppeteer-config");
        let gantt = resolve("gantt-config");
        let sequence = resolve("sequence-config");

        // When no explicit config file is given but a gantt or sequence
        // fragment is, they are merged into one config file for mmdc.
        let merged_config = if config.is_none() && (gantt.is_some() || sequence.is_some()) {
            Some(merge_config(gantt.as_deref(), sequence.as_deref())?)
        } else {
            None
        };
        let config_file =
            config.or_else(|| merged_config.as_ref().map(|file| file.path().to_path_buf()));

        generate::file_to_file(
            &tool,
            "mmd",
            format.as_str(),
            source.code().as_bytes(),
            |tool, input, output| {
                let mut spec = CommandSpec::new(tool).args([
                    "-i".to_string(),
                    generate::native(input),
                    "-o".to_string(),
                    generate::native(output),
                ]);
                if let Some(css) = &css {
                    spec = spec.args(["--cssFile".to_string(), generate::native(css)]);
                }
                for (flag, name) in [
                    ("--theme", "theme"),
                    ("--width", "width"),
                    ("--height", "height"),
                    ("--scale", "scale"),
                ] {
                    if let Some(value) = option(options, name) {
                        spec = spec.args([flag, value]);
                    }
                }
                if let Some(background) = option(options, "background") {
                    let colour = if background.starts_with('#') {
                        background.to_string()
                    } else {
                        format!("#{background}")
                    };
                    spec = spec.args(["--backgroundColor".to_string(), colour]);
                }
                if let Some(config) = &config_file {
                    spec = spec.args(["--configFile".to_string(), generate::native(config)]);
                }
                if let Some(puppeteer) = &puppeteer {
                    spec = spec.args([
                        "--puppeteerConfigFile".to_string(),
                        generate::native(puppeteer),
                    ]);
                }
                // Without this, a rejected promise inside mmdc exits 0 and
                // leaves an empty output file behind.
                spec.env("NODE_OPTIONS", "--unhandled-rejections=strict")
                    .chdir(source.base_dir())
            },
        )
        .map(Generated::from)
    }
}

/// Build a temporary mermaid config file out of the gantt and sequence
/// fragments the document supplied.
fn merge_config(gantt: Option<&Path>, sequence: Option<&Path>) -> Result<tempfile::NamedTempFile> {
    let mut sections = Vec::new();
    for (key, path) in [("gantt", gantt), ("sequence", sequence)] {
        if let Some(path) = path {
            let body = std::fs::read_to_string(path).map_err(|source| {
                Error::io(format!("could not read {}", path.display()), source)
            })?;
            sections.push(format!("\"{key}\": {}", body.trim()));
        }
    }
    let file = tempfile::Builder::new()
        .prefix("acdc-mermaid-")
        .suffix(".json")
        .tempfile()
        .map_err(|source| Error::io("could not create a mermaid config file", source))?;
    std::fs::write(file.path(), format!("{{{}}}", sections.join(",")))
        .map_err(|source| Error::io("could not write the mermaid config file", source))?;
    Ok(file)
}
