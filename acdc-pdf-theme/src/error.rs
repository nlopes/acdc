use thiserror::Error;

/// An error loading or validating a PDF theme.
#[derive(Debug, Error)]
pub enum Error {
    /// The YAML syntax or schema is invalid.
    #[error("invalid theme YAML: {0}")]
    Yaml(#[source] Box<serde_saphyr::Error>),
    /// A value does not satisfy the constraints for its field.
    #[error("invalid theme field `{field}`: {message}")]
    Validation {
        /// Dotted field path in the theme document.
        field: String,
        /// Human-readable description of the violated constraint.
        message: String,
    },
}

impl From<serde_saphyr::Error> for Error {
    fn from(error: serde_saphyr::Error) -> Self {
        Self::Yaml(Box::new(error))
    }
}

impl Error {
    pub(super) fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation {
            field: field.into(),
            message: message.into(),
        }
    }
}
