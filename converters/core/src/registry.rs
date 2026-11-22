//! Select a statically linked converter before constructing its pipeline.
//!
//! A converter owns its public names and typed variants. The application binds each
//! choice to a factory or runner, then resolves the user's selection before reading
//! input. Only the selected runner constructs a converter. Its [`crate::BackendProfile`]
//! supplies parser attributes and its [`crate::ConversionResult`] reports the actual
//! destination after rendering.
//!
//! ```
//! use acdc_converters_core::registry::{ConverterRegistry, ConverterSpec};
//! let mut registry = ConverterRegistry::default();
//! registry.register(ConverterSpec::new("text", ()).with_aliases(&["txt"]), |()| str::to_uppercase)?;
//! let uppercase = registry.resolve("txt", None)?;
//! assert_eq!(uppercase("Hello"), "HELLO");
//! # Ok::<(), acdc_converters_core::registry::SelectionError>(())
//! ```
use std::collections::{BTreeMap, BTreeSet};

/// A public variant spelling and its converter-owned typed value.
#[derive(Clone, Copy, Debug)]
pub struct Variant<V> {
    /// Canonical variant name.
    pub name: &'static str,
    /// Alternate spellings accepted as variants.
    pub aliases: &'static [&'static str],
    /// Legacy backend name selecting just this variant, if any.
    pub shortcut: Option<&'static str>,
    /// Typed choice passed to the application adapter.
    pub value: V,
}

/// Names by which applications can select a statically linked converter.
#[derive(Clone, Copy, Debug)]
pub struct ConverterSpec<V: 'static> {
    name: &'static str,
    aliases: &'static [&'static str],
    default: V,
    variants: &'static [Variant<V>],
}

impl<V> ConverterSpec<V> {
    /// Declare a backend with its default choice. A plain backend uses `()`.
    pub const fn new(name: &'static str, default: V) -> Self {
        Self {
            name,
            default,
            aliases: &[],
            variants: &[],
        }
    }

    /// Accept alternative names with the same variant selection as the canonical name.
    #[must_use]
    pub const fn with_aliases(mut self, aliases: &'static [&'static str]) -> Self {
        self.aliases = aliases;
        self
    }

    /// Declare all explicit variant choices, including the default.
    #[must_use]
    pub const fn with_variants(mut self, variants: &'static [Variant<V>]) -> Self {
        self.variants = variants;
        self
    }
}

/// Registration and selection errors are reported before input processing.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SelectionError {
    /// A backend or alias is already claimed.
    #[error("backend name '{0}' is already registered")]
    Conflict(&'static str),
    /// A registration has an ambiguous variant namespace or an invalid name/default.
    #[error("invalid registration for backend '{0}': {1}")]
    Invalid(&'static str, &'static str),
    /// No backend or alias matches this spelling.
    #[error("backend '{0}' is not available")]
    Unavailable(String),
    /// A plain backend or fixed shortcut does not allow an explicit variant.
    #[error("backend '{0}' does not accept a variant")]
    NoVariants(String),
    /// A variant belongs to another backend or has an unknown spelling.
    #[error("variant '{variant}' is not supported by backend '{backend}'")]
    UnsupportedVariant {
        /// Canonical backend.
        backend: &'static str,
        /// Requested variant.
        variant: String,
    },
}

struct Family<T> {
    default: T,
    // An explicit spelling of the default reuses its adapter.
    variants: BTreeMap<&'static str, Option<T>>,
    variant_aliases: BTreeMap<&'static str, &'static str>,
}

/// A checked registry of application adapters. Resolution borrows the selected
/// adapter. Writes require exclusive access and leave existing entries intact on error.
pub struct ConverterRegistry<T> {
    families: BTreeMap<&'static str, Family<T>>,
    // Every public backend spelling resolves to its family and optional fixed variant.
    names: BTreeMap<&'static str, (&'static str, Option<&'static str>)>,
}

