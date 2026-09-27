# Changelog

All notable changes to `acdc-execute` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Library callers now import command-discovery errors as
  `acdc_execute::DiscoveryError`.
- Building `acdc-execute` no longer enables unused graph implementations in its
  dependencies.

### Added

- Creating execution plans uses less temporary memory.
- Discover and run listing and source blocks marked with the `command` role,
  including commands in nested containers, AsciiDoc table cells, and included files.
  Scripts retain their literal text after includes and conditionals; enable
  textual document-attribute expansion with `subs=attributes` or `subs=+attributes`.
  Expansion uses attributes at the command's source position and preserves
  callouts. Missing enabled attributes fail before execution. Builds without
  `pre-spec-subs` reject explicit `subs` settings. The `interpreter` attribute
  overrides the source language.
- Select commands with their dependencies and inspect execution outcomes.
  A failed prerequisite blocks its dependents; callers can stop all commands at
  the first failure. Infrastructure errors and signal termination always stop
  execution. Invalid or incomplete command input fails before execution.
- Configure child working directories and environment variables while preserving
  inherited defaults. Source diagnostics identify malformed commands and
  dependency errors, including their locations in included files.
