//! Translate supported Asciidoctor PDF 2.3.15 settings into acdc design tokens.

mod expression;
mod mapping;
mod value;

use std::collections::{BTreeMap, BTreeSet};

use crate::{Error, Theme};
use mapping::{apply, font_size, set_font, warn};
use value::Value;

const BASE: &str = include_str!("../assets/asciidoctor/base-theme.yml");
const DEFAULT: &str = include_str!("../assets/asciidoctor/default-theme.yml");

/// A setting that cannot be preserved exactly by the PDF converter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompatibilityWarning {
    /// Original dotted YAML key path, or `extends` for a built-in theme.
    pub key: String,
    /// The effect on the converted theme.
    pub message: String,
}

/// A validated native theme and warnings about its source settings.
#[derive(Clone, Debug, PartialEq)]
pub struct ConvertedTheme {
    /// Native design tokens used by the renderer.
    pub theme: Theme,
    /// Settings that were omitted or could only be approximated.
    pub warnings: Vec<CompatibilityWarning>,
}

/// Convert an Asciidoctor PDF theme without accessing files or the network.
///
/// Nested, flat, underscore, and hyphen keys are accepted. References use
/// earlier assignments. Numeric arithmetic and `round`, `floor`, and `ceil`
/// are supported. Only the pinned `base` and `default` built-in themes can
/// be extended. Unsupported settings produce warnings with their input paths.
/// Supply custom fonts through the converter's font directories.
///
/// # Errors
/// Returns an error for invalid YAML, invalid values of supported settings,
/// or a converted theme that fails native validation.
pub fn from_yaml_str(yaml: &str) -> Result<ConvertedTheme, Error> {
    let value = value::parse(yaml)?;
    let mut loader = Loader {
        variables: BTreeMap::from([(
            "base_font_size".to_owned(),
            Value::Number("12".to_owned(), None),
        )]),
        ..Loader::default()
    };
    loader.load(&value, false)?;
    let mut result = ConvertedTheme {
        theme: Theme::default(),
        warnings: loader.warnings,
    };
    // Font-size ratios depend on the final body size, not on key sort order.
    if let Some(entry) = loader.entries.remove("base_font_size") {
        result.theme.typography.body_size_pt = font_size(&entry, 12.0)?;
    } else {
        result.theme.typography.body_size_pt = 12.0;
    }
    for (key, entry) in &loader.entries {
        apply(&mut result, key, entry)?;
    }
    if !loader.entries.contains_key("base_font_color") {
        "#000000".clone_into(&mut result.theme.palette.body_text);
    }
    if !loader.entries.contains_key("heading_font_color") {
        result
            .theme
            .palette
            .heading
            .clone_from(&result.theme.palette.body_text);
    }
    if !loader.entries.contains_key("heading_font_family") {
        result.theme.typography.heading_font = result.theme.typography.body_font.clone();
    }
    if !loader.entries.contains_key("code_font_family")
        && let Some(entry) = loader.entries.get("codespan_font_family")
    {
        set_font(&mut result, "code_font_family", entry)?;
    }
    // Both sizes must be resolved before checking their relationship.
    if result.theme.typography.code_min_size_em > result.theme.typography.code_size_em {
        if let Some(entry) = loader.entries.get("code_font_size_min") {
            return Err(Error::validation(
                &entry.path,
                "minimum code size must not exceed the normal code size",
            ));
        }
        result.theme.typography.code_min_size_em = result.theme.typography.code_size_em;
        if let Some(entry) = loader.entries.get("code_font_size") {
            warn(
                &mut result,
                entry,
                "acdc's minimum autofit size was reduced to the configured code size",
            );
        }
    }
    result.theme.palette.normalize()?;
    result.theme.caption.normalize()?;
    result.theme.table.normalize()?;
    result.theme.validate()?;
    Ok(result)
}

#[derive(Clone)]
struct Entry {
    path: String,
    value: Value,
    inherited: bool,
}

#[derive(Default)]
struct Loader {
    variables: BTreeMap<String, Value>,
    entries: BTreeMap<String, Entry>,
    loaded: BTreeSet<String>,
    warnings: Vec<CompatibilityWarning>,
}

impl Loader {
    fn warn(&mut self, key: &str, message: impl Into<String>, inherited: bool) {
        if !inherited {
            self.warnings.push(CompatibilityWarning {
                key: key.to_owned(),
                message: message.into(),
            });
        }
    }

    fn load(&mut self, value: &Value, inherited: bool) -> Result<(), Error> {
        let Value::Mapping(mapping) = value else {
            return Err(Error::validation("$", "expected a YAML mapping"));
        };
        if let Some((_, extends)) = mapping.iter().find(|(key, _)| key == "extends") {
            self.extend(extends, inherited)?;
        }
        for (key, value) in mapping.iter().filter(|(key, _)| key != "extends") {
            self.flatten(&key.replace('-', "_"), key, value, inherited)?;
        }
        Ok(())
    }