impl<T> Default for ConverterRegistry<T> {
    fn default() -> Self {
        Self {
            families: BTreeMap::new(),
            names: BTreeMap::new(),
        }
    }
}

impl<T> ConverterRegistry<T> {
    /// Register a converter, rejecting collisions with every existing name or alias.
    /// Bindings should describe how to run a choice without starting a conversion.
    ///
    /// # Errors
    /// Returns a conflict or invalid specification without changing the registry.
    pub fn register<V: Copy + Eq>(
        &mut self,
        spec: ConverterSpec<V>,
        bind: impl Fn(V) -> T,
    ) -> Result<(), SelectionError> {
        self.install(spec, bind, false)
    }

    /// Replace the family with this canonical name, retiring its former aliases.
    ///
    /// # Errors
    /// Rejects unknown families and alias collisions, preserving the prior registration.
    pub fn replace<V: Copy + Eq>(
        &mut self,
        spec: ConverterSpec<V>,
        bind: impl Fn(V) -> T,
    ) -> Result<(), SelectionError> {
        if !self.families.contains_key(spec.name) {
            return Err(SelectionError::Unavailable(spec.name.to_owned()));
        }
        self.install(spec, bind, true)
    }

    fn install<V: Copy + Eq>(
        &mut self,
        spec: ConverterSpec<V>,
        bind: impl Fn(V) -> T,
        replace: bool,
    ) -> Result<(), SelectionError> {
        let canonical = spec.name;
        let names = std::iter::once((canonical, None))
            .chain(spec.aliases.iter().map(|&name| (name, None)))
            .chain(
                spec.variants
                    .iter()
                    .filter_map(|v| Some((v.shortcut?, Some(v.name)))),
            );
        let mut additions = BTreeMap::new();
        for (name, fixed) in names {
            if name.is_empty() {
                return Err(SelectionError::Invalid(canonical, "empty backend name"));
            }
            if additions.insert(name, (canonical, fixed)).is_some()
                || self
                    .names
                    .get(name)
                    .is_some_and(|&(owner, _)| !replace || owner != canonical)
            {
                return Err(SelectionError::Conflict(name));
            }
        }
        let mut spellings = BTreeSet::new();
        for variant in spec.variants {
            for name in std::iter::once(&variant.name).chain(variant.aliases) {
                if name.is_empty() || !spellings.insert(*name) {
                    return Err(SelectionError::Invalid(
                        canonical,
                        "empty or duplicate variant spelling",
                    ));
                }
            }
        }
        if !spec.variants.is_empty() && !spec.variants.iter().any(|v| v.value == spec.default) {
            return Err(SelectionError::Invalid(
                canonical,
                "default variant is missing",
            ));
        }
        let family = Family {
            default: bind(spec.default),
            variants: spec
                .variants
                .iter()
                .map(|v| (v.name, (v.value != spec.default).then(|| bind(v.value))))
                .collect(),
            variant_aliases: spec
                .variants
                .iter()
                .flat_map(|v| v.aliases.iter().map(|&alias| (alias, v.name)))
                .collect(),
        };
        if replace {
            self.names.retain(|_, (owner, _)| *owner != canonical);
        }
        self.names.extend(additions);
        self.families.insert(canonical, family);
        Ok(())
    }

