# Changelog

All notable changes to `acdc-cli` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- `convert` makes the selected backend's attributes and the converter's default
  attributes available while the document is parsed, so backend conditionals such
  as `ifdef::backend-pdf[]` and references to `backend`, `basebackend`, `filetype`,
  `outfilesuffix`, and `htmlsyntax` reflect the chosen output during parsing —
  consistently for both stdin and file inputs.
- CLI subcommand errors now exit with a non-zero status after rendering the
  error message. This includes `acdc lint` runs with denied diagnostics.
- Missing `convert` and `lint` inputs now produce normal command usage errors,
  unsupported TCK input types return a regular command failure, and multi-file
  conversion failures are reported without bypassing top-level error handling.
- A warning that points into an `include::`d file (or any content shifted by the
  preprocessor) now renders its source snippet against the correct file instead of
  aborting with `Failed to read contents … OutOfBounds`.
- Diagnostics for empty files, multi-byte UTF-8 text, and stale or out-of-range
  locations now clamp to valid source spans instead of producing an invalid
  source-snippet range.
- `--no-default-features` builds no longer re-enable parser default features
  through internal workspace dependencies.
- Peak memory during multi-file conversion is now bounded by active work;
  parsed documents are released after their conversion instead of being
  retained for the complete input batch.
- Terminal backend warnings are now visible. Parser and converter warnings
  emitted during a `--backend terminal` run with a pager were previously
  written to stderr before the pager took over the screen, leaving them
  visually buried. They are now deferred until after the pager exits, and the
  no-pager terminal paths also render converter warnings, including file input
  converted with `--out-file`.
- A PDF-only build (`--no-default-features --features pdf`) now exposes the
  `convert` command instead of reporting that the binary has no subcommands.
- Default builds now include the runtime `convert --setext` compatibility flag,
  with `--enable-setext-compatibility` retained as an alias. The `highlighting`
  build feature now also enables terminal-backend source highlighting when that
  backend is selected.
- `inspect` now resolves includes relative to the inspected file, handles long
  Unicode text without panicking, renders accurate tree relationships, and
  omits ANSI styling when output is redirected.

### Added

- A new `lint` command is available by default. It accepts files or `--stdin`
  and Clippy-style lint level flags (`--allow`/`-A`, `--warn`/`-W`,
  `--deny`/`-D`, `--forbid`/`-F`) for the initial Asciidoctor recommended
  practices lint names. The `document-title-author` and
  `document-title-revision` lints are available by name but are not part of
  the `recommended-practices` group. Counter naming is not exposed as a lint
  because acdc already warns that counters are unsupported and removes them
  from output.
- `acdc lint` now renders full colored diagnostics by default, including source
  snippets, lint IDs, labels, and help text. Use `--output-style=compact` for
  compact `line:column` diagnostics without colors or snippets.
- Full `acdc lint` output now ends with lint statistics that count diagnostics
  by lint ID for the run.
- `acdc lint` level flags now accept location-scoped overrides for individual
  lint IDs, such as `-A section-title-capitalization@37` or
  `-D image-alt-text@10:1-10:80`. Multiple locations can be comma-separated in
  one flag, such as `-A delimited-block-minimal-delimiter@977,968`. If a scoped
  override no longer matches any diagnostic, the lint run reports the stale
  location.
- `--backend pdf` is now available when the `pdf` or `all-backends` feature is
  enabled. It writes `.pdf` files by default and writes raw PDF bytes when
  `-o -` is selected. PDF runs accept `--font-dir`, `--logo`, `--title`,
  `--watermark`, `--watermark-timestamp`, `--page`, `--theme`, `--plain`,
  `--toc`, and `--emit-typst`; `--strict` now makes unresolved PDF images or
  logos fail instead of falling back with a warning.
- The `terminal-emulator` build feature renders `[terminal]` session blocks
  through `libghostty-vt` on the `--backend terminal` path. Requires a Zig
  toolchain to build the bundled library, which is statically linked so the
  binary stays self-contained.
- The `html-terminal` feature forwards terminal-styled HTML rendering to the
  HTML converter. The `:acdc-terminal:` document attribute opts terminal-like
  source blocks into selectable preview rendering; Asciidoctor does not provide
  this attribute or an equivalent built-in feature.
