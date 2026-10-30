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
- **Document Preview** - Render the current unsaved document as HTML or PDF and open it in the system's viewer

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

### Includes

Open documents with a local `file:` URI expand local includes. The server uses
unsaved editor text when the target is open. Otherwise, it reads the target from
disk. Nested includes and parser selectors such as `lines=`, `tag=`, and `tags=`
use the same rules as the parser.

Analysis uses Server safe mode. Each entry document's folder is the base directory
and the include boundary. The LSP also rejects symlinks that point outside that
folder. Remote includes are disabled, including when a document sets
`:allow-uri-read:`. Untitled documents and other URI schemes do not load includes.
The background symbol scan also leaves includes disabled.

The parser limits selected include text to 10 MiB per include. Editor analysis
uses its default 64 MiB budget per parse, after indentation. Repeated and nested
includes count toward that budget. A line selection from a larger file works if
the selected text fits both limits. See the parser's
[include limits](../acdc-parser/README.md#include-source-size-limit) for details.

Opening, changing, or closing an included buffer refreshes its open parent
documents. Closing a buffer restores its disk contents, if the file exists.
Save and file-rename notifications also refresh parents. For changes made outside
the editor, the server requests file-change notifications when the client supports
them. These notifications cover file creation and deletion too. Without client
file watching, an external change takes effect when the parent is parsed again.

References and go-to-definition use UTF-16 character positions. Included targets
use the original file's text, including unsaved editor buffers. A location without
available source text is omitted instead of guessing columns.

## Editor setup

### Document preview

Use the **Preview AsciiDoc as PDF** or **Preview AsciiDoc as HTML** code action
to render the current editor buffer, including unsaved changes. The main action
uses the document's configured backend: `pdf`, `html5`, or `html5s` (semantic HTML).
PDF previews include PDF-specific conditionals and page layout. An explicit HTML5
action remains available when another backend is configured. Other analysis
backends offer only that HTML5 action.

The server sends
[`window/showDocument`](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#window_showDocument)
with `external: true`; the editor asks the system's URL handler to open the file.
HTML usually opens in a browser. PDF opens in the user's default PDF application,
which can be a browser or a dedicated viewer. Actions are available only when
the client advertises support for that request.

Both formats use the same local-include provider and limits as editor analysis,
including unsaved included buffers. Preview does not change diagnostics or the
analysis settings. Untitled buffers can be previewed, but have no base directory
for local resources.

HTML previews use acdc's built-in styles and syntax highlighting. Relative images
and other resources resolve against the source document's location. Section and
table-of-contents links stay within the preview. Custom stylesheets and docinfo
files are not loaded; the HTML includes a Content Security Policy.
The browser can load resources referenced by the document, such as local images,
remote images, web fonts, and MathJax. This is a browser preview, not a sandbox for
untrusted HTML.

PDF previews use acdc's bundled fonts and default theme. Local image paths must
resolve within the source document's directory; remote images are disabled,
including when the document sets `allow-uri-read`. Untitled PDF previews disable
image loading. CLI-specific font directories, themes, and other CLI options are
not applied by the language server.

Run the action again to render a new snapshot. The server reuses the document's
preview URL for each format; refresh or reopen an existing viewer if needed.
HTML and PDF snapshots have separate files. Preview files live in a
temporary directory until the language server shuts down, even if the source
buffer is closed. The action does not write generated files into the project.
The viewer must be able to access the server's local files; this does not provide
preview transport for a remote language server.

The default-enabled `preview` and `preview-pdf` Cargo features link the HTML and
PDF converters directly into `acdc-lsp`. Release binaries include both. No separate
`acdc` executable or Node.js runtime is required. To build with HTML preview only:

```bash
cargo build -p acdc-lsp --no-default-features --features pre-spec-subs,preview
```

To build without preview while retaining default parser compatibility:

```bash
cargo build -p acdc-lsp --no-default-features --features pre-spec-subs
```

Clients may also call `workspace/executeCommand` with command `acdc.preview`
to use the current backend, or `acdc.previewHtml` to request HTML5 explicitly.
Both commands take one argument: an open document's URI. Closed documents and
buffers larger than 10 MiB are rejected. Conversion errors and viewer-opening
failures are returned to the client; conversion warnings are reported separately.
Requesting PDF without `preview-pdf` returns an error; it does not produce HTML
silently. The explicit HTML action remains available in that build.

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

Zed requires an extension to register a language server. Settings alone cannot
register `acdc-lsp`. AsciiDoc syntax support is supplied by the community
[AsciiDoc extension](https://github.com/dunyakirkali/zed-asciidoc).

The published AsciiDoc extension 0.5.10 does not include acdc support. Until the
integration is published, install a checkout containing the acdc adapter with
**Extensions → Install Dev Extension**. This replaces the published extension
locally. Install the server from this repository as described above, or point
Zed at a local build:

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
        "path": "/absolute/path/to/acdc/target/release/acdc-lsp"
      }
    }
  }
}
```

The server identifier is `acdc-lsp`, with a hyphen. The adapter also finds
`acdc-lsp` on the project environment's `PATH` when no binary path is set.

For a project that targets PDF, put this in `.zed/settings.json`:

```json
{
  "lsp": {
    "acdc-lsp": {
      "settings": {
        "backend": "pdf"
      }
    }
  }
}
```

The adapter supplies these settings through `workspace/configuration` and
forwards `initialization_options` as a fallback. See [Analysis backend](#analysis-backend).

Tree-sitter highlighting, snippets, and editing rules remain available. These
optional language settings enable the corresponding LSP features:

```json
{
  "languages": {
    "AsciiDoc": {
      "semantic_tokens": "combined",
      "document_symbols": "on",
      "document_folding_ranges": "on"
    }
  }
}
```

The LSP outline includes paragraphs and blocks as well as headings. Leave
`document_symbols` off for the extension's heading-only outline. In Zed 1.22,
changing the backend refreshes diagnostics and semantic tokens, but the outline
can retain old symbols until the document is edited. Zed also skips LSP symbol
requests for Git-ignored files.

If the server does not start, check the project's trust status and use
**dev: open language server logs** or **zed: open log** from the command palette.

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
