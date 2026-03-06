#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    IntoInner(#[from] std::io::IntoInnerError<std::io::BufWriter<Vec<u8>>>),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Parsing error: {0}")]
    Parse(#[from] acdc_parser::Error),

    #[error(
        "Invalid admonition caption: {0} - caption attribute should match one of the defaults provided by the parser (e.g., 'note-caption', 'tip-caption', 'important-caption', 'warning-caption', 'caution-caption')"
    )]
    InvalidAdmonitionCaption(String),

    #[cfg(feature = "highlighting")]
    #[error("Invalid theme: {0} - theme not found in highlighting themes")]
    InvalidTheme(String),
}
