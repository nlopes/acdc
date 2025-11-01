#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Parse(#[from] acdc_parser::Error),
}
