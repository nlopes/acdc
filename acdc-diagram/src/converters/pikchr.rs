//! Pikchr, the PIC-like language from the SQLite project.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated},
    error::{Error, Result},
    generate,
    source::{CommandLookup, DiagramSource},
};

/// `pikchr`.
pub(crate) struct Pikchr;

impl DiagramConverter for Pikchr {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Svg]
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        _options: &ConverterOptions,
    ) -> Result<Generated> {
        let tool = source.find_command(&CommandLookup::new(&["pikchr"]))?;
        let output = generate::file_to_stdout(
            &tool,
            format.as_str(),
            source.code().as_bytes(),
            |tool, input| {
                CommandSpec::new(tool)
                    .args(["--svg-only", &generate::native(input)])
                    .chdir(source.base_dir())
            },
        )?;

        // Pikchr reports syntax errors on stdout, wrapped in HTML, and still
        // exits successfully — so the payload has to be inspected.
        let text = String::from_utf8_lossy(&output);
        if text.trim_start().starts_with("<svg") {
            Ok(output.into())
        } else {
            Err(Error::CommandFailed {
                command: "pikchr".to_string(),
                output: strip_tags(&text).trim().to_string(),
            })
        }
    }
}

/// Drop the HTML tags pikchr wraps its error messages in.
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0_usize;
    for character in text.chars() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(character),
            _ => {}
        }
    }
    out
}