    /// Available canonical backend names, aliases, and shortcuts in lexical order.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.names.keys().copied()
    }

    /// Backend spellings for command-line help. Plain aliases are accepted but hidden.
    pub fn backend_spellings(&self) -> impl Iterator<Item = (&'static str, bool)> + '_ {
        self.names
            .iter()
            .map(|(&name, &(canonical, fixed))| (name, name == canonical || fixed.is_some()))
    }

    /// All variant spellings and whether each has a canonical use in the registry.
    pub fn variant_spellings(&self) -> impl Iterator<Item = (&'static str, bool)> {
        let mut names = BTreeMap::new();
        for family in self.families.values() {
            for &alias in family.variant_aliases.keys() {
                names.entry(alias).or_insert(false);
            }
            for &name in family.variants.keys() {
                names.insert(name, true);
            }
        }
        names.into_iter()
    }

    /// Resolve a backend and optional variant. Fixed shortcuts reject another variant.
    ///
    /// # Errors
    /// Reports an unavailable backend or incompatible variant.
    pub fn resolve(&self, name: &str, variant: Option<&str>) -> Result<&T, SelectionError> {
        let &(canonical, fixed) = self
            .names
            .get(name)
            .ok_or_else(|| SelectionError::Unavailable(name.to_owned()))?;
        let family = self
            .families
            .get(canonical)
            .ok_or_else(|| SelectionError::Unavailable(name.to_owned()))?;
        if variant.is_some() && (fixed.is_some() || family.variants.is_empty()) {
            return Err(SelectionError::NoVariants(name.to_owned()));
        }
        let Some(requested) = fixed.or(variant) else {
            return Ok(&family.default);
        };
        let key = family
            .variant_aliases
            .get(requested)
            .copied()
            .unwrap_or(requested);
        family
            .variants
            .get(key)
            .map(|adapter| adapter.as_ref().unwrap_or(&family.default))
            .ok_or_else(|| SelectionError::UnsupportedVariant {
                backend: canonical,
                variant: requested.to_owned(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORMAT: ConverterSpec<bool> = ConverterSpec::new("format", false)
        .with_aliases(&["f"])
        .with_variants(&[
            Variant {
                name: "standard",
                aliases: &[],
                shortcut: None,
                value: false,
            },
            Variant {
                name: "alternate",
                aliases: &["alt"],
                shortcut: Some("format2"),
                value: true,
            },
        ]);

    #[test]
    fn registered_names_select_one_adapter_per_choice() -> Result<(), SelectionError> {
        let mut catalog = ConverterRegistry::default();
        let calls = std::cell::Cell::new(0);
        catalog.register(FORMAT, |choice| {
            calls.set(calls.get() + 1);
            choice
        })?;
        assert_eq!(calls.get(), 2);
        assert!(std::ptr::eq(
            catalog.resolve("f", None)?,
            catalog.resolve("format", Some("standard"))?
        ));
        assert!(std::ptr::eq(
            catalog.resolve("format2", None)?,
            catalog.resolve("f", Some("alt"))?
        ));
        assert_eq!(
            catalog.resolve("format2", Some("alt")),
            Err(SelectionError::NoVariants("format2".into()))
        );
        assert!(matches!(
            catalog.resolve("format", Some("other")),
            Err(SelectionError::UnsupportedVariant {
                backend: "format",
                ..
            })
        ));
        Ok(())
    }

    #[test]
    fn replacement_retires_aliases_and_rejected_updates_are_atomic() -> Result<(), SelectionError> {
        let mut catalog = ConverterRegistry::default();
        catalog.register(FORMAT, |x| x)?;
        let other = ConverterSpec::new("other", ());
        catalog.register(other, |()| false)?;
        assert_eq!(
            catalog.register(other.with_aliases(&["f"]), |()| true),
            Err(SelectionError::Conflict("other"))
        );
        assert_eq!(
            catalog.replace(other.with_aliases(&["format2"]), |()| true),
            Err(SelectionError::Conflict("format2"))
        );
        assert!(matches!(
            catalog.register(
                ConverterSpec::new("third", ()).with_aliases(&["third"]),
                |()| false
            ),
            Err(SelectionError::Conflict("third"))
        ));
        assert_eq!(catalog.resolve("format2", None), Ok(&true));
        catalog.replace(
            ConverterSpec::new("format", false).with_aliases(&["new"]),
            |x| x,
        )?;
        assert!(matches!(
            catalog.resolve("f", None),
            Err(SelectionError::Unavailable(_))
        ));
        assert!(matches!(
            catalog.resolve("format2", None),
            Err(SelectionError::Unavailable(_))
        ));
        assert_eq!(catalog.resolve("new", None), Ok(&false));
        Ok(())
    }
}
