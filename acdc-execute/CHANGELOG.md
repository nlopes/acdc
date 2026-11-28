# Changelog

All notable changes to `acdc-execute` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `acdc execute` runs listing and source blocks with the `command` role,
  including blocks in table cells and included files. The `interpreter`
  attribute overrides the source language.
- Scripts keep their literal text. `subs=attributes` or `subs=+attributes`
  expands document attributes, using their values at the command's position.
  A missing attribute fails before any command runs.
- Selecting a command also selects its prerequisites. A failed prerequisite
  blocks its dependents. `--exit-on-failure` stops all commands at the first
  failure. Invalid command input fails before any command runs.
- `--cwd` and repeated `--env` options set the working directory and
  environment variables for commands.
- `--list` shows each command's direct prerequisites and enclosing section
  title.
- Diagnostics point to malformed commands and dependency errors, also in
  included files.

### Changed

- Planning which commands to run uses less temporary memory.
- Explicit `subs=` settings on commands work in every build.

### Fixed

- With `subs=attributes`, attribute names in scripts now follow the parser's
  rules for the compatibility mode.
- Commands can now use a `[[id]]` anchor as their ID. An explicit `id=` still
  takes precedence.
