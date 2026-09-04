# Changelog

All notable changes to `acdc-execute` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial release of the command-block runner library. AsciiDoc listing (or
  source) blocks carrying the `command` role and an explicit `id` are
  discovered from a parsed document — including blocks inside nested
  containers and included files — and validated: duplicate ids, unknown
  dependencies, dependency cycles, missing ids, non-listing command blocks,
  and invalid ids are all rejected with diagnostics that name the offending
  command and source line.
- Commands declare prerequisites with a `deps` attribute
  (`deps="build, gen"`); the resulting command graph yields execution queues
  in topological order, and selecting a command includes its transitive
  dependencies exactly once each.
- `[source, <lang>]` headers select the interpreter a script runs under
  (default `sh`). The value is passed directly to the operating system as an
  executable name or path without an allowlist, so executing a document is not
  a sandbox boundary. Scripts are written to a temporary file passed to that
  interpreter; commands run in the caller's working directory and inherit its
  environment. A non-zero exit is reported as a command failure; a script
  that cannot be written or an interpreter that cannot be spawned is
  reported as an infrastructure failure.
