use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use acdc_parser::AttributeValue;
use clap::Parser;

use crate::error::{Error, Result};

const MAX_FILE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Parser)]
#[command(version, about)]
pub(super) struct Cli {
    /// Search and include roots, added to the current project directory by default. Repeat as needed.
    #[arg(long)]
    root: Vec<PathBuf>,
    /// Use only --root directories; do not add the current project directory.
    #[arg(long, requires = "root")]
    strictly_root_folders: bool,
    /// Document attribute: NAME=VALUE, NAME (set), or NAME! (unset). Repeat as needed.
    #[arg(short = 'a', long = "attribute", value_parser = parse_attribute)]
    attributes: Vec<(String, AttributeValue<'static>)>,
}

impl Cli {
    pub(super) fn into_config(mut self) -> Result<Config> {
        if !self.strictly_root_folders {
            let current_directory = match std::env::var_os("CLAUDE_PROJECT_DIR") {
                Some(directory) => PathBuf::from(directory),
                None => std::env::current_dir()?,
            };
            self.root.push(current_directory);
        }
        Config::new(self.root, self.attributes.into_iter().collect())
    }
}

fn parse_attribute(input: &str) -> std::result::Result<(String, AttributeValue<'static>), String> {
    let (name, value) = if let Some((name, value)) = input.split_once('=') {
        (name, AttributeValue::from(value.to_owned()))
    } else if let Some(name) = input.strip_suffix('!') {
        (name, AttributeValue::Bool(false))
    } else {
        (input, AttributeValue::Bool(true))
    };
    if name.is_empty() || name.chars().any(char::is_whitespace) {
        return Err("attribute names must be non-empty and contain no whitespace".into());
    }
    Ok((name.to_owned(), value))
}

#[derive(Clone, Debug)]
pub(super) struct Config {
    roots: Vec<PathBuf>,
    attributes: BTreeMap<String, AttributeValue<'static>>,
}

impl Config {
    fn new(
        roots: Vec<PathBuf>,
        attributes: BTreeMap<String, AttributeValue<'static>>,
    ) -> Result<Self> {
        let mut canonical = BTreeSet::new();
        for root in roots {
            let root = root.canonicalize()?;
            if !root.is_dir() {
                return Err(Error::Input(format!("not a directory: {}", root.display())));
            }
            canonical.insert(root);
        }
        if canonical.is_empty() {
            return Err(Error::Input("at least one --root is required".into()));
        }
        Ok(Self {
            roots: canonical.into_iter().collect(),
            attributes,
        })
    }

    pub(super) fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub(super) fn attributes(&self) -> &BTreeMap<String, AttributeValue<'static>> {
        &self.attributes
    }

    pub(super) fn check_path(&self, path: &Path) -> Result<PathBuf> {
        let canonical = path.canonicalize()?;
        if self.roots.iter().any(|root| canonical.starts_with(root)) {
            Ok(canonical)
        } else {
            Err(Error::Input(format!(
                "path is outside the configured roots: {}",
                path.display()
            )))
        }
    }

    pub(super) fn resolve(&self, input: &Path) -> Result<PathBuf> {
        if input.is_absolute() {
            return self.check_path(input);
        }
        let mut matches = BTreeSet::new();
        for root in &self.roots {
            let candidate = root.join(input);
            if candidate.try_exists()? {
                matches.insert(self.check_path(&candidate)?);
            }
        }
        match matches.len() {
            0 => Err(Error::Input(format!("path not found: {}", input.display()))),
            1 => Ok(matches.into_iter().next().expect("one path")),
            _ => Err(Error::Input(format!(
                "relative path matches multiple roots; use an absolute path: {}",
                input.display()
            ))),
        }
    }

    pub(super) fn read(&self, path: &Path) -> Result<(PathBuf, Vec<u8>)> {
        let path = self.check_path(path)?;
        if !path.metadata()?.is_file() {
            return Err(Error::Input(format!(
                "not a regular file: {}",
                path.display()
            )));
        }
        let file = File::open(&path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(Error::Input(format!(
                "not a regular file: {}",
                path.display()
            )));
        }
        if metadata.len() > MAX_FILE_BYTES as u64 {
            return Err(Error::Input(format!(
                "file exceeds 1 MiB: {}",
                path.display()
            )));
        }
        let mut bytes = Vec::new();
        file.take((MAX_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(Error::Input(format!(
                "file exceeds 1 MiB: {}",
                path.display()
            )));
        }
        if bytes.contains(&0) {
            return Err(Error::Input(format!("binary file: {}", path.display())));
        }
        Ok((path, bytes))
    }
}

pub(super) fn is_asciidoc(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["adoc", "asciidoc", "asc"]
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

#[cfg(test)]
mod tests {
    use acdc_parser::AttributeValue;
    use clap::Parser;

    use super::Cli;

    #[test]
    fn strictly_root_folders_requires_an_explicit_root() {
        let error = Cli::try_parse_from(["acdc-mcp", "--strictly-root-folders"]).unwrap_err();
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
        assert!(error.to_string().contains("--root"));
    }

    #[test]
    fn startup_attributes_preserve_exclamation_marks_in_values() {
        let cli = Cli::try_parse_from([
            "reader",
            "--root",
            ".",
            "-a",
            "product=Widget!",
            "-a",
            "enabled",
            "-a",
            "disabled!",
        ])
        .unwrap();
        assert!(
            matches!(&cli.attributes[0], (name, AttributeValue::String(value)) if name == "product" && value == "Widget!")
        );
        assert!(
            matches!(&cli.attributes[1], (name, AttributeValue::Bool(true)) if name == "enabled")
        );
        assert!(
            matches!(&cli.attributes[2], (name, AttributeValue::Bool(false)) if name == "disabled")
        );
        assert!(Cli::try_parse_from(["reader", "--root", ".", "-a", "=value"]).is_err());
    }
}
