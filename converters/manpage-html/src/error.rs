use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error("Parsing error: {0}")]
    Parse(#[from] acdc_parser::Error),

    #[error("input file and output file cannot be the same: {0}")]
    OutputPathSameAsInput(PathBuf),

    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}