    fn extend(&mut self, value: &Value, inherited: bool) -> Result<(), Error> {
        let values = match value {
            Value::Null => return Ok(()),
            Value::Sequence(values) => values.as_slice(),
            Value::Bool | Value::Number(..) | Value::String(_) | Value::Mapping(_) => {
                std::slice::from_ref(value)
            }
        };
        for value in values {
            let Value::String(name) = value else {
                return Err(Error::validation(
                    "extends",
                    "expected a theme name or a list of theme names",
                ));
            };
            let (name, force) = name
                .strip_suffix(" !important")
                .map_or((name.as_str(), false), |name| (name, true));
            let yaml = match name {
                "base" => BASE,
                "default" => DEFAULT,
                _ => {
                    self.warn(
                        "extends",
                        format!("unsupported theme '{name}'; external themes are not loaded"),
                        inherited,
                    );
                    continue;
                }
            };
            if self.loaded.insert(name.to_owned()) || force {
                self.load(&value::parse(yaml)?, true)?;
            }
            self.warn("extends", format!("'{name}' uses the supported Asciidoctor PDF 2.3.15 settings; other inherited styles use acdc defaults"), inherited);
        }
        Ok(())
    }

    fn flatten(
        &mut self,
        key: &str,
        path: &str,
        value: &Value,
        inherited: bool,
    ) -> Result<(), Error> {
        if let Value::Mapping(mapping) = value
            && !mapping.is_empty()
        {
            for (child, value) in mapping {
                self.flatten(
                    &format!("{key}_{}", child.replace('-', "_")),
                    &format!("{path}.{child}"),
                    value,
                    inherited,
                )?;
            }
            return Ok(());
        }
        let resolved = match resolve(value, &self.variables) {
            Ok(value) => value,
            Err(message) => {
                self.warn(path, message, inherited);
                // An unsupported expression must not expose an older inherited value.
                self.entries.remove(key);
                self.variables.remove(key);
                return Ok(());
            }
        };
        let resolved = if key.ends_with("_color") {
            resolved
        } else {
            match &resolved {
                // A quoted numeric string can be an RGB variable, so keep its width.
                Value::String(text) if text.parse::<f64>().is_err() => expression::evaluate(text)
                    .map_or(resolved.clone(), |number| {
                        Value::Number(number.to_string(), None)
                    }),
                Value::Null
                | Value::Bool
                | Value::String(_)
                | Value::Number(..)
                | Value::Sequence(_)
                | Value::Mapping(_) => resolved,
            }
        };
        self.variables.insert(key.to_owned(), resolved.clone());
        if let Some(previous) = self.entries.get(key)
            && !previous.inherited
            && !inherited
        {
            self.warn(
                path,
                format!("overrides '{}' after key normalization", previous.path),
                false,
            );
        }
        self.entries.insert(
            key.to_owned(),
            Entry {
                path: path.to_owned(),
                value: resolved,
                inherited,
            },
        );
        Ok(())
    }
}

fn resolve(value: &Value, variables: &BTreeMap<String, Value>) -> Result<Value, String> {
    match value {
        Value::Sequence(values) => values
            .iter()
            .map(|value| resolve(value, variables))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Sequence),
        Value::String(text) if text.contains('$') => {
            let mut output = String::new();
            let mut remaining = text.as_str();
            while let Some((before, after)) = remaining.split_once('$') {
                output.push_str(before);
                let end = after
                    .find(|character: char| {
                        !character.is_ascii_alphanumeric() && character != '_' && character != '-'
                    })
                    .unwrap_or(after.len());
                let name = after
                    .get(..end)
                    .ok_or("invalid theme reference")?
                    .replace('-', "_");
                let variable = variables.get(&name).ok_or_else(|| {
                    format!("unsupported reference '${name}'; define it before this setting")
                })?;
                if expanded_size(variable) > 4096 {
                    return Err("expanded theme reference is too long".to_owned());
                }
                if output.is_empty() && end == after.len() && text.starts_with('$') {
                    return Ok(variable.clone());
                }
                output.push_str(
                    variable
                        .text()
                        .ok_or_else(|| format!("unsupported scalar reference '${name}'"))?,
                );
                remaining = after.get(end..).ok_or("invalid theme reference")?;
                if output.len() > 4096 {
                    return Err("expanded theme expression is too long".to_owned());
                }
            }
            output.push_str(remaining);
            if output.len() > 4096 {
                return Err("expanded theme expression is too long".to_owned());
            }
            Ok(Value::String(output))
        }
        Value::Null | Value::Bool | Value::Number(..) | Value::String(_) | Value::Mapping(_) => {
            Ok(value.clone())
        }
    }
}

fn expanded_size(value: &Value) -> usize {
    match value {
        Value::Number(text, _) => text
            .len()
            .saturating_add(value.colour_text().map_or(0, str::len)),
        Value::String(text) => text.len(),
        Value::Sequence(values) => values.iter().fold(1_usize, |size, value| {
            size.saturating_add(expanded_size(value))
        }),
        Value::Mapping(values) => values.iter().fold(1_usize, |size, (key, value)| {
            size.saturating_add(key.len())
                .saturating_add(expanded_size(value))
        }),
        Value::Null | Value::Bool => 1,
    }
}
