use std::{
    io::Read as _,
    path::{Path, PathBuf},
};

use base64::Engine as _;
use percent_encoding::percent_decode_str;

use crate::{ResolveConfig, SourcePolicy, error::Error};

/// Fetches image bytes for a batch of references. It holds the resolver policy
/// (`base_dir`, size cap) and, when the `network` feature is on, a single HTTP
/// agent reused across every remote fetch, so N remote images from one host
/// share a connection pool instead of rebuilding it each time.
pub(super) struct Fetcher {
    base_dir: PathBuf,
    max_bytes: u64,
    source_policy: SourcePolicy,
    #[cfg(feature = "network")]
    agent: ureq::Agent,
}

impl Fetcher {
    pub(super) fn new(config: &ResolveConfig) -> Self {
        Fetcher {
            base_dir: config.base_dir.clone(),
            max_bytes: config.max_bytes,
            source_policy: config.source_policy,
            #[cfg(feature = "network")]
            agent: {
                let agent_config = ureq::Agent::config_builder()
                    .timeout_global(Some(config.timeout))
                    .user_agent("acdc-pdf")
                    .build();
                ureq::Agent::new_with_config(agent_config)
            },
        }
    }

    /// Fetch the raw bytes for one image reference, dispatching on its scheme:
    /// inline `data:`, remote `http(s)`, a `file://` URL, or a bare local path.
    /// Scheme matching is case-insensitive. Every path enforces `max_bytes` as
    /// early as it can so no single image can blow up memory or the render; the
    /// final check here is a safety net for the boundary case.
    ///
    pub(super) fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        if self.source_policy == SourcePolicy::DenyAll {
            return Err(Error::AccessDenied(
                "all document image sources are disabled".to_string(),
            ));
        }

        let bytes = if starts_with_ci(url, "data:") {
            decode_data_uri(url.get(5..).unwrap_or_default(), self.max_bytes)?
        } else if starts_with_ci(url, "http://") || starts_with_ci(url, "https://") {
            if matches!(
                self.source_policy,
                SourcePolicy::Confined {
                    allow_network: false
                }
            ) {
                return Err(Error::AccessDenied(
                    "remote image access is disabled".to_string(),
                ));
            }
            self.fetch_remote(url)?
        } else if starts_with_ci(url, "file://") {
            let path = self.allowed_local_path(file_url_path(url)?)?;
            read_file_capped(&path, self.max_bytes)?
        } else {
            let path = self.allowed_local_path(self.base_dir.join(url))?;
            read_file_capped(&path, self.max_bytes)?
        };
        if bytes.len() as u64 > self.max_bytes {
            return Err(Error::TooLarge {
                limit: self.max_bytes,
                actual: Some(bytes.len() as u64),
            });
        }
        Ok(bytes)
    }

    fn allowed_local_path(&self, path: PathBuf) -> Result<PathBuf, Error> {
        if !matches!(self.source_policy, SourcePolicy::Confined { .. }) {
            return Ok(path);
        }

        let base_dir = canonicalize(&self.base_dir)?;
        let candidate = canonicalize(&path)?;
        if !candidate.starts_with(&base_dir) {
            return Err(Error::AccessDenied(format!(
                "{} is outside {}",
                candidate.display(),
                base_dir.display()
            )));
        }
        Ok(candidate)
    }

    #[cfg(feature = "network")]
    fn fetch_remote(&self, url: &str) -> Result<Vec<u8>, Error> {
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|error| classify_network_error(error, self.max_bytes))?;
        // ureq applies its built-in limit before content decoding. Cap the
        // reader again after gzip/brotli decoding so compressed responses cannot
        // expand past the configured image limit.
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(self.max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| Error::Network(error.to_string()))?;
        Ok(bytes)
    }

    /// Without the `network` feature there is no HTTP stack, so a remote reference
    /// becomes a per-image failure and the rest of the document still renders.
    #[cfg(not(feature = "network"))]
    #[allow(clippy::unused_self)]
    fn fetch_remote(&self, _url: &str) -> Result<Vec<u8>, Error> {
        Err(Error::NetworkDisabled)
    }
}

/// Map a `ureq` transport error to the most specific resolver error, so warnings
/// distinguish a timeout, an HTTP status, and an oversized body from a generic
/// connection failure.
#[cfg(feature = "network")]
fn classify_network_error(error: ureq::Error, limit: u64) -> Error {
    // `ureq::Error` is `#[non_exhaustive]`; only the cases worth their own
    // diagnostic are matched, the rest collapse to `Network`.
    #[allow(clippy::wildcard_enum_match_arm)]
    match error {
        ureq::Error::BodyExceedsLimit(_) => Error::TooLarge {
            limit,
            actual: None,
        },
        ureq::Error::StatusCode(code) => Error::HttpStatus(code),
        ureq::Error::Timeout(_) => Error::Timeout,
        other => Error::Network(other.to_string()),
    }
}

