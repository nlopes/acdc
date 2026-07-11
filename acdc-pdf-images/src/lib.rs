//! Resolves image references (local paths, remote URLs, `data:` URIs) to
//! validated snapshots on disk, keyed by a stable, content-hashed virtual path
//! (identical bytes share one path).
//!
//! The resolved bytes are **not** retained: every source is spooled to the
//! caller-owned directory after validation. The renderer reads each snapshot on
//! demand, so a document with many large images never pins them all in memory
//! here — at most one image is held transiently while it is fetched, validated,
//! hashed, and written.
//!
//! This crate is IR-agnostic: it takes a slice of URL strings and returns a
//! map. The tree walk that collects those URLs lives in the consuming converter.
#![forbid(unsafe_code)]

mod detect;
mod error;
mod fetch;
mod map;
mod path;
mod spool;

use std::{collections::HashSet, path::PathBuf, time::Duration};

pub use error::Error;
pub use map::{ImageMap, ResolvedImage};

/// Which image sources the resolver may access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourcePolicy {
    /// Allow local files, `data:` URIs, and remote URLs.
    Unrestricted,
    /// Restrict local files to [`ResolveConfig::base_dir`] and optionally allow
    /// remote URLs. `data:` URIs remain available.
    Confined { allow_network: bool },
    /// Reject every image source.
    DenyAll,
}

/// Options controlling image resolution.
#[derive(Debug)]
pub struct ResolveConfig {
    /// Directory relative image paths are resolved against.
    pub base_dir: PathBuf,
    /// Directory that validated image snapshots are spooled into. The caller
    /// owns this directory and is responsible for cleaning it up once rendering
    /// is done.
    pub spool_dir: PathBuf,
    /// Per-request timeout for remote fetches.
    pub timeout: Duration,
    /// Maximum bytes accepted for a single image, from any source (local file,
    /// remote response, or `data:` URI). Anything larger is a per-image failure.
    pub max_bytes: u64,
    /// Sources the resolver may access.
    pub source_policy: SourcePolicy,
}

impl ResolveConfig {
    /// Create a resolver configuration with an explicit base and private spool
    /// directory.
    #[must_use]
    pub fn new(base_dir: impl Into<PathBuf>, spool_dir: impl Into<PathBuf>) -> Self {
        ResolveConfig {
            base_dir: base_dir.into(),
            spool_dir: spool_dir.into(),
            timeout: Duration::from_secs(30),
            max_bytes: 20 * 1024 * 1024,
            source_policy: SourcePolicy::Unrestricted,
        }
    }
}

/// A single image reference that could not be resolved.
#[derive(Debug)]
pub struct ResolveFailure {
    pub url: String,
    pub error: Error,
}

/// The outcome of resolving a set of image references.
#[derive(Debug)]
pub struct Resolved {
    pub assets: ImageMap,
    pub failures: Vec<ResolveFailure>,
}

/// Resolve every image reference, embedding what it can and recording per-URL
/// failures so a single broken image never aborts the document.
///
/// Repeated references to the same source string are resolved once: a document
/// that uses one logo in fifty places fetches and decodes it a single time, and
/// a broken reference is reported once rather than once per occurrence.
#[must_use]
pub fn resolve(urls: &[&str], config: &ResolveConfig) -> Resolved {
    let fetcher = fetch::Fetcher::new(config);
    let mut assets = ImageMap::new();
    let mut failures = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for &url in urls {
        if !seen.insert(url) {
            continue;
        }
        match resolve_one(&fetcher, url, &config.spool_dir) {
            Ok(image) => assets.insert(url.to_string(), image),
            Err(error) => failures.push(ResolveFailure {
                url: url.to_string(),
                error,
            }),
        }
    }
    Resolved { assets, failures }
}

