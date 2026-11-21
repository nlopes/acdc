//! `TikZ` standalone-document adapter ported from acdc-diagram (PR #451).

use std::{fs, io, path::Path, process::Command};

use acdc_parser::{DocumentAttributeValue, extensions::BlockStyle};

use super::{
    OutputDirectories,
    asset::{self, Format},
    diagram_source, executable, image, requested_format, style,
};

/// Register the `tikz` style for listing and literal blocks in trusted documents.
/// The caller chooses the `LuaLaTeX` executable and optionally a PDF-to-SVG
/// converter implementing the `pdf2svg INPUT OUTPUT` interface. Both run
/// in a private scratch directory. `TeX` shell escape is disabled.
///
/// # Errors
/// Returns an error if the supplied tools or output directory cannot be resolved.
pub fn tikz<'a>(
    lualatex: &Path,
    pdf2svg: Option<&Path>,
    output_dir: impl Into<OutputDirectories<'a>>,
) -> io::Result<BlockStyle> {
    // TeX selects its format from argv[0]. Preserve the lualatex symlink name.
    let engine = executable(std::path::absolute(lualatex)?)?;
    let converter = pdf2svg
        .map(|tool| executable(tool.canonicalize()?))
        .transpose()?;
    style("TikZ", output_dir.into(), move |root, input| {
        let attrs = input.document_attributes;
        let args = &input.metadata.attributes;
        for key in ["lualatex", "pdflatex", "pdf2svg"] {
            if args.get(key).is_some() || attrs.get(key).is_some() {
                return Err(format!(
                    "set the {key} executable when registering the TikZ style"
                ));
            }
        }
        let command = args.get_string("command");
        for value in [
            command.as_deref(),
            attrs
                .get("tikz-command")
                .and_then(DocumentAttributeValue::text),
        ]
        .into_iter()
        .flatten()
        {
            if value != "lualatex" {
                return Err("TikZ supports only command=lualatex".into());
            }
        }
        let format = match requested_format(input, "tikz-format").as_deref() {
            Some("svg") => Format::Svg,
            Some("pdf") => Format::Pdf,
            Some(other) => {
                return Err(format!(
                    "unsupported TikZ format '{other}' (expected svg or pdf)"
                ));
            }
            None if attrs
                .get("backend")
                .and_then(DocumentAttributeValue::text)
                .is_some_and(|backend| {
                    backend == "pdf" || (backend.starts_with("html") && converter.is_some())
                }) =>
            {
                Format::Svg
            }
            None => Format::Pdf,
        };
        let svg_tool = if format == Format::Svg {
            Some(
                converter
                    .as_ref()
                    .ok_or("TikZ SVG output requires a configured pdf2svg executable")?,
            )
        } else {
            None
        };
        let preamble = args.get_string("preamble").as_deref() == Some("true")
            || (&input.metadata.options)
                .into_iter()
                .any(|opt| *opt == "preamble");
        let document = latex_document(&diagram_source(input), preamble);
        image("tikz", root, input, format, &document, |scratch, output| {
            fs::write(scratch.join("input.tex"), &document)?;
            asset::run(
                Command::new(&engine).args([
                    "-no-shell-escape",
                    "-halt-on-error",
                    "-file-line-error",
                    "-interaction=nonstopmode",
                    "input.tex",
                ]),
                scratch,
                "lualatex",
            )?;
            let pdf = scratch.join("input.pdf");
            if let Some(tool) = svg_tool {
                Format::Pdf.validate(&pdf)?;
                asset::run(Command::new(tool).arg(pdf).arg(output), scratch, "pdf2svg")
            } else {
                fs::rename(pdf, output)
            }
        })
    })
}

fn latex_document(source: &str, has_preamble: bool) -> String {
    let (mut preamble, mut body) = ("", source);
    if has_preamble {
        let mut offset = 0;
        for line in source.split_inclusive('\n') {
            if line.trim_end_matches(['\r', '\n']) == "~~~~" {
                (preamble, _) = source.split_at(offset);
                (_, body) = source.split_at(offset + line.len());
                break;
            }
            offset += line.len();
        }
    }
    format!(
        "\\documentclass[border=2bp, tikz]{{standalone}}\n\\usepackage{{tikz}}\n{preamble}\\begin{{document}}\n\\begingroup\n\\tikzset{{every picture/.style={{scale=1}}}}\n{body}\n\\endgroup\n\\end{{document}}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::latex_document;

    #[test]
    fn a_preamble_requires_a_separator_on_its_own_line() {
        let body = "% an inline ~~~~ stays in the picture
\\draw (0,0) -- (1,1);";
        assert_eq!(latex_document(body, true), latex_document(body, false));
        assert_eq!(
            latex_document(&format!("~~~~\n{body}"), true),
            latex_document(body, false)
        );
        assert!(latex_document("first\n~~~~\nnext", false).contains("first\n~~~~\nnext"));
    }
}