/// Case-insensitive scheme-prefix test that never panics on a short or
/// non-char-boundary input.
fn starts_with_ci(url: &str, prefix: &str) -> bool {
    url.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Resolve a `file://` URL to a local path, handling an empty or `localhost`
/// host and percent-escaped segments.
fn file_url_path(url: &str) -> Result<PathBuf, Error> {
    let invalid = |detail: &str| Error::Io {
        path: url.to_string(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, detail.to_string()),
    };
    let parsed = url::Url::parse(url).map_err(|error| invalid(&error.to_string()))?;
    parsed
        .to_file_path()
        .map_err(|()| invalid("not a local file URL"))
}

/// Resolve a path to its canonical form for containment checks.
fn canonicalize(path: &Path) -> Result<PathBuf, Error> {
    std::fs::canonicalize(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })
}

/// Read a local file, rejecting anything over `max_bytes` before allocating for
/// it: the metadata length is checked first, and the read itself is capped at
/// `limit + 1` so a file that lies about or grows past its length still cannot
/// allocate without bound (the extra byte lets [`Fetcher::fetch`] see the
/// overflow).
fn read_file_capped(path: &Path, max_bytes: u64) -> Result<Vec<u8>, Error> {
    let io_err = |source| Error::Io {
        path: path.display().to_string(),
        source,
    };
    let file = std::fs::File::open(path).map_err(io_err)?;
    if let Ok(metadata) = file.metadata()
        && metadata.len() > max_bytes
    {
        return Err(Error::TooLarge {
            limit: max_bytes,
            actual: Some(metadata.len()),
        });
    }
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(io_err)?;
    Ok(bytes)
}

/// Decode a `data:` URI payload (`[<mediatype>][;base64],<data>`), enforcing the
/// size cap before allocating the full image.
fn decode_data_uri(rest: &str, max_bytes: u64) -> Result<Vec<u8>, Error> {
    let comma = rest
        .find(',')
        .ok_or_else(|| Error::DataUri("missing comma separator".to_string()))?;
    let meta = rest.get(..comma).unwrap_or_default();
    let data = rest.get(comma + 1..).unwrap_or_default();

    if meta.ends_with(";base64") {
        decode_base64_capped(data, max_bytes)
    } else {
        // Non-base64 payloads are percent-encoded (RFC 2397), e.g.
        // `data:image/svg+xml,%3Csvg.../%3E`; decode the escapes.
        percent_decode_capped(data, max_bytes)
    }
}

/// Base64-decode a data-URI payload, preflighting the decoded size from the
/// input length so a huge payload is rejected before it is decoded.
fn decode_base64_capped(data: &str, max_bytes: u64) -> Result<Vec<u8>, Error> {
    let engine = base64::engine::general_purpose::STANDARD;
    // The STANDARD alphabet rejects whitespace, so it must be stripped — but only
    // pay for the copy when there is actually whitespace to strip.
    let has_ws = data.bytes().any(|b| b.is_ascii_whitespace());
    let significant = if has_ws {
        data.bytes().filter(|b| !b.is_ascii_whitespace()).count()
    } else {
        data.len()
    };
    // Base64 expands 3 bytes into 4 characters, so decoded length ≈ chars / 4 * 3.
    let approx_decoded = significant as u64 / 4 * 3;
    if approx_decoded > max_bytes {
        return Err(Error::TooLarge {
            limit: max_bytes,
            actual: Some(approx_decoded),
        });
    }

    let decoded = if has_ws {
        let cleaned: Vec<u8> = data.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        engine.decode(cleaned)
    } else {
        engine.decode(data.as_bytes())
    };
    decoded.map_err(|error| Error::DataUri(error.to_string()))
}

/// Percent-decode a data-URI payload, stopping as soon as the output would
/// exceed the cap so a pathological URI cannot allocate without bound.
fn percent_decode_capped(data: &str, max_bytes: u64) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    for byte in percent_decode_str(data) {
        bytes.push(byte);
        if bytes.len() as u64 > max_bytes {
            return Err(Error::TooLarge {
                limit: max_bytes,
                actual: None,
            });
        }
    }
    Ok(bytes)
}
