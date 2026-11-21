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
        let generate = |scratch: &Path, output: &Path| {
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
        };
        if format == Format::Txt {
            let render = || -> io::Result<String> {
                let scratch = asset::scratch_dir(roots.embedded)?;
                let output = scratch.path().join("output.txt");
                generate(scratch.path(), &output)?;
                asset::read_text(&output)
            };
            return render()
                .map(BlockBuilder::literal)
                .map_err(|e| format!("{}: {e}", self.name));
        }
        image(
            self.name,
            roots,
            input,
            format,
            identity,
            self.attributes.contains(&"scale"),
            generate,
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
    fn flags(&self, cmd: &mut Command, flags: &[(&str, &str)]) {
        for &(attribute, flag) in flags {
            if let Some(value) = self.option(attribute) {
                cmd.arg(flag).arg(value);
            }
        }
    }

    fn source_file(&self, extension: &str) -> io::Result<std::path::PathBuf> {
        let path = self.scratch.join(format!("input.{extension}"));
        std::fs::write(&path, self.source)?;
        Ok(path)
    }
    fn run(&self, command: &mut Command) -> io::Result<()> {
        asset::run(command, self.dir, self.name)
    }

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

/// Render `actdiag` diagrams using `actdiag`.
pub const ACTDIAG: Renderer = Renderer {
    name: "actdiag",
    tool: "actdiag",
    formats: &[Format::Png, Format::Pdf, Format::Svg],
    attributes: &["fontpath"],
    generate: blockdiag_family,
};

/// Render `nwdiag` diagrams using `nwdiag`.
pub const NWDIAG: Renderer = Renderer {
    name: "nwdiag",
    tool: "nwdiag",
    formats: &[Format::Png, Format::Pdf, Format::Svg],
    attributes: &["fontpath"],
    generate: blockdiag_family,
};

/// Render `packetdiag` diagrams using `packetdiag`.
pub const PACKETDIAG: Renderer = Renderer {
    name: "packetdiag",
    tool: "packetdiag",
    formats: &[Format::Png, Format::Pdf, Format::Svg],
    attributes: &["fontpath"],
    generate: blockdiag_family,
};

/// Render `rackdiag` diagrams using `rackdiag`.
pub const RACKDIAG: Renderer = Renderer {
    name: "rackdiag",
    tool: "rackdiag",
    formats: &[Format::Png, Format::Pdf, Format::Svg],
    attributes: &["fontpath"],
    generate: blockdiag_family,
};

fn nomnoml(job: &Job<'_>) -> io::Result<()> {
    let input = job.source_file("nomnoml")?;
    job.run(Command::new(job.tool).arg(input).arg(job.output))
}

/// Render `nomnoml` diagrams using `nomnoml`.
pub const NOMNOML: Renderer = Renderer {
    name: "nomnoml",
    tool: "nomnoml",
    formats: &[Format::Svg],
    attributes: &[],
    generate: nomnoml,
};

fn smcat(job: &Job<'_>) -> io::Result<()> {
    let mut cmd = Command::new(job.tool);
    cmd.arg("-o")
        .arg(job.output)
        .args(["-T", job.format.extension()]);
    job.flags(&mut cmd, &[("direction", "-d"), ("engine", "-E")]);
    job.stdin(cmd.arg("-"), None)
}

/// Render `smcat` diagrams using `smcat`.
pub const SMCAT: Renderer = Renderer {
    name: "smcat",
    tool: "smcat",
    formats: &[Format::Svg],
    attributes: &["direction", "engine"],
    generate: smcat,
};

fn dbml(job: &Job<'_>) -> io::Result<()> {
    job.stdin(&mut Command::new(job.tool), Some(job.output))
}

/// Render `dbml` diagrams using `dbml-renderer`.
pub const DBML: Renderer = Renderer {
    name: "dbml",
    tool: "dbml-renderer",
    formats: &[Format::Svg],
    attributes: &[],
    generate: dbml,
};

fn graphviz_py(job: &Job<'_>) -> io::Result<()> {
    let mut out = std::ffi::OsString::from("-o");
    out.push(job.output);
    let mut cmd = Command::new(job.tool);
    cmd.arg(out).arg(format!("-T{}", job.format.extension()));
    if let Some(layout) = job.option("layout") {
        cmd.arg(format!("-K{layout}"));
    }
    job.flags(&mut cmd, &[("argument", "-a")]);
    job.stdin(&mut cmd, None)
}

/// Render `graphviz_py` diagrams using `graphviz-py`.
pub const GRAPHVIZ_PY: Renderer = Renderer {
    name: "graphviz_py",
    tool: "graphviz-py",
    formats: &[Format::Png, Format::Pdf, Format::Svg],
    attributes: &["layout", "argument"],
    generate: graphviz_py,
};

fn d2(job: &Job<'_>) -> io::Result<()> {
    let mut cmd = Command::new(job.tool);
    cmd.args(["--browser", "false"]);
    for flag in [
        "layout",
        "theme",
        "dark-theme",
        "pad",
        "animate-interval",
        "font-regular",
        "font-italic",
        "font-bold",
    ] {
        if let Some(value) = job.option(flag) {
            cmd.arg(format!("--{flag}")).arg(value);
        }
    }
    if job.option("sketch").is_some_and(|v| v != "false") {
        cmd.arg("--sketch");
    }
    job.stdin(cmd.arg("-").arg(job.output), None)
}

/// Render `d2` diagrams using `d2`.
pub const D2: Renderer = Renderer {
    name: "d2",
    tool: "d2",
    formats: &[Format::Svg, Format::Png, Format::Pdf],
    attributes: &[
        "layout",
        "theme",
        "dark-theme",
        "pad",
        "animate-interval",
        "font-regular",
        "font-italic",
        "font-bold",
        "sketch",
    ],
    generate: d2,
};

fn shaape(job: &Job<'_>) -> io::Result<()> {
    job.stdin(
        Command::new(job.tool)
            .arg("-o")
            .arg(job.output)
            .args(["-t", job.format.extension(), "-"]),
        None,
    )
}

/// Render `shaape` diagrams using `shaape`.
pub const SHAAPE: Renderer = Renderer {
    name: "shaape",
    tool: "shaape",
    formats: &[Format::Png, Format::Svg],
    attributes: &[],
    generate: shaape,
};

fn symbolator(job: &Job<'_>) -> io::Result<()> {
    let mut out = std::ffi::OsString::from("-o");
    out.push(job.output);
    job.stdin(
        Command::new(job.tool)
            .arg("-i-")
            .arg(out)
            .arg(format!("-f{}", job.format.extension())),
        None,
    )
}

/// Render `symbolator` diagrams using `symbolator`.
pub const SYMBOLATOR: Renderer = Renderer {
    name: "symbolator",
    tool: "symbolator",
    formats: &[Format::Png, Format::Pdf, Format::Svg],
    attributes: &[],
    generate: symbolator,
};

fn pintora(job: &Job<'_>) -> io::Result<()> {
    let path = job.source_file("pintora")?;
    let mut cmd = Command::new(job.tool);
    cmd.args(["render", "-i"])
        .arg(path)
        .arg("-o")
        .arg(job.output);
    job.flags(
        &mut cmd,
        &[
            ("width", "-w"),
            ("theme", "-t"),
            ("pixel-ratio", "-p"),
            ("background-color", "-b"),
        ],
    );
    job.run(&mut cmd)
}

/// Render `pintora` diagrams using `pintora`.
pub const PINTORA: Renderer = Renderer {
    name: "pintora",
    tool: "pintora",
    formats: &[Format::Png, Format::Svg],
    attributes: &["width", "theme", "background-color", "pixel-ratio"],
    generate: pintora,
};

fn a2s(job: &Job<'_>) -> io::Result<()> {
    if job.format == Format::Txt {
        return std::fs::write(job.output, job.source);
    }
    let mut cmd = Command::new(job.tool);
    cmd.arg("-o").arg(job.output);
    let axes = job
        .option("scalex")
        .zip(job.option("scaley"))
        .or_else(|| job.option("scale").map(|s| (s, s)));
    if let Some((x, y)) = axes {
        cmd.args(["-s", &format!("{x},{y}")]);
    }
    if job.option("noblur") == Some("true") {
        cmd.arg("-b");
    }
    job.flags(&mut cmd, &[("fontfamily", "-f")]);
    // a2s counts a terminating newline as an extra row of the canvas.
    let source = job
        .source
        .strip_suffix("\r\n")
        .or_else(|| job.source.strip_suffix('\n'))
        .unwrap_or(job.source);
    asset::run_with_source(&mut cmd, job.dir, job.name, job.scratch, source, None)
}

/// Render `a2s` diagrams using `a2s`.
pub const A2S: Renderer = Renderer {
    name: "a2s",
    tool: "a2s",
    formats: &[Format::Svg, Format::Txt],
    attributes: &["scalex", "scaley", "scale", "noblur", "fontfamily"],
    generate: a2s,
};

fn svgbob(job: &Job<'_>) -> io::Result<()> {
    if job.format == Format::Txt {
        return std::fs::write(job.output, job.source);
    }
    let mut cmd = Command::new(job.tool);
    cmd.arg("-o").arg(job.output);
    job.flags(
        &mut cmd,
        &[
            ("font-family", "--font-family"),
            ("font-size", "--font-size"),
            ("stroke-width", "--stroke-width"),
            ("scale", "--scale"),
        ],
    );
    job.stdin(&mut cmd, None)
}

/// Render `svgbob` diagrams using `svgbob`.
pub const SVGBOB: Renderer = Renderer {
    name: "svgbob",
    tool: "svgbob",
    formats: &[Format::Svg, Format::Txt],
    attributes: &["font-family", "font-size", "stroke-width", "scale"],
    generate: svgbob,
};

fn syntrax(job: &Job<'_>) -> io::Result<()> {
    let input = job.source_file("spec")?;
    let mut cmd = Command::new(job.tool);
    cmd.arg("-i").arg(input).arg("-o").arg(job.output);
    job.flags(
        &mut cmd,
        &[
            ("heading", "--title"),
            ("scale", "--scale"),
            ("style-file", "--style"),
        ],
    );
    if job.option("transparent") == Some("true") {
        cmd.arg("--transparent");
    }
    job.run(&mut cmd)
}

/// Render `syntrax` diagrams using `syntrax`.
pub const SYNTRAX: Renderer = Renderer {
    name: "syntrax",
    tool: "syntrax",
    formats: &[Format::Png, Format::Svg],
    attributes: &["heading", "scale", "transparent", "style-file"],
    generate: syntrax,
};
