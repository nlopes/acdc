use std::{collections::HashMap, path::Path, sync::Arc};

use crate::{Error, Options, ParseResult};

use super::{BlockMacro, ExtensionParseResult, Session};

/// A registration rejected before any document is parsed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum RegistrationError {
    #[error("invalid extension name: {0}")]
    InvalidName(String),
    #[error("block macro name is reserved: {0}")]
    ReservedName(String),
    #[error("block macro is already registered: {0}")]
    DuplicateName(String),
    #[error("block macro is not registered: {0}")]
    UnknownName(String),
}

/// Builds a registry without modifying existing registry snapshots.
#[derive(Debug, Default)]
pub struct RegistryBuilder {
    macros: Vec<BlockMacro>,
    names: HashMap<String, usize>,
}

impl RegistryBuilder {
    /// Register a macro after the existing registrations.
    ///
    /// Names match `[A-Za-z][A-Za-z0-9_-]*`. Built-in macro names are reserved.
    ///
    /// # Errors
    /// Rejects invalid names, built-in names, and duplicate registrations.
    pub fn register(&mut self, descriptor: BlockMacro) -> Result<&mut Self, RegistrationError> {
        validate_name(&descriptor.name)?;
        if self.names.contains_key(&descriptor.name) {
            return Err(RegistrationError::DuplicateName(descriptor.name));
        }
        self.names
            .insert(descriptor.name.clone(), self.macros.len());
        self.macros.push(descriptor);
        Ok(self)
    }

    /// Replace a macro explicitly, preserving its registration position.
    ///
    /// # Errors
    /// Rejects names that have not been registered.
    pub fn replace(&mut self, descriptor: BlockMacro) -> Result<&mut Self, RegistrationError> {
        let index = *self
            .names
            .get(&descriptor.name)
            .ok_or_else(|| RegistrationError::UnknownName(descriptor.name.clone()))?;
        if let Some(entry) = self.macros.get_mut(index) {
            *entry = descriptor;
        }
        Ok(self)
    }

    #[must_use]
    pub fn build(self) -> Registry {
        Registry(Arc::new(RegistryData {
            macros: self.macros,
            names: self.names,
        }))
    }
}

fn validate_name(name: &str) -> Result<(), RegistrationError> {
    validate_identifier(name)?;
    if matches!(
        name,
        "image" | "audio" | "video" | "toc" | "include" | "ifdef" | "ifndef" | "ifeval" | "endif"
    ) {
        return Err(RegistrationError::ReservedName(name.to_owned()));
    }
    Ok(())
}

fn validate_identifier(name: &str) -> Result<(), RegistrationError> {
    let mut bytes = name.bytes();
    if !bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(RegistrationError::InvalidName(name.to_owned()));
    }
    Ok(())
}

#[derive(Debug, Default)]
struct RegistryData {
    macros: Vec<BlockMacro>,
    names: HashMap<String, usize>,
}

/// A cheap-to-clone, immutable collection of extension factories.
#[derive(Clone, Debug, Default)]
pub struct Registry(Arc<RegistryData>);

impl Registry {
    #[must_use]
    pub fn builder() -> RegistryBuilder {
        RegistryBuilder::default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.macros.is_empty()
    }

    /// Parse with fresh processors and retain this document's deferred requests.
    ///
    /// Macro processors run after the complete document is accepted. The host
    /// repairs captions and catalogs. Callbacks cannot change source processing.
    /// An empty registry uses the ordinary parser path.
    ///
    /// # Errors
    /// Returns preprocessing, parsing, or processor errors with source locations.
    pub fn parse(&self, input: &str, options: &Options<'_>) -> Result<ExtensionParseResult, Error> {
        self.parse_using(options, |options, session| {
            crate::parse_with_session(input, options, session)
        })
    }

    /// Parse a file with the same session contract as [`Self::parse`].
    ///
    /// # Errors
    /// Returns file, preprocessing, parsing, or processor errors.
    pub fn parse_file(
        &self,
        path: impl AsRef<Path>,
        options: &Options<'_>,
    ) -> Result<ExtensionParseResult, Error> {
        self.parse_using(options, |options, session| {
            crate::parse_file_with_session(path.as_ref(), options, session)
        })
    }

    fn parse_using(
        &self,
        options: &Options<'_>,
        parse: impl FnOnce(&Options<'_>, Option<&mut Session>) -> Result<ParseResult, Error>,
    ) -> Result<ExtensionParseResult, Error> {
        let mut session = self.session();
        let parsed = if self.0.macros.is_empty() {
            parse(options, None)?
        } else {
            let mut options = options.clone();
            options.extensions = Some(self.clone());
            parse(&options, Some(&mut session))?
        };
        let mut result = ExtensionParseResult::new(parsed, session);
        if result.session.processed_blocks {
            result.repair_after_edit()?;
        }
        Ok(result)
    }

    pub(crate) fn find(&self, name: &str) -> Option<(usize, &BlockMacro)> {
        let index = *self.0.names.get(name)?;
        self.0
            .macros
            .get(index)
            .map(|descriptor| (index, descriptor))
    }

    pub(crate) fn session(&self) -> Session {
        Session::new(
            self.0
                .macros
                .iter()
                .map(|entry| (entry.factory)())
                .collect(),
        )
    }
}
