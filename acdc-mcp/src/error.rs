pub(super) type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("{0}")]
    Input(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Parse(#[from] acdc_parser::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("request cancelled")]
    Cancelled,
}
