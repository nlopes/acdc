# acdc-lsp

A Language Server Protocol (LSP) implementation for AsciiDoc documents, powered by `acdc-parser`.

> **Note**: This tool was heavily built using [Claude Code](https://claude.com/claude-code). It's been reviewed, but I didn't approach the code with maintainability in mind, so beware. Please report any issues you encounter.

## Features

### Core

- **Diagnostics** - Parse errors and validation warnings (unresolved xrefs, duplicate anchors)
- **Document Symbols** - Section outline for navigation (breadcrumbs, outline panel)
- **Go-to-Definition** - Jump from `xref:target[]` to the corresponding `[[target]]` anchor or section
- **Find References** - Find all xrefs pointing to an anchor
- **Rename** - Refactor anchor IDs and automatically update all xrefs

### Navigation & Editing

- **Hover** - Information about xrefs, anchors, and links at cursor position
- **Completion** - Suggestions for xref targets, attribute references, and includes
- **Document Links** - Clickable URLs and file references
- **Folding Ranges** - Collapse sections, delimited blocks, and lists

### Syntax

- **Semantic Tokens** - Rich syntax highlighting for sections, macros, attributes, formatting, comments, and anchors

## Installation

### From source

```bash
cargo install --path acdc-lsp
```

Or build manually:

```bash
cargo build --release -p acdc-lsp
# Binary at: target/release/acdc-lsp
```

## Running the server

The server communicates over stdio (standard input/output) using JSON-RPC, which is the standard LSP transport. You don't run it directly - your editor starts it automatically.

### Analysis backend

The language server analyzes documents using the `html5` backend by default, matching
Asciidoctor. Clients that support `workspace/configuration` can set the `backend` key in
the `acdc-lsp` settings section:

```json
{
  "acdc-lsp": {
    "backend": "pdf"
  }
}
```

These settings are resource-scoped. In a multi-root workspace, each folder can therefore
select a different backend; the setting for the most specific containing workspace folder
wins. Changes made while the server is running take effect immediately and refresh parsed
documents, diagnostics, semantic tokens, code lenses, inlay hints, and workspace symbols.

For clients without workspace configuration support, or as a fallback until workspace
settings are loaded, use the LSP initialization options:

```json
{
  "initializationOptions": {
    "backend": "pdf"
  }
}
```

Supported values are `html5`, `html5s`, `docbook5`, `manpage`, `markdown`, `pdf`, and
`terminal`. The Asciidoctor aliases `html` and `docbook` are also accepted. The selected
backend controls intrinsic attributes such as `backend`, `backend-pdf`, `basebackend`,
`filetype`, and `outfilesuffix`, so conditional blocks and attribute references are
analyzed in the same context as conversion. A workspace setting overrides the initialization
fallback; setting the workspace `backend` to `null` restores that fallback.

For debugging, you can enable trace logging:

```bash
RUST_LOG=acdc_lsp=debug acdc-lsp
```

## Editor setup

### Emacs (eglot)

Add to your Emacs config:

```elisp
(require 'eglot)

;; Register acdc-lsp for adoc-mode
(add-to-list 'eglot-server-programs
             '(adoc-mode . ("acdc-lsp")))

;; Auto-start eglot for AsciiDoc files
(add-hook 'adoc-mode-hook 'eglot-ensure)

;; Optional: enable semantic highlighting
(setq eglot-enable-semantic-highlighting t)
```

To select a backend for one project, add a `.dir-locals.el` file at the project root:

```elisp
((adoc-mode
  . ((eglot-workspace-configuration
      . (:acdc-lsp (:backend "pdf"))))))
```

Eglot supplies this setting through `workspace/configuration`. After changing the setting
during an active session, reload the directory-local variables and run
`M-x eglot-signal-didChangeConfiguration`. Use
`M-x eglot-show-workspace-configuration` to inspect the JSON Eglot will send.

### Zed

Add to your Zed settings (`~/.config/zed/settings.json`):

```json
{
  "languages": {
    "AsciiDoc": {
      "language_servers": ["acdc-lsp"]
    }
  },
  "lsp": {
    "acdc-lsp": {
      "binary": {
        "path": "acdc-lsp"
      }
    }
  }
}
```

If `acdc-lsp` isn't in your PATH, use the full path:

```json
{
  "lsp": {
    "acdc-lsp": {
      "binary": {
        "path": "/path/to/acdc-lsp"
      }
    }
  }
}
```

Zed recognizes `.adoc` and `.asciidoc` files as AsciiDoc by default.

### Neovim

Using the built-in LSP client (Neovim 0.8+):

```lua
-- In your init.lua
vim.api.nvim_create_autocmd('FileType', {
  pattern = { 'asciidoc', 'asciidoctor' },
  callback = function()
    vim.lsp.start({
      name = 'acdc-lsp',
      cmd = { 'acdc-lsp' },
    })
  end,
})

-- Ensure .adoc files are recognized
vim.filetype.add({
  extension = {
    adoc = 'asciidoc',
    asciidoc = 'asciidoc',
  },
})
```

### Helix

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "asciidoc"
scope = "source.asciidoc"
file-types = ["adoc", "asciidoc", "asc"]
language-servers = ["acdc-lsp"]

[language-server.acdc-lsp]
command = "acdc-lsp"
```

### VS Code

Currently no extension available. Contributions welcome!

## Native release packaging

The [Release LSP workflow](../.github/workflows/release-lsp.yml) tests the server
for macOS ARM64/x86-64, Linux ARM64/x86-64, and Windows x86-64. It uses
[upload-rust-binary-action](https://github.com/taiki-e/upload-rust-binary-action)
to build the release binaries, create archives, and calculate SHA-256 checksums.
Linux builds use Ubuntu 22.04 and require glibc 2.35 or later.

Archives are named `acdc-lsp-X.Y.Z-TARGET.tar.gz` on macOS and Linux, or
`acdc-lsp-X.Y.Z-TARGET.zip` on Windows. They contain `acdc-lsp` (or
`acdc-lsp.exe`) and the license at their root. Each target also has an
`acdc-lsp-X.Y.Z-TARGET.sha256` file.

A manual workflow run uploads build artifacts without creating a release. A
pushed `acdc-lsp-vX.Y.Z` tag must match the crate version and creates a **draft**
GitHub release after all platform tests pass. Review the archives and changelog
before publishing the draft.

Before tagging a new release, update the crate version and lockfile, and move
the relevant changelog entries into its release section. Use a new tag for
each release; do not move an existing tag to newer code.

## File extensions

The server works with any file your editor sends it. Configure your editor to recognize these extensions as AsciiDoc:

| Extension | Description |
|-----------|-------------|
| `.adoc` | Most common AsciiDoc extension |
| `.asciidoc` | Full name extension |
| `.asc` | Short form (less common) |
| `.ad` | Rarely used |

## Troubleshooting

### Server not starting

1. Verify the binary is in your PATH: `which acdc-lsp`
2. Test it runs: `echo '= Test' | acdc-lsp` (should hang waiting for LSP messages)
3. Check editor logs for error messages

### No diagnostics appearing

1. Ensure the file type is recognized as AsciiDoc in your editor
2. Check that the language server is attached (most editors show this in the status bar)
3. Enable debug logging: `RUST_LOG=acdc_lsp=debug`

### Go-to-definition not working

The target must exist either in the current document or in another AsciiDoc file reachable from the workspace root. Valid targets:

- Section IDs: `[[my-section]]` or auto-generated from section titles
- Inline anchors: `[[anchor-id]]`
- Bibliography anchors: `[[[entry,label]]]`

Cross-file resolution uses the workspace index in `src/state/workspace.rs`, which scans open documents and on-disk `.adoc` files under the workspace root.

## Limitations

- Full document sync (reparsing on every change) — incremental parsing is not yet implemented
- The code-action catalog is small; suggestions are limited to the quick fixes currently wired up in `capabilities/code_actions.rs`
- Cross-file navigation relies on heuristic filesystem scanning; very large workspaces may see startup cost on first open
