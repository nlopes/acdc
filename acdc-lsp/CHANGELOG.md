# Changelog

All notable changes to `acdc-lsp` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Tracing spans on all `LanguageServer` methods (`lsp/*` naming convention) for request timing visibility. Latency-sensitive methods (`completion`, `hover`, `gotoDefinition`, `semanticTokensFull`, `formatting`) use `info` level; others use `debug`.

### Changed

- Narrowed tokio dependency from `full` to only the required features (`macros`, `rt-multi-thread`, `io-std`)

## [0.2.0] - 2026-03-28

### Added

- **On-type formatting** — pressing Enter after a list item auto-inserts the
  list marker on the next line (supports `*`, `-`, `.`, `N.`, `<N>` markers
  with nesting and numbering). Pressing Enter on an empty list item removes
  it. Opening a delimited block (`----`, `====`, `....`, etc.) and pressing
  Enter auto-inserts the matching closing delimiter.
- **Signature help for macros** — when editing inside the attribute list `[...]`
  of a macro like `image::`, `include::`, `link:`, etc., the editor shows a
  tooltip with expected positional and named parameters. The current parameter
  is highlighted based on cursor position. Triggered by `[` and `,`.
- **Full document outline** — the document symbol tree now includes all block
  types (paragraphs, admonitions, lists, delimited blocks, images, media,
  discrete headers, breaks) with proper nesting, not just sections.
- **Call hierarchy for includes** — implements `textDocument/prepareCallHierarchy`,
  `callHierarchy/incomingCalls`, and `callHierarchy/outgoingCalls` to navigate
  include-tree relationships. "Outgoing calls" shows what a document includes;
  "Incoming calls" shows which documents include a given file. Works across
  open documents and non-open workspace files.
- **Macro completion snippets** — typing a macro name prefix (e.g., `ima`, `kbd`,
  `menu`) triggers snippet completions that expand to the full macro syntax with
  tab stops for arguments. Supports all inline macros (`image:`, `link:`, `kbd:`,
  `btn:`, `menu:`, `footnote:`, `pass:`, `stem:`, `xref:`, etc.) and block macros
  (`image::`, `audio::`, `video::`, `toc::`, `include::`) when at line start.
- **Conditional directive awareness** — `ifdef`/`ifndef` blocks are now detected
  and inactive branches are grayed out based on defined document attributes.
  Uses semantic tokens with a custom "disabled" modifier and `DiagnosticTag::Unnecessary`
  for universal editor support. Conditional directive lines are highlighted as keywords.
- **Include path completion** — filesystem traversal completion for `include::`
  directives. Suggests files and directories as the user types the path, with
  AsciiDoc files prioritized. Selecting a directory re-triggers completion for
  continued path navigation.
- **Automatic link updates on file rename** — when an AsciiDoc file is renamed
  or moved in the editor, all cross-file references (xrefs, includes) across the
  workspace are automatically updated (`workspace/willRenameFiles`). Also scans
  non-open workspace files on disk for comprehensive coverage.
- **Link validation diagnostics** — flag missing images, audio, video, and
  include files with warning-level diagnostics. Resolves image paths through
  the `imagesdir` attribute when set. URLs and icon names are skipped.
- **Inlay hints** — show resolved attribute values and cross-reference titles as
  ghost text inline (`textDocument/inlayHint`). Attribute references like
  `{product-name}` display their resolved value; xrefs like `<<setup>>` show the
  target section title.
- **Selection range** — smart expand/shrink selection based on AST structure
  (`textDocument/selectionRange`). Progressively selects larger syntactic units:
  word → inline markup → block → section → document.
- **CodeLens** — word/character counts, section preview, and include resolution
  status shown inline above blocks via `textDocument/codeLens`.
- **Section level validation warnings** — warn about skipped heading levels
  (e.g., jumping from `==` to `====`) via WARNING diagnostics. Converts parser
  `NestedSectionLevelMismatch` errors and walks the AST for top-level section
  level skips.
- **Document formatting** (`textDocument/formatting`, `textDocument/rangeFormatting`) — normalize
  whitespace for clean diffs: trim trailing whitespace, collapse multiple blank lines to one,
  ensure final newline, insert blank lines between adjacent top-level blocks. Verbatim blocks
  (listing, literal, passthrough, comment, verse, stem) are protected from formatting changes.
  Falls back to text-based delimiter detection when the AST is unavailable (parse errors).
- **Code actions** (`textDocument/codeAction`) — quick-fixes, refactorings, and source actions:
  - Quick-fix: create missing anchor for unresolved cross-references.
  - Wrap in block: wrap selected text in sidebar, example, listing, literal, open, comment,
    passthrough, or quote block delimiters.
  - Generate TOC: insert `:toc:` attribute in document header.
- **Workspace symbols** (`workspace/symbol`) — search sections, anchors, discrete headers, block
  titles, and document attributes across all project files. Scans workspace roots for `.adoc`,
  `.asciidoc`, and `.asc` files on initialization; open documents use live ASTs while closed files
  use cached symbols.

  ![Workspace symbols in Emacs](images/workspace-symbols-emacs.png)
- **Cross-file reference support** — workspace-wide anchor indexing across all open documents.
  - Go-to-definition navigates between files via `xref:file.adoc#anchor[text]`.
  - Hover shows cross-file target information (file name, anchor status).
  - Find references discovers xrefs across all open documents.
  - Completion suggests anchors from other open files with `file.adoc#anchor` insertion.
  - Rename updates anchor IDs and all xrefs across all open files.
  - Diagnostics emit info-level notes for cross-file xrefs instead of false warnings.
- **Include directive links** — `include::file.adoc[]` directives appear as clickable document
  links in the editor.
- **Relative path resolution** — relative paths in link macros and images are now resolved
  against the document's directory and appear as clickable links.

### Fixed

- **Cross-file xref go-to-definition** — fallback to global anchor index when direct file+anchor
  lookup fails, mirroring the existing pattern for local xrefs.
- **Cross-file xref to unopened files** — read and parse target files from disk when they aren't
  open in the editor, so go-to-definition works without opening the target file first.
- **Cross-file xref find-references** — include anchor definition from on-disk files when the
  target document isn't open, so find-references shows the definition location.
- **Cross-file xref diagnostics** — suppress info diagnostic for cross-file xrefs when the target
  anchor is found in the workspace-wide index.

### Changed

- Switched from `tower-lsp` to `tower-lsp-server` crate (community-maintained successor)


## [0.1.0] - 2025-12-28

Initial release of acdc-lsp, a Language Server Protocol implementation for AsciiDoc.

### Added

- Go-to-definition support
- Hover information
- Completion suggestions
- Diagnostics
- Semantic tokens

[Unreleased]: https://github.com/nlopes/acdc/compare/acdc-lsp-v0.2.0...HEAD
[0.2.0]: https://github.com/nlopes/acdc/compare/acdc-lsp-v0.1.0...acdc-lsp-v0.2.0
[0.1.0]: https://github.com/nlopes/acdc/releases/tag/acdc-lsp-v0.1.0
