# Changelog

All notable changes to `acdc-lsp` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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

### Fixed

- **Cross-file xref go-to-definition** — fallback to global anchor index when direct file+anchor
  lookup fails, mirroring the existing pattern for local xrefs.
- **Cross-file xref to unopened files** — read and parse target files from disk when they aren't
  open in the editor, so go-to-definition works without opening the target file first.
- **Cross-file xref find-references** — include anchor definition from on-disk files when the
  target document isn't open, so find-references shows the definition location.
- **Cross-file xref diagnostics** — suppress info diagnostic for cross-file xrefs when the target
  anchor is found in the workspace-wide index.

### Added

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

## [0.1.0] - 2025-12-28

Initial release of acdc-lsp, a Language Server Protocol implementation for AsciiDoc.

### Added

- Go-to-definition support
- Hover information
- Completion suggestions
- Diagnostics
- Semantic tokens

[Unreleased]: https://github.com/nlopes/acdc/compare/acdc-lsp-v0.1.0...HEAD
[0.1.0]: https://github.com/nlopes/acdc/releases/tag/acdc-lsp-v0.1.0
