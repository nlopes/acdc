//! Driving the diagram tools that ship as Java archives.
//!
//! asciidoctor-diagram bundles `PlantUML`, `Ditaa`, `JSyntrax` and `Structurizr` as
//! jars inside the gem and talks to a long-lived helper server. acdc has no
//! jars to bundle, so it prefers whatever native launcher the tool ships with
//! and otherwise runs `java -jar` against a jar the document (or the
//! environment) points at.

use std::path::{Path, PathBuf};

use crate::{
    cli::CommandSpec,
    error::{Error, Result},
    paths,
    source::{CommandLookup, DiagramSource},
    which::{is_usable_command_path, which},
};

/// How a Java-based tool will be launched.
pub(crate) enum Launcher {
    /// A native launcher script or binary found on `PATH`.
    Native(PathBuf),
    /// `java -jar <archive>`.
    Jar {
        /// The `java` executable.
        java: PathBuf,
        /// The archive to run.
        archive: PathBuf,
    },
}

impl Launcher {
    /// The executable that will actually be spawned, for scratch-file naming.
    pub(crate) fn program(&self) -> &Path {
        match self {
            Launcher::Native(path) => path,
            Launcher::Jar { java, .. } => java,
        }
    }

    /// Start a command line for this launcher.
    pub(crate) fn spec(&self) -> CommandSpec {
        match self {
            Launcher::Native(path) => CommandSpec::new(path),
            Launcher::Jar { java, archive } => CommandSpec::new(java)
                .arg("-jar")
                .arg(crate::platform::native_path(archive)),
        }
    }
}

/// Find a native launcher, falling back to a jar.
///
/// `commands` are the native launcher names to look for, `attributes` are the
/// document attributes that may point at either a launcher or a jar, and
/// `env_var` is the classpath-style environment variable asciidoctor-diagram
/// uses for the same tool.
///
/// # Errors
///
/// Returns [`Error::CommandNotFound`] when no launcher and no jar can be
/// found.
pub(crate) fn resolve(
    source: &DiagramSource<'_>,
    commands: &[&str],
    jar_attributes: &[&str],
    env_var: &str,
) -> Result<Launcher> {
    if let Some(native) = source.find_command_opt(&CommandLookup::new(commands)) {
        // A `.jar` named by the tool attribute is not something we can exec.
        if native
            .extension()
            .is_none_or(|extension| extension != "jar")
        {
            return Ok(Launcher::Native(native));
        }
        return Ok(Launcher::Jar {
            java: java_command(source)?,
            archive: native,
        });
    }

    if let Some(archive) = find_archive(source, jar_attributes, env_var) {
        return Ok(Launcher::Jar {
            java: java_command(source)?,
            archive,
        });
    }

    Err(Error::CommandNotFound {
        commands: commands.iter().map(|c| (*c).to_string()).collect(),
        attribute: jar_attributes
            .first()
            .or_else(|| commands.first())
            .map_or_else(String::new, |name| (*name).to_string()),
    })
}

/// Locate the jar named by a document attribute or environment variable.
fn find_archive(source: &DiagramSource<'_>, attributes: &[&str], env_var: &str) -> Option<PathBuf> {
    for attribute in attributes {
        if let Some(value) = source.doc_attr(attribute) {
            let candidate = paths::resolve(Path::new(&value), source.base_dir());
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    // The environment variable is a path list, as the gem's own
    // `DIAGRAM_*_CLASSPATH` variables are; the first entry that exists wins.
    let value = std::env::var_os(env_var)?;
    std::env::split_paths(&value).find(|entry| entry.is_file())
}

/// Locate a `java` executable.
///
/// # Errors
///
/// Returns [`Error::CommandNotFound`] when neither `JAVA_HOME` nor `PATH`
/// yields one.
pub(crate) fn java_command(source: &DiagramSource<'_>) -> Result<PathBuf> {
    if let Some(value) = source.doc_attr("java") {
        let candidate = paths::resolve(Path::new(&value), source.base_dir());
        if is_usable_command_path(&candidate) {
            return Ok(candidate);
        }
    }
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        let candidate = PathBuf::from(home).join("bin").join("java");
        if is_usable_command_path(&candidate) {
            return Ok(candidate);
        }
    }
    which("java", &[]).ok_or_else(|| Error::CommandNotFound {
        commands: vec!["java".to_string()],
        attribute: "java".to_string(),
    })
}
