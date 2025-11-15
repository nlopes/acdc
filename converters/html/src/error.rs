#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Parsing error: {0}")]
    Parse(#[from] acdc_parser::Error),

    #[error("input file and output file cannot be the same: {0}")]
    OutputPathSameAsInput(std::path::PathBuf),

    #[error(
        "Invalid admonition caption: {0} - caption attribute should match one of the defaults provided by the parser (e.g., 'note-caption', 'tip-caption', 'important-caption', 'warning-caption', 'caution-caption')"
    )]
    InvalidAdmonitionCaption(String),

    #[error("Index out of bounds for {0}: {1}")]
    IndexOutOfBounds(&'static str, usize),
}
