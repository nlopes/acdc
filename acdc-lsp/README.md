# acdc-lsp

A Language Server Protocol (LSP) implementation for AsciiDoc documents, powered by `acdc-parser`.

## Features

- **Diagnostics** - Parse errors appear as warnings/errors in your editor
- **Document Symbols** - Section outline for navigation (breadcrumbs, outline panel)
- **Go-to-Definition** - Jump from `xref:target[]` to the corresponding `[[target]]` anchor or section

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

For debugging, you can enable trace logging:

```bash
RUST_LOG=acdc_lsp=debug acdc-lsp
```

## Editor setup

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

The target must exist in the same document. Cross-file navigation isn't supported yet.

Valid targets:
- Section IDs: `[[my-section]]` or auto-generated from section titles
- Inline anchors: `[[anchor-id]]`

## Limitations

- Single-file only (no cross-file references yet)
- Full document sync (reparsing on every change)
- No semantic tokens/highlighting
- No hover documentation
- No completion