- Explicit `[terminal]` blocks now render in HTML output when the
  `html-terminal` feature is enabled. `[terminal]` is the terminal-session
  path: it renders transcripts through `libghostty-vt` as selectable styled
  HTML and does not require the `:acdc-terminal:` source-block opt-in. It
  supports per-block `cols=` and `rows=` attributes and follows the document
  `:dark-mode:` setting. `[terminal]` is an acdc-only block style: Asciidoctor
  renders it as a plain listing or literal block with the raw text (ANSI
  escapes included) left as-is.
- Converter warnings are now rendered on stderr with the same miette warning
  styling used for parser warnings.
- `convert --open` now opens converter-reported output files, including
  volume-aware manpage outputs such as `cmd.1` and `cmd.7`.
- **`--variant` flag for `convert`** — pick a backend-specific output style.
  HTML: `standard` (default) or `semantic`. Markdown: `commonmark` or `gfm`
  (default). `--backend html5s` remains an alias for `--backend html --variant
  semantic` and rejects `--variant`. Invalid combinations are rejected up-front,
  and unavailable backends or variants are omitted from command help.
- **Timing summary table for multi-file conversions** — when `convert` is invoked with
  `--timings` and more than one input file, a summary table is printed after the
  per-file output with parse time, convert time, total per-file time, and wall-clock
  time across the batch. Single-file output is unchanged.
- **Parser warnings on stderr** — non-fatal parser warnings (section level out of
  sequence, unknown table formats, missing includes, `ifdef`/`endif` mismatches,
  etc.) are now shown on stderr with the same colored source-snippet treatment
  as errors: a yellow caret, framed source excerpt, span marker, and an optional
  `help:` line. Previously these were silent without a tracing subscriber.

## [0.2.0] - 2026-03-28

### Added

- **Automatic pager for terminal output** - When using `--backend terminal` and stdout is
  a TTY, output is automatically piped through a pager. Defaults to `less -FRX` on Unix
  and `more` on Windows. Respects the `PAGER` environment variable for custom pagers. Use
  `--no-pager` to disable, or set `PAGER=""`. On Unix, sets `LESSCHARSET=utf-8` for proper
  UTF-8 display. ([#311])
- `--embedded` / `-e` flag to output embeddable content without document wrapper elements.
  Behaviour varies by backend: HTML skips DOCTYPE/html/head/body/content wrapper, manpage
  skips preamble and NAME section, terminal skips header/authors/ revision info. Matches
  asciidoctor's `--embedded` behaviour. ([#272])
- **Index catalog rendering for HTML** - Documents with `[index]` sections now generate a
  fully populated index catalog, organized alphabetically by first letter with
  hierarchical nesting for secondary and tertiary terms. Each entry links back to the
  source location via inline anchors. This goes beyond asciidoctor's HTML backend which
  leaves index sections empty. The index only renders when it's the last section in the
  document.
- **Semantic HTML5 backend** — `--backend html5s` produces semantic HTML5 output with proper
  elements and ARIA roles instead of div-based layout. ([#329])
- `--open` flag to open generated output files after conversion using the system's default
  application (e.g., browser for HTML). Ignored when output goes to stdout. ([#330])

### Fixed

- Horizontal description lists (`[horizontal]`) now render as `<table>` with `hdlist`
  class instead of `<dl>` with `dlist horizontal`, matching asciidoctor output ([#270])
- List titles (`.My title` syntax) now render correctly in HTML and manpage output.
  HTML uses `<div class="title">`, manpage uses bold formatting, matching asciidoctor
  behaviour. Terminal output already supported this. ([#273])

## [0.1.0] - 2026-01-02

This is tagged but unreleased in crates.io for now.

### Added

- Description lists now support roles (e.g., `[.stack]`) which are applied to the wrapper
  `<div>` element, matching asciidoctor behaviour ([#264])

### Changed

- Removed dependency on `acdc-core` (purely internal change so no need to bump minor
  version)

[#264]: https://github.com/nlopes/acdc/issues/264
[#270]: https://github.com/nlopes/acdc/issues/270
[#272]: https://github.com/nlopes/acdc/issues/272
[#273]: https://github.com/nlopes/acdc/issues/273
[#311]: https://github.com/nlopes/acdc/issues/311
[#329]: https://github.com/nlopes/acdc/issues/329
[#330]: https://github.com/nlopes/acdc/issues/330

[Unreleased]: https://github.com/nlopes/acdc/compare/acdc-cli-v0.2.0...HEAD
[0.2.0]: https://github.com/nlopes/acdc/compare/acdc-cli-v0.1.0...acdc-cli-v0.2.0
[0.1.0]: https://github.com/nlopes/acdc/releases/tag/acdc-cli-v0.1.0
