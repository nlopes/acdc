//! External renderers registered as listing styles and local file macros.
use std::{borrow::Cow, io, path::Path, process::Command};

use super::{
    DiagramInput, OutputDirectories,
    asset::{self, Format},
    diagram_attribute, diagram_source, executable, file_macro, image, style,
};
use acdc_parser::extensions::{BlockBuilder, BlockMacro, BlockStyle};

/// A diagram language and the command line used to render it.
#[derive(Clone, Copy)]
pub struct Renderer {
    name: &'static str,
    tool: &'static str,
    formats: &'static [Format],
    attributes: &'static [&'static str],
    generate: fn(&Job<'_>) -> io::Result<()>,
}

impl Renderer {
    /// Bind a listing style and local file macro to a caller-chosen executable.
    ///
    /// # Errors
    /// Returns an error if the executable or output directory cannot be resolved.
    pub fn processors<'a>(
        self,
        tool: &Path,
        output: impl Into<OutputDirectories<'a>>,
    ) -> io::Result<(BlockStyle, BlockMacro)> {
        let tool = executable(std::path::absolute(tool)?)?;
        let output = output.into();
        let block_tool = tool.clone();
        let block = style(self.name, output, move |roots, input| {
            self.render(&block_tool, roots.embedded, roots, input)
        })?;
        let file = file_macro(
            self.name,
            output,
            self.attributes,
            move |dir, roots, input| self.render(&tool, dir, roots, input),
        )?;
        Ok((block, file))
    }

    fn render(
        self,
        tool: &Path,
        dir: &Path,
        roots: OutputDirectories<'_>,
        input: &DiagramInput<'_>,
    ) -> Result<BlockBuilder, String> {
        if input.metadata.attributes.get(self.tool).is_some()
            || input.document_attributes.get(self.tool).is_some()
        {
            return Err(format!(
                "set the {} executable when registering the diagram",
                self.tool
            ));
        }
        let selected = diagram_attribute(input, self.name, "format");
        let format = match selected.as_deref() {
            None => *self
                .formats
                .first()
                .ok_or("renderer has no output format")?,
            Some(name) => *self
                .formats
                .iter()
                .find(|format| format.extension() == name)
                .ok_or_else(|| {
                    format!(
                        "unsupported {} format '{name}' (expected {})",
                        self.name,
                        self.formats
                            .iter()
                            .map(|f| f.extension())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })?,
        };
        let options: Vec<_> = self
            .attributes
            .iter()
            .filter_map(|&key| diagram_attribute(input, self.name, key).map(|value| (key, value)))
            .collect();
        let source = diagram_source(input);
        let identity = options
            .iter()
            .flat_map(|(k, v)| [*k, "\0", v.as_ref(), "\0"])
            .chain([source.as_ref()]);
        image(
            self.name,
            roots,
            input,
            format,
            identity,
            |scratch, output| {
                (self.generate)(&Job {
                    tool,
                    name: self.tool,
                    dir,
                    format,
                    source: &source,
                    options: &options,
                    scratch,
                    output,
                })
            },
        )
    }
}

struct Job<'a> {
    tool: &'a Path,
    name: &'static str,
    dir: &'a Path,
    format: Format,
    source: &'a str,
    options: &'a [(&'static str, Cow<'a, str>)],
    scratch: &'a Path,
    output: &'a Path,
}

impl Job<'_> {
    fn option(&self, key: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.as_ref())
    }

    fn stdin(&self, cmd: &mut Command, stdout: Option<&Path>) -> io::Result<()> {
        asset::run_with_source(cmd, self.dir, self.name, self.scratch, self.source, stdout)
    }
}

fn blockdiag_family(job: &Job<'_>) -> io::Result<()> {
    let mut command = Command::new(job.tool);
    command
        .args(["-a", "-o"])
        .arg(job.output)
        .arg(format!("-T{}", job.format.extension()));
    if let Some(font) = job.option("fontpath") {
        command.arg(format!("-f{font}"));
    }
    job.stdin(command.arg("-"), None)
}

/// Render `blockdiag` diagrams using `blockdiag`.
pub const BLOCKDIAG: Renderer = Renderer {
    name: "blockdiag",
    tool: "blockdiag",
    formats: &[Format::Png, Format::Pdf, Format::Svg],
    attributes: &["fontpath"],
    generate: blockdiag_family,
};

/// Render `seqdiag` diagrams using `seqdiag`.
pub const SEQDIAG: Renderer = Renderer {
    name: "seqdiag",
    tool: "seqdiag",
    formats: &[Format::Png, Format::Pdf, Format::Svg],
    attributes: &["fontpath"],
    generate: blockdiag_family,
};
