# Changelog

All notable changes to `acdc-pdf-images` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Padded base64 image data is accepted when its decoded file size is exactly
  the configured byte limit, including payloads with whitespace.

- Percent-encoded local image targets load files whose paths contain the
  corresponding decoded characters, including spaces encoded as `%20`.

### Changed

- Image resolution now limits each call to 1,024 distinct source attempts and
  100 MiB of accepted image data by default. Raise `max_sources` or
  `max_total_bytes` for trusted inputs. Failed attempts use source slots;
  different source strings each use bytes even when their content matches.
  Later images are not fetched after a quota is reached or would be exceeded.
  Earlier images remain available. These limits are specific to acdc.

- The crate now lives under `converters/pdf/crates` as a non-publishable implementation
  component of `acdc-converters-pdf`; its Cargo package name remains unchanged.

### Added

- Initial release for a PDF image resolver for local, remote, `file://`, and `data:` URI
  images (scheme matching is case-insensitive). Each validated image is snapshotted into an
  explicit caller-owned spool and handed to the renderer as a path, never retained bytes.
