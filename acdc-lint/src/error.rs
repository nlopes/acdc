/// Error type for lint execution and lint-name parsing.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Failed to parse the `AsciiDoc` input.
    #[error(transparent)]
    Parser(#[from] acdc_parser::Error),

    /// Failed to read input.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Failed to parse a lint or lint group name.
    #[error("unknown lint or lint group `{name}`")]
    UnknownLintName {
        /// Unknown name as it was provided by the caller.
        name: String,
    },

    /// Failed to parse a location scope attached to a lint override.
    #[error("invalid lint location `{location}`: {reason}")]
    InvalidLintLocation {
        /// Location text as it was provided by the caller.
        location: String,
        /// Human-readable reason parsing failed.
        reason: &'static str,
    },
}
