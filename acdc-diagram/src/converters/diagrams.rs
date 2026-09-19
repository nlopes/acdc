//! mingrammer's `diagrams`, a Python library rather than a command-line tool.
//!
//! The block's code is a Python script whose `Diagram(...)` constructor is
//! rewritten to name the output file and format acdc wants, and the result is
//! then executed with the interpreter.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated},
    error::{Error, Result},
    source::{CommandLookup, DiagramSource},
};

/// The `diagrams` Python library.
pub(crate) struct Diagrams;

impl DiagramConverter for Diagrams {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Svg, Format::Pdf]
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        _options: &ConverterOptions,
    ) -> Result<Generated> {
        let python = source.find_command(
            &CommandLookup::new(&["python3", "python"]).with_attributes(&["diagrams-python"]),
        )?;

        // `diagrams` writes `<filename>.<outformat>`, so the constructor gets
        // the stem and the library appends the extension.
        let dir = tempfile::Builder::new()
            .prefix("acdc-diagrams-")
            .tempdir()
            .map_err(|error| Error::io("could not create a temporary directory", error))?;
        let stem = dir.path().join("diagram");
        let output = dir.path().join(format!("diagram.{format}"));

        let code = rewrite_constructor(source.code(), &stem.display().to_string(), format)?;

        crate::cli::run(
            &CommandSpec::new(&python).arg("-").chdir(source.base_dir()),
            Some(code.as_bytes()),
        )?;

        let data = std::fs::read(&output).map_err(|error| {
            Error::io(
                format!("`diagrams` did not write {}", output.display()),
                error,
            )
        })?;
        Ok(data.into())
    }
}

/// Point the script's `Diagram(...)` call at our output file.
fn rewrite_constructor(code: &str, filename: &str, format: Format) -> Result<String> {
    let start = code.find("Diagram(").ok_or_else(|| {
        Error::config("could not find a `Diagram(...)` constructor in the diagrams block")
    })?;
    let open = start + "Diagram(".len();
    let close = code[open..]
        .find(')')
        .ok_or_else(|| Error::config("the `Diagram(...)` constructor is not closed"))?
        + open;

    let existing = code[open..close].trim();
    let separator = if existing.is_empty() { "" } else { "," };
    Ok(format!(
        "{}{existing}{separator}filename=\"{filename}\",outformat=\"{format}\"{}",
        &code[..open],
        &code[close..]
    ))
}
