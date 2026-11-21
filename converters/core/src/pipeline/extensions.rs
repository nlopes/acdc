use std::collections::{BTreeMap, HashSet};

use acdc_parser::extensions::{RegistrationError, Registry};

use super::Extensions;

/// Available extension registrations. Availability does not activate processors.
#[derive(Clone, Debug, Default)]
pub struct ExtensionCatalog {
    available: BTreeMap<String, Extensions>,
}

/// Configuration rejected before input processing or handler construction.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtensionSelectionError {
    /// A catalog name does not follow the documented identifier syntax.
    #[error("invalid extension name: {0}")]
    InvalidName(String),
    /// The catalog already contains this name.
    #[error("extension `{0}` is already available")]
    DuplicateName(String),
    /// The selected name was not compiled and registered by the application.
    #[error("extension `{0}` is not available in this build")]
    Unavailable(String),
    /// Selected extension packages conflict in the parser registration namespace.
    #[error("could not activate extension `{name}`: {source}")]
    Conflict {
        /// Extension whose registrations conflict with an earlier selection.
        name: String,
        /// Rejected macro or tree-processor registration.
        #[source]
        source: RegistrationError,
    },
}

impl ExtensionCatalog {
    /// Make registrations available under an ASCII `[A-Za-z][A-Za-z0-9_-]*` name.
    /// Processor factories are retained without being invoked.
    ///
    /// # Errors
    /// Rejects invalid or duplicate catalog names without changing the catalog.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        registrations: impl Into<Extensions>,
    ) -> Result<(), ExtensionSelectionError> {
        let name = name.into();
        let mut bytes = name.bytes();
        if !bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
            || !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(ExtensionSelectionError::InvalidName(name));
        }
        if self.available.contains_key(&name) {
            return Err(ExtensionSelectionError::DuplicateName(name));
        }
        self.available.insert(name, registrations.into());
        Ok(())
    }

    /// Available names in sorted order, independently of activation.
    pub fn names(&self) -> impl ExactSizeIterator<Item = &str> + '_ {
        self.available.keys().map(String::as_str)
    }

    /// Activate packages in selection order, preserving each package's registration order.
    /// Repeated names are ignored after their first occurrence. No active registrations
    /// returns `None`, keeping ordinary parsing free of extension sessions and overlays.
    ///
    /// # Errors
    /// Rejects unavailable names and conflicting registrations before parsing starts.
    pub fn select(
        &self,
        names: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<Option<Extensions>, ExtensionSelectionError> {
        let mut selected = HashSet::new();
        let mut builder = Registry::builder();
        let mut active = false;
        let mut renderers = Vec::new();
        for name in names {
            let (name, registrations) = self
                .available
                .get_key_value(name.as_ref())
                .ok_or_else(|| ExtensionSelectionError::Unavailable(name.as_ref().into()))?;
            if !selected.insert(name.as_str()) {
                continue;
            }
            if let Some(parser) = registrations.parser_registry() {
                builder
                    .extend(parser)
                    .map_err(|source| ExtensionSelectionError::Conflict {
                        name: name.clone(),
                        source,
                    })?;
                active = true;
            }
            renderers.extend(registrations.renderers.iter().cloned());
        }
        let extensions = Extensions {
            parser: active.then(|| builder.build()),
            renderers,
        };
        Ok((!extensions.is_empty()).then_some(extensions))
    }
}
