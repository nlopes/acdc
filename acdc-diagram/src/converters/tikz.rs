//! `TikZ` pictures, typeset by `LaTeX` and optionally converted to SVG.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, option, set_option},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
};

/// The standalone document `TikZ` bodies are wrapped in.
const PREAMBLE: &str = "\\documentclass[border=2bp, tikz]{standalone}\n\\usepackage{tikz}\n";
const BODY_START: &str =
    "\\begin{document}\n\\begingroup\n\\tikzset{every picture/.style={scale=1}}\n";
const BODY_END: &str = "\n\\endgroup\n\\end{document}\n";

/// The engine used unless the document names another one.
const DEFAULT_ENGINE: &str = "pdflatex";

/// A `LaTeX` engine, with `pdf2svg` for SVG output.
pub(crate) struct TikZ;

impl DiagramConverter for TikZ {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Pdf, Format::Svg]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        let mut options = ConverterOptions::new();
        if source.opt("preamble") || source.attr(&["preamble"]).as_deref() == Some("true") {
            options.insert("preamble".to_string(), "true".to_string());
        }
        // Recording the engine here — rather than reading it inside `convert` —
        // is what makes switching engines invalidate the cached image.
        set_option(&mut options, "command", source.attr(&["command"]));
        Ok(options)
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let engine = [engine(options)];
        let latex = source.find_command(&CommandLookup::new(&engine))?;

        // With `preamble`, everything before the `~~~~` line is LaTeX preamble
        // rather than picture body.
        let (preamble, body) = if options.contains_key("preamble") {
            source
                .code()
                .split_once("\n~~~~\n")
                .map_or_else(|| ("", source.code()), |(preamble, body)| (preamble, body))
        } else {
            ("", source.code())
        };

        let document = format!("{PREAMBLE}{preamble}{BODY_START}{body}{BODY_END}");

        let pdf = generate::file_to_file(
            &latex,
            "tex",
            "pdf",
            document.as_bytes(),
            |tool, input, output| {
                let directory = generate::parent_of(output);
                CommandSpec::new(tool)
                    .args([
                        "-shell-escape".to_string(),
                        "-file-line-error".to_string(),
                        "-interaction=nonstopmode".to_string(),
                        "-output-directory".to_string(),
                        generate::native(&directory),
                        generate::native(input),
                    ])
                    // Every engine names its output after the input file.
                    .out_file(directory.join("input.pdf"))
            },
        )?;

        if format == Format::Pdf {
            return Ok(pdf.into());
        }

        let pdf2svg = source.find_command(&CommandLookup::new(&["pdf2svg"]))?;
        generate::file_to_file(&pdf2svg, "pdf", "svg", &pdf, |tool, input, output| {
            CommandSpec::new(tool)
                .args([generate::native(input), generate::native(output)])
                .chdir(source.base_dir())
        })
        .map(Generated::from)
    }
}

/// The `LaTeX` engine to run: `command=lualatex` on the block, else
/// `:tikz-command:` on the document, else `pdflatex`.
///
/// The engine is looked up like any other tool, so `:lualatex: /opt/tex/lualatex`
/// pins a particular build and a value containing a path separator is used
/// directly.
fn engine(options: &ConverterOptions) -> &str {
    option(options, "command")
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .unwrap_or(DEFAULT_ENGINE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_pdflatex() {
        assert_eq!(engine(&ConverterOptions::new()), "pdflatex");
    }

    #[test]
    fn honours_an_explicit_command() {
        let options = ConverterOptions::from([("command".to_string(), "lualatex".to_string())]);
        assert_eq!(engine(&options), "lualatex");
    }

    #[test]
    fn ignores_a_blank_command() {
        let options = ConverterOptions::from([("command".to_string(), "  ".to_string())]);
        assert_eq!(engine(&options), "pdflatex");
    }
}
