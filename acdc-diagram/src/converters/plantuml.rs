//! `PlantUML`, and the `salt` wireframe dialect that shares its implementation.
//!
//! Both run the same tool; only the `@start…`/`@end…` tag the body gets
//! wrapped in differs. The diagram code is run through `PlantUML`'s own
//! preprocessor before it is hashed, so that a change to an `!include`d file
//! invalidates the cache even though the block itself is untouched.

use crate::{
    Format,
    cli::CommandSpec,
    converters::{
        ConverterOptions, DiagramConverter, Generated,
        java::{self, Launcher},
        option, set_option,
    },
    error::{Error, Result},
    generate,
    source::{CommandLookup, DiagramSource},
};

/// Native launcher names, most specific first.
const COMMANDS: &[&str] = &["plantuml", "plantuml-full", "plantuml-headless"];
/// Attributes that may name a launcher or a jar.
const ATTRIBUTES: &[&str] = &["plantuml-native", "plantumljar", "plantuml"];
/// The classpath variable asciidoctor-diagram uses for the same tool.
const CLASSPATH_ENV: &str = "DIAGRAM_PLANTUML_CLASSPATH";

/// `PlantUML`, in one of its dialects.
pub(crate) struct PlantUml {
    /// The `@start…`/`@end…` tag: `uml` for `PlantUML`, `salt` for salt.
    tag: &'static str,
}

impl PlantUml {
    pub(crate) fn new(tag: &'static str) -> Self {
        Self { tag }
    }

    /// Arguments every invocation needs: where relative includes resolve,
    /// which config file to load, and where `!include` should search.
    fn common_args(source: &DiagramSource<'_>, mut spec: CommandSpec) -> CommandSpec {
        let base_dir = source.base_dir().to_path_buf();
        spec = spec.arg("-filedir").arg(generate::native(&base_dir));

        if let Some(config) = source
            .doc_attr("plantumlconfig")
            .or_else(|| source.attr(&["config"]))
        {
            spec = spec
                .arg("-config")
                .arg(generate::native(&source.resolve_path(&config, None)));
        }
        if let Some(include_dir) = source.attr(&["includedir"]) {
            spec = spec.arg(format!(
                "-Dplantuml.include.path={}",
                generate::native(&source.resolve_path(&include_dir, None))
            ));
        }
        spec
    }

    /// Locate `PlantUML`, or report that nothing was found.
    fn launcher(source: &DiagramSource<'_>) -> Result<Launcher> {
        java::resolve(source, COMMANDS, ATTRIBUTES, CLASSPATH_ENV)
    }
}

impl DiagramConverter for PlantUml {
    fn supported_formats(&self) -> &'static [Format] {
        &[
            Format::Png,
            Format::Svg,
            Format::Txt,
            Format::Atxt,
            Format::Utxt,
        ]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        let mut options = ConverterOptions::new();
        options.insert(
            "size-limit".to_string(),
            source
                .attr(&["size-limit"])
                .unwrap_or_else(|| "4096".to_string()),
        );
        set_option(&mut options, "theme", source.attr(&["theme"]));
        if source.opt("smetana") {
            options.insert("smetana".to_string(), "true".to_string());
        }
        if source.opt("debug") {
            options.insert("debug".to_string(), "true".to_string());
        }
        Ok(options)
    }

    fn prepare(&self, source: &mut DiagramSource<'_>) -> Result<()> {
        let code = source.code();
        let wrapped = if code.contains("@start") && code.contains("@end") {
            code.to_string()
        } else {
            format!("@start{tag}\n{code}\n@end{tag}", tag = self.tag)
        };
        source.set_code(wrapped);

        if source.attr(&["preprocess"]).as_deref() == Some("false") {
            return Ok(());
        }
        // Preprocessing costs a whole JVM start-up, and the gem only gets away
        // with it because it keeps a helper server alive. A diagram that uses
        // none of the preprocessor's syntax cannot be changed by it, so it is
        // skipped; `preprocess=false` remains the way to skip it outright.
        if !source.code().contains(['!', '$', '%']) {
            return Ok(());
        }
        // Preprocessing folds `!include`d files into the code, which is what
        // makes the checksum notice when one of them changes. It is skipped
        // silently when PlantUML is missing so that the far more useful "tool
        // not found" error comes from the render step instead.
        let Ok(launcher) = Self::launcher(source) else {
            return Ok(());
        };
        let theme = source.attr(&["theme"]);
        let expanded =
            generate::stdin_to_stdout(launcher.program(), source.code().as_bytes(), |_| {
                let mut spec =
                    launcher
                        .spec()
                        .args(["-pipe", "-preproc", "-failfast2", "-stdrpt:1"]);
                spec = Self::common_args(source, spec);
                if let Some(theme) = &theme {
                    spec = spec.args(["-theme", theme]);
                }
                spec
            })?;
        let expanded = String::from_utf8(expanded).map_err(|_| {
            Error::Image("PlantUML preprocessor output is not valid UTF-8".to_string())
        })?;
        source.set_code(expanded);
        Ok(())
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let launcher = Self::launcher(source)?;

        let format_flag = match format {
            Format::Png => "-tpng",
            Format::Svg => "-tsvg",
            // PlantUML's `-tutxt` is the Unicode renderer and `-ttxt` the
            // ASCII-only one; acdc's `txt` follows `utxt`, as the gem does.
            Format::Txt | Format::Utxt => "-tutxt",
            Format::Atxt => "-ttxt",
            Format::Pdf | Format::Gif | Format::Jpeg => {
                return Err(Error::UnsupportedFormat {
                    diagram: source.diagram_type().to_string(),
                    format: format.to_string(),
                    supported: Format::list(self.supported_formats()),
                });
            }
        };

        // Smetana is PlantUML's built-in layout engine; without it PlantUML
        // needs Graphviz, so fall back to it when `dot` is missing rather than
        // failing.
        let dot = source
            .find_command_opt(&CommandLookup::new(&["dot"]).with_attributes(&["graphvizdot"]));
        let use_smetana = option(options, "smetana").is_some() || dot.is_none();

        generate::stdin_to_stdout(launcher.program(), source.code().as_bytes(), |_| {
            let mut spec = launcher.spec().arg(format_flag);
            spec = Self::common_args(source, spec);
            if let Some(theme) = option(options, "theme") {
                spec = spec.args(["-theme", theme]);
            }
            if use_smetana {
                spec = spec.arg("-Playout=smetana");
            } else if let Some(dot) = &dot {
                spec = spec.arg("-graphvizdot").arg(generate::native(dot));
            }
            if let Some(limit) = option(options, "size-limit") {
                spec = spec.env("PLANTUML_LIMIT_SIZE", limit);
            }
            spec.args(["-pipe", "-failfast2", "-stdrpt:1"])
        })
        .map(Generated::from)
    }
}
