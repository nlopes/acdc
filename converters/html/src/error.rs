#[derive(thiserror::Error, Debug)]
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
}
