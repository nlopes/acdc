use std::{collections::HashMap, path::Path, sync::Arc};

use crate::{Error, Options, ParseResult};

use super::{
    BlockMacro, DocumentEditor, ExtensionParseResult, Session, SourcePreprocessor,
    TreeProcessorRegistration, source::SourceRegistration,
};

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
    #[error("tree processor is already registered: {0}")]
    DuplicateTreeProcessor(String),
    #[error("source preprocessor is already registered: {0}")]
    DuplicatePreprocessor(String),
    #[error("deferred processor already registered for block macro: {0}")]
    DuplicateDeferredProcessor(String),
}

/// Builds a registry without modifying existing registry snapshots.
#[derive(Debug, Default)]
pub struct RegistryBuilder {
    macros: Vec<BlockMacro>,
    names: HashMap<String, usize>,
    trees: Vec<TreeProcessorRegistration>,
    preprocessors: Vec<SourceRegistration<dyn SourcePreprocessor>>,
}

impl RegistryBuilder {
    /// Append another immutable registry in its registration order.
    ///
    /// # Errors
    /// Rejects duplicate registrations without changing this builder.
    pub fn extend(&mut self, registry: &Registry) -> Result<&mut Self, RegistrationError> {
        for descriptor in &registry.0.macros {
            if self.names.contains_key(&descriptor.name) {
                return Err(RegistrationError::DuplicateName(descriptor.name.clone()));
            }
        }
        for descriptor in &registry.0.trees {
            self.check_tree_name(descriptor)?;
        }
        for entry in &registry.0.preprocessors {
            check_source_name(
                &self.preprocessors,
                &entry.name,
                RegistrationError::DuplicatePreprocessor,
            )?;
        }
        self.preprocessors
            .extend_from_slice(&registry.0.preprocessors);
        let offset = self.macros.len();
        self.names.extend(
            registry
                .0
                .macros
                .iter()
                .enumerate()
                .map(|(index, entry)| (entry.name.clone(), offset + index)),
        );
        self.macros.extend(registry.0.macros.iter().cloned());
        self.trees.extend(registry.0.trees.iter().cloned());
        Ok(self)
    }

    /// Append an entry-source preprocessor in registration order.
    ///
    /// # Errors
    /// Rejects invalid identifiers and duplicate names within this family.
    pub fn register_preprocessor<P: SourcePreprocessor + 'static>(
        &mut self,
        name: impl Into<String>,
        factory: impl Fn() -> P + Send + Sync + 'static,
    ) -> Result<&mut Self, RegistrationError> {
        let name = name.into();
        check_source_name(
            &self.preprocessors,
            &name,
            RegistrationError::DuplicatePreprocessor,
        )?;
        self.preprocessors.push(SourceRegistration {
            name,
            factory: Arc::new(move || Box::new(factory())),
        });
        Ok(self)
    }

    /// Register a tree processor after the existing tree processors.
    ///
    /// General tree processor names are independent of macro names.
    /// Use [`TreeProcessorRegistration::for_macro`] for deferred requests.
    ///
    /// # Errors
    /// Rejects invalid or duplicate registrations. A macro must be registered
    /// before its deferred processor and can have only one such processor.
    pub fn register_tree_processor(
        &mut self,
        registration: TreeProcessorRegistration,
    ) -> Result<&mut Self, RegistrationError> {
        validate_identifier(&registration.name)?;
        if registration.handles_deferred && !self.names.contains_key(&registration.name) {
            return Err(RegistrationError::UnknownName(registration.name));
        }
        self.check_tree_name(&registration)?;
        self.trees.push(registration);
        Ok(self)
    }

    fn check_tree_name(
        &self,
        registration: &TreeProcessorRegistration,
    ) -> Result<(), RegistrationError> {
        if self.trees.iter().any(|entry| {
            entry.name == registration.name
                && entry.handles_deferred == registration.handles_deferred
        }) {
            return Err(if registration.handles_deferred {
                RegistrationError::DuplicateDeferredProcessor(registration.name.clone())
            } else {
                RegistrationError::DuplicateTreeProcessor(registration.name.clone())
            });
        }
        Ok(())
    }

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

    /// Replace a macro explicitly, preserving its registration position and deferred processor.
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
            trees: self.trees,
            preprocessors: self.preprocessors,
        }))
    }
}

