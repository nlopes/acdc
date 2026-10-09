use std::{
    fmt,
    fs::File,
    io::{Cursor, Read},
    path::PathBuf,
    sync::Arc,
};

/// A fully resolved include target presented to an [`IncludeSourceProvider`].
///
/// The parser applies attribute substitution, relative-path resolution, and
/// Safe/Server confinement before constructing this value. Providers therefore
/// supply bytes; they do not reinterpret the document's raw include target.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum IncludeSourceTarget {
    /// A local filesystem or virtual-filesystem path.
    File(PathBuf),
    /// An absolute HTTP(S) URI.
    Uri(String),
}

/// An opened include source.
///
/// Providers can return an in-memory buffer or an owned byte reader.
pub struct IncludeSource {
    reader: Box<dyn Read>,
    #[cfg(feature = "network")]
    pub(crate) read_limit: Option<usize>,
}

impl IncludeSource {
    /// Create an include source from an owned reader.
    #[must_use]
    pub fn from_reader(reader: impl Read + 'static) -> Self {
        Self {
            reader: Box::new(reader),
            #[cfg(feature = "network")]
            read_limit: None,
        }
    }

    /// Create an include source from owned bytes.
    #[must_use]
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self::from_reader(Cursor::new(bytes.into()))
    }

    /// Create an include source from an owned UTF-8 string.
    #[must_use]
    pub fn from_string(source: impl Into<String>) -> Self {
        Self::from_bytes(source.into().into_bytes())
    }

    pub(crate) fn into_reader(self) -> Box<dyn Read> {
        self.reader
    }
}

impl fmt::Debug for IncludeSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IncludeSource")
            .finish_non_exhaustive()
    }
}

/// Classification of a source-provider failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum IncludeSourceErrorKind {
    /// The resolved source does not exist.
    NotFound,
    /// The source exists conceptually but could not be opened or retrieved.
    Unavailable,
    /// The provider does not support this target kind or transport.
    Unsupported,
    /// The provider cannot recover from this failure.
    Fatal,
}

/// Failure returned by an [`IncludeSourceProvider`].
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct IncludeSourceError {
    kind: IncludeSourceErrorKind,
    message: String,
}

impl IncludeSourceError {
    /// Create a classified source-provider error.
    #[must_use]
    pub fn new(kind: IncludeSourceErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Return this failure's classification.
    #[must_use]
    pub const fn kind(&self) -> IncludeSourceErrorKind {
        self.kind
    }
}

/// Supplies bytes for include targets resolved by the parser.
///
/// Implement this trait to parse against an editor overlay, virtual filesystem,
/// embedded source map, restricted filesystem capability, or custom HTTP client.
/// The parser checks safe mode and URI permission before calling the provider.
pub trait IncludeSourceProvider {
    /// Open one resolved include target.
    ///
    /// # Errors
    ///
    /// Return a classified [`IncludeSourceError`] when the target cannot be
    /// supplied. The parser maps recoverable failures to Asciidoctor-compatible
    /// include diagnostics and fallbacks.
    fn open(&self, target: &IncludeSourceTarget) -> Result<IncludeSource, IncludeSourceError>;
}

impl<F> IncludeSourceProvider for F
where
    F: Fn(&IncludeSourceTarget) -> Result<IncludeSource, IncludeSourceError>,
{
    fn open(&self, target: &IncludeSourceTarget) -> Result<IncludeSource, IncludeSourceError> {
        self(target)
    }
}

/// Selects how the parser loads include sources.
///
/// [`crate::SafeMode`] sets processing restrictions independently of this choice.
/// Safe-mode path checks and `allow-uri-read` apply to every provider.
/// The default is [`Self::System`]. Secure mode never calls a provider.
#[derive(Clone, Default)]
#[non_exhaustive]
pub enum IncludeLoader {
    /// Do not load include targets. Preserve directives unless Secure mode
    /// requires a link fallback.
    ///
    /// Applies to string, reader, and file input. Disabled includes are reported
    /// through source recovery diagnostics.
    Disabled,
    /// Load local targets from the operating-system filesystem, and, with the
    /// `network` feature, authorized HTTP(S) targets.
    ///
    /// This is the default for all parse entry points.
    #[default]
    System,
    /// Load sources through a caller-supplied provider.
    Custom(Arc<dyn IncludeSourceProvider + Send + Sync>),
}

impl IncludeLoader {
    /// Load include sources through `provider`.
    #[must_use]
    pub fn custom(provider: impl IncludeSourceProvider + Send + Sync + 'static) -> Self {
        Self::Custom(Arc::new(provider))
    }

    /// Return the selected provider, or `None` when loading is disabled.
    pub(crate) fn provider(&self) -> Option<&(dyn IncludeSourceProvider + '_)> {
        static SYSTEM: SystemIncludeSourceProvider = SystemIncludeSourceProvider;
        match self {
            Self::Disabled => None,
            Self::System => Some(&SYSTEM),
            Self::Custom(provider) => Some(&**provider),
        }
    }
}

impl fmt::Debug for IncludeLoader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => formatter.write_str("Disabled"),
            Self::System => formatter.write_str("System"),
            Self::Custom(_) => formatter.debug_tuple("Custom").finish_non_exhaustive(),
        }
    }
}

/// Operating-system provider used by [`IncludeLoader::System`].
///
/// Local targets are opened from the operating-system filesystem. With the
/// `network` feature enabled, authorized HTTP(S) targets use `ureq`; without it,
/// URI loading reports [`IncludeSourceErrorKind::Unsupported`].
/// The parser limits built-in HTTP(S) response bodies to 10 MiB after transport
/// decompression. Custom providers manage their own transport limits.
#[derive(Clone, Copy, Debug, Default)]
struct SystemIncludeSourceProvider;

impl IncludeSourceProvider for SystemIncludeSourceProvider {
    fn open(&self, target: &IncludeSourceTarget) -> Result<IncludeSource, IncludeSourceError> {
        match target {
            IncludeSourceTarget::File(path) => {
                // Preserve the parser's established classification for
                // directories, broken symlinks, and metadata failures: they are
                // missing include files rather than opened-but-unreadable sources.
                if !path.is_file() {
                    return Err(IncludeSourceError::new(
                        IncludeSourceErrorKind::NotFound,
                        format!("include file not found: {}", path.display()),
                    ));
                }
                File::open(path)
                    .map(IncludeSource::from_reader)
                    .map_err(|error| {
                        let kind = if error.kind() == std::io::ErrorKind::NotFound {
                            IncludeSourceErrorKind::NotFound
                        } else {
                            IncludeSourceErrorKind::Unavailable
                        };
                        IncludeSourceError::new(kind, error.to_string())
                    })
            }
            IncludeSourceTarget::Uri(uri) => {
                #[cfg(feature = "network")]
                {
                    let response = ureq::get(uri).call().map_err(|error| {
                        IncludeSourceError::new(
                            IncludeSourceErrorKind::Unavailable,
                            error.to_string(),
                        )
                    })?;
                    let (_, body) = response.into_parts();
                    let mut source = IncludeSource::from_reader(body.into_reader());
                    source.read_limit = Some(10 * 1024 * 1024);
                    Ok(source)
                }
                #[cfg(not(feature = "network"))]
                {
                    let _ = uri;
                    Err(IncludeSourceError::new(
                        IncludeSourceErrorKind::Unsupported,
                        "network support is disabled",
                    ))
                }
            }
        }
    }
}
