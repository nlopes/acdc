use std::{
    io::Write,
    path::{Component, Path},
};

use acdc_pdf_theme::Theme;

use crate::{Error, MAX_THEME_FILE_BYTES, Processor};

impl Processor<'_> {
    pub(super) fn validate_theme_config_paths(
        &self,
        source_file: Option<&Path>,
        output_path: Option<&Path>,
    ) -> Result<(), Error> {
        let Some(path) = &self.pdf_options.emit_theme_config else {
            return Ok(());
        };
        for other in source_file
            .into_iter()
            .chain(output_path)
            .chain(self.pdf_options.theme.as_deref())
            .chain(self.pdf_options.emit_typst.as_deref())
        {
            if paths_match(path, other) {
                return Err(Error::ThemeConfigPathCollision {
                    path: path.clone(),
                    other: other.to_path_buf(),
                });
            }
        }
        Ok(())
    }

    pub(super) fn write_loaded_theme_config(path: &Path, theme: &Theme) -> Result<(), Error> {
        let yaml = theme
            .to_yaml_string()
            .map_err(|source| Error::ThemeConfigSerialize {
                path: path.to_path_buf(),
                source,
            })?;
        let write = || -> Result<(), std::io::Error> {
            if yaml.len() > MAX_THEME_FILE_BYTES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "resolved theme exceeds the 1 MiB theme input limit; reduce repeated values so the exported file can be loaded",
                ));
            }
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            // Replace the destination only after the complete YAML is written.
            let mut output = tempfile::NamedTempFile::new_in(parent)?;
            output.write_all(yaml.as_bytes())?;
            output.persist(path).map_err(|error| error.error)?;
            Ok(())
        };
        write().map_err(|source| Error::ThemeConfigWrite {
            path: path.to_path_buf(),
            source,
        })
    }
}

fn paths_match(first: &Path, second: &Path) -> bool {
    let resolved = |path: &Path| {
        path.canonicalize().ok().or_else(|| {
            // Compare destinations before their parent directories exist.
            path.ancestors().find_map(|ancestor| {
                let base = if ancestor.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    ancestor
                };
                let mut resolved = base.canonicalize().ok()?;
                for component in path.strip_prefix(ancestor).ok()?.components() {
                    match component {
                        Component::CurDir => {}
                        Component::ParentDir => {
                            resolved.pop();
                        }
                        Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                            resolved.push(component.as_os_str());
                        }
                    }
                }
                Some(resolved)
            })
        })
    };
    first == second
        || matches!((resolved(first), resolved(second)), (Some(first), Some(second)) if first == second)
}
