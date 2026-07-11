use thiserror::Error;

/// The raster formats the resolver can decode, named for diagnostics. Kept in
/// sync with the `image` crate features enabled in the workspace manifest.
const SUPPORTED_RASTER: &str = "PNG, JPEG, GIF, WebP, BMP, TIFF";

/// A failure to resolve a single image reference. Resolution records these per
/// URL and keeps going, so one broken image never aborts the whole document.
#[derive(Debug, Error)]
pub enum Error {
    /// A remote fetch failed for a reason other than a timeout or HTTP status
    /// (connection refused, DNS, TLS, …).
    #[cfg(feature = "network")]
    #[error("network error: {0}")]
    Network(String),

    /// A remote fetch exceeded the configured timeout.
    #[cfg(feature = "network")]
    #[error("remote fetch timed out")]
    Timeout,

    /// A remote fetch returned a non-success HTTP status.
    #[cfg(feature = "network")]
    #[error("remote server returned HTTP status {0}")]
    HttpStatus(u16),

    /// A remote reference was encountered in a build without the `network`
    /// feature, so it could not be fetched.
    #[cfg(not(feature = "network"))]
    #[error("remote image fetching is disabled (build with the `network` feature to enable it)")]
    NetworkDisabled,

    /// The configured source policy rejected this image reference.
    #[error("image access denied: {0}")]
    AccessDenied(String),

    #[error("could not read {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },

    #[error("invalid data URI: {0}")]
    DataUri(String),

    #[error("{}", too_large_message(*limit, *actual))]
    TooLarge { limit: u64, actual: Option<u64> },

    #[error("image data was empty")]
    Empty,

    #[error(
        "unrecognised or unsupported image format (supported: {SUPPORTED_RASTER}, or a valid SVG)"
    )]
    UnknownFormat,

    #[error("could not decode image: {0}")]
    Decode(String),

    #[error("invalid SVG: {0}")]
    Svg(String),
}

/// Build the `TooLarge` message, quoting the actual size when it is known so the
/// warning is actionable rather than a bare cap.
fn too_large_message(limit: u64, actual: Option<u64>) -> String {
    match actual {
        Some(actual) => {
            format!("image is {actual} bytes, over the maximum allowed size of {limit} bytes")
        }
        None => format!("image exceeds the maximum allowed size of {limit} bytes"),
    }
}