fn resolve_one(
    fetcher: &fetch::Fetcher,
    url: &str,
    spool_dir: &std::path::Path,
) -> Result<ResolvedImage, Error> {
    let raw = fetcher.fetch(url)?;
    let detected = detect::detect(&raw)?;
    let name = path::content_name(&raw, detected.extension);
    let virtual_path = path::virtual_path(&name);
    let path = spool::write(spool_dir, &name, &raw)?;
    Ok(ResolvedImage { virtual_path, path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    // A valid 1×1 PNG.
    const PNG_1X1_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABAQMAAAAl21bKAAAAA1BMVEXyVTNpJlJjAAAACklEQVQI12NgAAAAAgAB4iG8MwAAAABJRU5ErkJggg==";

    fn png_bytes() -> Result<Vec<u8>, base64::DecodeError> {
        base64::engine::general_purpose::STANDARD.decode(PNG_1X1_B64)
    }

    fn missing_image(url: &str) -> std::io::Error {
        std::io::Error::other(format!("image {url} was not resolved"))
    }

    fn has_extension(path: &str, extension: &str) -> bool {
        std::path::Path::new(path)
            .extension()
            .is_some_and(|ext| ext == extension)
    }

    fn spool() -> std::io::Result<tempfile::TempDir> {
        tempfile::tempdir()
    }

    fn spooled_config(spool: &tempfile::TempDir) -> ResolveConfig {
        ResolveConfig::new(".", spool.path())
    }

    fn assets_are_empty(assets: &ImageMap) -> bool {
        assets.images().next().is_none()
    }

    #[test]
    fn resolves_data_uri_png() -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("data:image/png;base64,{PNG_1X1_B64}");
        let spool = spool()?;
        let resolved = resolve(&[url.as_str()], &spooled_config(&spool));
        assert!(resolved.failures.is_empty(), "{:?}", resolved.failures);
        let image = resolved
            .assets
            .get(&url)
            .ok_or_else(|| missing_image(&url))?;
        assert!(has_extension(&image.virtual_path, "png"));
        Ok(())
    }

    #[test]
    fn resolves_local_file_relative_to_base_dir() -> Result<(), Box<dyn std::error::Error>> {
        let base = tempfile::tempdir()?;
        std::fs::write(base.path().join("pic.png"), png_bytes()?)?;
        let spool = spool()?;
        let mut config = ResolveConfig::new(base.path(), spool.path());
        config.source_policy = SourcePolicy::Confined {
            allow_network: false,
        };
        let resolved = resolve(&["pic.png"], &config);
        assert!(resolved.failures.is_empty(), "{:?}", resolved.failures);
        assert!(resolved.assets.get("pic.png").is_some());
        Ok(())
    }

    #[test]
    fn local_file_is_spooled_as_a_stable_snapshot() -> Result<(), Box<dyn std::error::Error>> {
        let base = tempfile::tempdir()?;
        let source = base.path().join("pic.png");
        let original = png_bytes()?;
        std::fs::write(&source, &original)?;
        let spool = spool()?;
        let config = ResolveConfig::new(base.path(), spool.path());
        let resolved = resolve(&["pic.png"], &config);
        let image = resolved
            .assets
            .get("pic.png")
            .ok_or_else(|| missing_image("pic.png"))?;
        assert!(image.path.starts_with(spool.path()));

        std::fs::remove_file(source)?;
        assert_eq!(std::fs::read(&image.path)?, original);
        Ok(())
    }

    #[test]
    fn data_uri_is_spooled_to_disk() -> Result<(), Box<dyn std::error::Error>> {
        // A `data:` image has no on-disk home, so it is written into the spool
        // and the resolved path points there — the bytes are not kept in memory.
        let url = format!("data:image/png;base64,{PNG_1X1_B64}");
        let spool = spool()?;
        let resolved = resolve(&[url.as_str()], &spooled_config(&spool));
        let image = resolved
            .assets
            .get(&url)
            .ok_or_else(|| missing_image(&url))?;
        assert!(image.path.starts_with(spool.path()));
        assert!(image.path.exists(), "spooled file should be on disk");
        Ok(())
    }

    #[test]
    fn image_over_max_bytes_is_a_failure() -> Result<(), Box<dyn std::error::Error>> {
        // A well-formed PNG that is rejected purely for exceeding the size cap,
        // proving the limit is enforced on local files, not just remote ones.
        let base = tempfile::tempdir()?;
        std::fs::write(base.path().join("big.png"), png_bytes()?)?;
        let spool = spool()?;
        let mut config = ResolveConfig::new(base.path(), spool.path());
        config.max_bytes = 4;
        let resolved = resolve(&["big.png"], &config);
        assert_eq!(resolved.failures.len(), 1);
        assert!(assets_are_empty(&resolved.assets));
        Ok(())
    }

    #[test]
    fn missing_local_file_is_a_failure_not_a_panic() -> Result<(), Box<dyn std::error::Error>> {
        let spool = spool()?;
        let resolved = resolve(&["does-not-exist.png"], &spooled_config(&spool));
        assert_eq!(resolved.failures.len(), 1);
        assert!(assets_are_empty(&resolved.assets));
        Ok(())
    }

    #[test]
    fn identical_bytes_share_a_virtual_path() -> Result<(), Box<dyn std::error::Error>> {
        let a = format!("data:image/png;base64,{PNG_1X1_B64}");
        let b = format!("data:image/png;base64, {PNG_1X1_B64}"); // whitespace ignored
        let spool = spool()?;
        let resolved = resolve(&[a.as_str(), b.as_str()], &spooled_config(&spool));
        let path_a = &resolved
            .assets
            .get(&a)
            .ok_or_else(|| missing_image(&a))?
            .virtual_path;
        let path_b = &resolved
            .assets
            .get(&b)
            .ok_or_else(|| missing_image(&b))?
            .virtual_path;
        assert_eq!(path_a, path_b);
        Ok(())
    }

    #[test]
    fn detects_svg_by_content() -> Result<(), Box<dyn std::error::Error>> {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"></svg>";
        let url = format!("data:image/svg+xml,{svg}");
        let spool = spool()?;
        let resolved = resolve(&[url.as_str()], &spooled_config(&spool));
        assert!(resolved.failures.is_empty(), "{:?}", resolved.failures);
        let image = resolved
            .assets
            .get(&url)
            .ok_or_else(|| missing_image(&url))?;
        assert!(has_extension(&image.virtual_path, "svg"));
        Ok(())
    }

    #[test]
    fn malformed_svg_is_a_failure_not_embedded() -> Result<(), Box<dyn std::error::Error>> {
        // Sniffs as SVG but is malformed XML (mismatched tags): `usvg` rejects
        // it, so Typst would too. Catch it here as a per-image failure instead
        // of letting it abort the whole compile.
        let url = "data:image/svg+xml,<svg xmlns=\"http://www.w3.org/2000/svg\"><g></svg>";
        let spool = spool()?;
        let resolved = resolve(&[url], &spooled_config(&spool));
        assert_eq!(resolved.failures.len(), 1, "{:?}", resolved.failures);
        assert!(assets_are_empty(&resolved.assets));
        assert!(
            resolved
                .failures
                .first()
                .is_some_and(|failure| matches!(failure.error, Error::Svg(_)))
        );
        Ok(())
    }

    #[test]
    fn percent_encoded_data_uri_svg_resolves() -> Result<(), Box<dyn std::error::Error>> {
        // A non-base64 `data:` payload is percent-encoded (RFC 2397); the escapes
        // must be decoded before format detection.
        let url = "data:image/svg+xml,%3Csvg%20xmlns=%22http://www.w3.org/2000/svg%22%20width=%2210%22%20height=%2210%22/%3E";
        let spool = spool()?;
        let resolved = resolve(&[url], &spooled_config(&spool));
        assert!(resolved.failures.is_empty(), "{:?}", resolved.failures);
        let image = resolved.assets.get(url).ok_or_else(|| missing_image(url))?;
        assert!(has_extension(&image.virtual_path, "svg"));
        Ok(())
    }

    #[test]
    fn svg_external_image_references_are_not_loaded() -> Result<(), Box<dyn std::error::Error>> {
        let external = tempfile::NamedTempFile::new()?;
        std::fs::write(external.path(), b"not an image")?;
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><image href=\"{}\"/></svg>",
            external.path().display()
        );
        let url = format!("data:image/svg+xml,{svg}");
        let spool = spool()?;
        let resolved = resolve(&[url.as_str()], &spooled_config(&spool));

        assert!(resolved.failures.is_empty(), "{:?}", resolved.failures);
        assert!(resolved.assets.get(&url).is_some());
        Ok(())
    }

    #[test]
    fn deny_all_policy_rejects_data_uris() -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("data:image/png;base64,{PNG_1X1_B64}");
        let spool = spool()?;
        let mut config = spooled_config(&spool);
        config.source_policy = SourcePolicy::DenyAll;
        let resolved = resolve(&[url.as_str()], &config);

        assert!(
            resolved
                .failures
                .first()
                .is_some_and(|failure| { matches!(failure.error, Error::AccessDenied(_)) })
        );
        Ok(())
    }

    #[test]
    fn confined_policy_rejects_files_outside_base() -> Result<(), Box<dyn std::error::Error>> {
        let base = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let outside_image = outside.path().join("outside.png");
        std::fs::write(&outside_image, png_bytes()?)?;
        let outside_url = url::Url::from_file_path(&outside_image)
            .map_err(|()| std::io::Error::other("could not build file URL"))?;
        let spool = spool()?;
        let mut config = ResolveConfig::new(base.path(), spool.path());
        config.source_policy = SourcePolicy::Confined {
            allow_network: true,
        };
        let resolved = resolve(&[outside_url.as_str()], &config);

        assert!(
            resolved
                .failures
                .first()
                .is_some_and(|failure| { matches!(failure.error, Error::AccessDenied(_)) })
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn confined_policy_rejects_symlinks_outside_base() -> Result<(), Box<dyn std::error::Error>> {
        let base = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let outside_image = outside.path().join("outside.png");
        std::fs::write(&outside_image, png_bytes()?)?;
        std::os::unix::fs::symlink(&outside_image, base.path().join("link.png"))?;
        let spool = spool()?;
        let mut config = ResolveConfig::new(base.path(), spool.path());
        config.source_policy = SourcePolicy::Confined {
            allow_network: true,
        };
        let resolved = resolve(&["link.png"], &config);

        assert!(
            resolved
                .failures
                .first()
                .is_some_and(|failure| { matches!(failure.error, Error::AccessDenied(_)) })
        );
        Ok(())
    }

    #[test]
    fn confined_policy_can_disable_network() -> Result<(), Box<dyn std::error::Error>> {
        let spool = spool()?;
        let mut config = spooled_config(&spool);
        config.source_policy = SourcePolicy::Confined {
            allow_network: false,
        };
        let resolved = resolve(&["https://example.com/pic.png"], &config);

        assert!(
            resolved
                .failures
                .first()
                .is_some_and(|failure| { matches!(failure.error, Error::AccessDenied(_)) })
        );
        Ok(())
    }

    #[test]
    fn base64_data_uri_over_cap_is_too_large() -> Result<(), Box<dyn std::error::Error>> {
        // The oversized payload is rejected from its length before it is decoded.
        let url = format!("data:image/png;base64,{PNG_1X1_B64}");
        let spool = spool()?;
        let mut config = spooled_config(&spool);
        config.max_bytes = 4;
        let resolved = resolve(&[url.as_str()], &config);
        assert_eq!(resolved.failures.len(), 1, "{:?}", resolved.failures);
        assert!(
            resolved
                .failures
                .first()
                .is_some_and(|failure| matches!(failure.error, Error::TooLarge { .. }))
        );
        Ok(())
    }

    #[test]
    fn duplicate_references_are_reported_once() -> Result<(), Box<dyn std::error::Error>> {
        // The same broken reference used twice yields a single failure, proving
        // resolution deduplicates by source string.
        let spool = spool()?;
        let resolved = resolve(&["missing.png", "missing.png"], &spooled_config(&spool));
        assert_eq!(resolved.failures.len(), 1, "{:?}", resolved.failures);
        Ok(())
    }

    #[cfg(feature = "network")]
    #[test]
    fn fetches_remote_image() -> Result<(), Box<dyn std::error::Error>> {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/pic.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(png_bytes()?)
            .create();
        let url = format!("{}/pic.png", server.url());

        let spool = spool()?;
        let resolved = resolve(&[url.as_str()], &spooled_config(&spool));
        mock.assert();
        assert!(resolved.failures.is_empty(), "{:?}", resolved.failures);
        let image = resolved
            .assets
            .get(&url)
            .ok_or_else(|| missing_image(&url))?;
        assert!(has_extension(&image.virtual_path, "png"));
        Ok(())
    }

    #[cfg(feature = "network")]
    #[test]
    fn remote_404_is_reported_as_http_status() -> Result<(), Box<dyn std::error::Error>> {
        let mut server = mockito::Server::new();
        let _mock = server.mock("GET", "/missing.png").with_status(404).create();
        let url = format!("{}/missing.png", server.url());

        let spool = spool()?;
        let resolved = resolve(&[url.as_str()], &spooled_config(&spool));
        assert_eq!(resolved.failures.len(), 1);
        assert!(assets_are_empty(&resolved.assets));
        assert!(
            resolved
                .failures
                .first()
                .is_some_and(|failure| matches!(failure.error, Error::HttpStatus(404)))
        );
        Ok(())
    }

    #[cfg(feature = "network")]
    #[test]
    fn remote_body_over_cap_is_too_large() -> Result<(), Box<dyn std::error::Error>> {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("GET", "/big.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(png_bytes()?)
            .create();
        let url = format!("{}/big.png", server.url());
        let spool = spool()?;
        let mut config = spooled_config(&spool);
        config.max_bytes = 4;

        let resolved = resolve(&[url.as_str()], &config);
        assert_eq!(resolved.failures.len(), 1, "{:?}", resolved.failures);
        assert!(
            resolved
                .failures
                .first()
                .is_some_and(|failure| matches!(failure.error, Error::TooLarge { .. })),
            "expected TooLarge, got {:?}",
            resolved.failures.first().map(|f| &f.error)
        );
        Ok(())
    }

    #[cfg(feature = "network")]
    #[test]
    fn decompressed_remote_body_is_capped() -> Result<(), Box<dyn std::error::Error>> {
        const GZIP_SVG: &[u8] = &[
            0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xb3, 0x29, 0x2e, 0x4b,
            0x57, 0xa8, 0xc8, 0xcd, 0xc9, 0x2b, 0xb6, 0x55, 0xca, 0x28, 0x29, 0x29, 0xb0, 0xd2,
            0xd7, 0x2f, 0x2f, 0x2f, 0xd7, 0x2b, 0x37, 0xd6, 0xcb, 0x2f, 0x4a, 0xd7, 0x37, 0x32,
            0x30, 0x30, 0xd0, 0x07, 0xaa, 0x50, 0x52, 0x28, 0xcf, 0x4c, 0x29, 0xc9, 0xb0, 0x55,
            0x32, 0x54, 0x52, 0xc8, 0x48, 0xcd, 0x4c, 0xcf, 0x28, 0x01, 0x31, 0xed, 0x14, 0x46,
            0xc1, 0x88, 0x06, 0x36, 0xa0, 0xb4, 0x61, 0x07, 0x00, 0x73, 0x78, 0x80, 0xe1, 0x43,
            0x02, 0x00, 0x00,
        ];
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("GET", "/large.svg")
            .with_status(200)
            .with_header("content-type", "image/svg+xml")
            .with_header("content-encoding", "gzip")
            .with_body(GZIP_SVG)
            .create();
        let url = format!("{}/large.svg", server.url());
        let spool = spool()?;
        let mut config = spooled_config(&spool);
        config.max_bytes = 100;
        let resolved = resolve(&[url.as_str()], &config);

        assert!(
            resolved
                .failures
                .first()
                .is_some_and(|failure| { matches!(failure.error, Error::TooLarge { .. }) })
        );
        Ok(())
    }

    #[cfg(not(feature = "network"))]
    #[test]
    fn remote_url_without_network_feature_is_a_failure() -> Result<(), Box<dyn std::error::Error>> {
        let spool = spool()?;
        let resolved = resolve(&["https://example.com/pic.png"], &spooled_config(&spool));
        assert_eq!(resolved.failures.len(), 1);
        assert!(assets_are_empty(&resolved.assets));
        Ok(())
    }

    #[test]
    fn corrupt_image_is_a_failure_not_embedded() -> Result<(), Box<dyn std::error::Error>> {
        // Valid PNG magic + IHDR, but the pixel data is garbage: a decode-time
        // failure that must be caught here rather than aborting the render.
        let mut bytes = png_bytes()?;
        for byte in bytes.iter_mut().skip(40) {
            *byte = 0xff;
        }
        let url = format!("data:image/png;base64,{}", {
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        });
        let spool = spool()?;
        let resolved = resolve(&[url.as_str()], &spooled_config(&spool));
        assert_eq!(resolved.failures.len(), 1, "expected a decode failure");
        assert!(assets_are_empty(&resolved.assets));
        Ok(())
    }
}
