# Changelog

All notable changes to `acdc-pdf-images` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Remote image timeouts during response-body reads now return `Timeout`,
  matching timeouts before the response headers arrive. Other images still
  resolve after a timeout.

- Padded base64 image data is accepted when its decoded file size is exactly
  the configured byte limit, including payloads with whitespace.

- Percent-encoded local image targets load files whose paths contain the
  corresponding decoded characters, including spaces encoded as `%20`.

### Changed

- Unsupported image-format errors now use the American spelling "unrecognized".

- **Breaking:** `ResolveConfig` fields are private. Keep both directory
  arguments in `ResolveConfig::new` and set optional values with
  `with_timeout`, `with_max_bytes`, `with_max_sources`, `with_max_total_bytes`,
  and `with_source_policy`. Defaults and image resolution behavior are unchanged.

- Image resolution now limits each call to 1,024 distinct source attempts and
  100 MiB of accepted image data by default. Use `with_max_sources` or
  `with_max_total_bytes` to raise the limits for trusted inputs. Failed attempts
  use source slots; different source strings each use bytes even when their
  content matches.
  Later images are not fetched after a quota is reached or would be exceeded.
  Earlier images remain available. These limits are specific to acdc.

- The crate now lives under `converters/pdf/crates` as a non-publishable implementation
  component of `acdc-converters-pdf`; its Cargo package name remains unchanged.

### Added

- Remote images use up to two simultaneous requests by default.
  `with_remote_concurrency(1)` selects sequential fetching; larger values
  permit more requests. Source and byte quotas can reduce the batch size.
  Local sources, validation, and snapshot writes
  remain sequential, and failures keep input order.

- Initial release for a PDF image resolver for local, remote, `file://`, and `data:` URI
  images (scheme matching is case-insensitive). Each validated image is snapshotted into an
  explicit caller-owned spool and handed to the renderer as a path, never retained bytes.
