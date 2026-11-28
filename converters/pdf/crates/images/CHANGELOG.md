# Changelog

All notable changes to `acdc-pdf-images` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- PDF output loads local, remote, `file://` and `data:` images.
- Remote images load two at a time by default. `--image-concurrency N` changes
  the number, and 1 loads them one at a time.

### Changed

- Image loading is limited to 1,024 distinct sources and 100 MiB of image
  data. Failed attempts count as sources. Asciidoctor PDF has no such limits.

### Fixed

- A remote image that times out during download now reports a timeout.
- Padded base64 data at exactly the byte limit is now accepted.
- Percent-encoded local image paths, such as `%20`, now load.