fn check_source_name<P: ?Sized>(
    entries: &[SourceRegistration<P>],
    name: &str,
    duplicate: fn(String) -> RegistrationError,
) -> Result<(), RegistrationError> {
    validate_identifier(name)?;
    if entries.iter().any(|entry| entry.name == name) {
        return Err(duplicate(name.to_owned()));
    }
    Ok(())
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
    trees: Vec<TreeProcessorRegistration>,
    preprocessors: Vec<SourceRegistration<dyn SourcePreprocessor>>,
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
        !self.has_input_processors() && self.0.trees.is_empty()
    }

    fn has_input_processors(&self) -> bool {
        !(self.0.macros.is_empty() && self.0.preprocessors.is_empty())
    }

    pub(crate) fn preprocessors(&self) -> impl Iterator<Item = Box<dyn SourcePreprocessor>> + '_ {
        self.0.preprocessors.iter().map(|entry| (entry.factory)())
    }

    pub(crate) fn has_tree_processors(&self) -> bool {
        !self.0.trees.is_empty()
    }

    /// Parse with fresh processors and retain this document's deferred requests.
    ///
    /// Macro processors run once at committed block boundaries, before the
    /// reader advances to later conditionals and includes. Attribute changes
    /// follow normal document assignment policy. Tree processors then run once
    /// in registration order, with catalog repair after each edit. Effects from
    /// completed callbacks are not rolled back if a later block fails.
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

    /// Parse a reader with the same session and source-order contract as [`Self::parse`].
    /// Reader input keeps the ordinary reader attribute and include-origin policy.
    ///
    /// # Errors
    /// Returns input, preprocessing, parsing, or processor errors.
    pub fn parse_from_reader<R: std::io::Read>(
        &self,
        reader: R,
        options: &Options<'_>,
    ) -> Result<ExtensionParseResult, Error> {
        self.parse_using(options, |options, session| {
            crate::parse_reader_with_session(reader, options, session)
        })
    }

    fn parse_using(
        &self,
        options: &Options<'_>,
        parse: impl FnOnce(&Options<'_>, Option<&mut Session>) -> Result<ParseResult, Error>,
    ) -> Result<ExtensionParseResult, Error> {
        let mut session = self.session();
        let mut options = options.clone();
        // Retain query input even when there are no registered macros. The
        // ordinary recognizers and inactive source-reader path remain shared.
        options.extensions = Some(self.clone());
        let parsed = parse(
            &options,
            self.has_input_processors().then_some(&mut session),
        )?;
        let mut result = ExtensionParseResult::new(parsed, session, options.compatibility_mode);
        if result.session.processed_blocks {
            result.repair_after_edit()?;
        }
        let mut processors = std::mem::take(&mut result.session.tree_processors);
        for (processor, registration) in processors.iter_mut().zip(&self.0.trees) {
            let owner = registration
                .handles_deferred
                .then_some(registration.name.as_str());
            processor.process(&mut DocumentEditor::new(&mut result, owner))?;
        }
        result.session.tree_processors = processors;
        Ok(result)
    }

    pub(crate) fn find(&self, name: &str) -> Option<(usize, &BlockMacro)> {
        let index = *self.0.names.get(name)?;
        self.0
            .macros
            .get(index)
            .map(|descriptor| (index, descriptor))
    }

    /// A conservative dispatch filter, not a syntax recognizer. Includes can
    /// introduce registered names that are absent from the entry source.
    pub(crate) fn may_execute(&self, source: &str) -> bool {
        !self.0.preprocessors.is_empty()
            || source.match_indices("::").any(|(end, _)| {
                let name = source[..end]
                    .rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                    .next()
                    .unwrap_or_default();
                name == "include" || self.0.names.contains_key(name)
            })
    }

    pub(crate) fn session(&self) -> Session {
        Session::new(
            self.0
                .macros
                .iter()
                .map(|entry| (entry.factory)())
                .collect(),
            self.0.trees.iter().map(|entry| (entry.factory)()).collect(),
        )
    }
}
