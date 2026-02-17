# acdc-editor-wasm

A WebAssembly-based AsciiDoc live editor with syntax highlighting and HTML preview. Type AsciiDoc in one pane, see the rendered output in real-time.

## Installation

Download the latest release from [GitHub Releases](https://github.com/nlopes/acdc/releases). The tarball contains:

- `acdc_editor_wasm_bg.wasm` - The compiled WebAssembly binary
- `acdc_editor_wasm.js` - ES module that loads and initializes the WASM
- `acdc_editor_wasm.d.ts` - TypeScript declarations

## Quick start

### HTML structure

The editor requires these DOM elements with specific IDs:

```html
<textarea id="editor"></textarea>
<div id="editor-backdrop">
  <pre id="highlight"></pre>
</div>
<div id="preview"></div>
<div id="status"></div>
```

- `#editor` - The textarea where users type AsciiDoc source
- `#editor-backdrop` - Container for the syntax highlighting overlay
- `#highlight` - Pre element that receives highlighted HTML (overlays the textarea)
- `#preview` - Container for the rendered HTML output
- `#status` - Displays parse status ("OK" or error messages)

### JavaScript initialization

```javascript
import init from './acdc_editor_wasm.js';

await init();
// Editor is now running - typing in #editor updates #highlight and #preview
```

The `init()` function is automatically called as the WASM entry point. It:
1. Sets up panic hooks for better error messages
2. Populates `#editor` with default content
3. Attaches input, scroll, and keyboard event listeners
4. Performs the initial parse and render

### Basic CSS

The editor uses a transparent textarea overlaid on a highlighted `<pre>` element:

```css
.editor-container {
  position: relative;
}

#editor {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  background: transparent;
  color: transparent;
  caret-color: black;
  font-family: monospace;
  font-size: 14px;
  line-height: 1.5;
  padding: 1rem;
  border: none;
  resize: none;
  white-space: pre-wrap;
  word-wrap: break-word;
  z-index: 1;
}

#editor-backdrop {
  position: relative;
  overflow: auto;
}

#highlight {
  font-family: monospace;
  font-size: 14px;
  line-height: 1.5;
  padding: 1rem;
  margin: 0;
  white-space: pre-wrap;
  word-wrap: break-word;
}
```

## API reference

### `init()`

Entry point called automatically when the WASM module loads. Sets up the editor DOM orchestration including:

- Input debouncing (25ms) for parse/render updates
- Scroll synchronization between textarea and highlight overlay
- Tab key handling (inserts 2 spaces)

Returns an error if any required DOM element is missing.

### `parse_and_render(input: string): ParseResult`

Manually parse AsciiDoc source and get both highlighted source HTML and rendered preview HTML. Useful if you want to control when parsing happens rather than using the automatic editor setup.

```typescript
interface ParseResult {
  highlight_html: string;  // Source with <span class="adoc-*"> highlighting
  preview_html: string;    // Rendered HTML preview
}
```

Returns an error string if parsing fails.

## CSS classes

The syntax highlighter wraps source text in `<span>` elements with these classes:

### Block-level

| Class | Used for |
|-------|----------|
| `adoc-title` | Document title (`= My document`) |
| `adoc-heading` | Section headings (`== Section`) |
| `adoc-attribute` | Document attributes (`:key: value`) and block attributes (`[source,rust]`) |
| `adoc-block-title` | Block titles (`.Title`) |
| `adoc-comment` | Comments (`// ...` or `////...////`) |
| `adoc-delimiter` | Block delimiters (`----`, `====`, `****`, etc.) |
| `adoc-list-marker` | List bullets and numbers (`*`, `.`, `-`) |
| `adoc-description-marker` | Description list delimiters (`::`, `:::`) |
| `adoc-admonition` | Admonition labels (`TIP:`, `NOTE:`, etc.) |
| `adoc-thematic-break` | Thematic breaks (`'''`) |
| `adoc-page-break` | Page breaks (`<<<`) |
| `adoc-macro` | Block macros (`image::`, `video::`, etc.) |
| `adoc-anchor` | Anchors (`[[id]]`) |
| `adoc-table-delimiter` | Table delimiters (`\|===`) |
| `adoc-table-cell` | Table cell separators (`\|`) |
| `adoc-code-content` | Content inside listing/source blocks |
| `adoc-literal-content` | Content inside literal blocks |
| `adoc-passthrough-content` | Content inside passthrough blocks |
| `adoc-callout` | Callout markers (`<1>`, `<2>`) |
| `adoc-checklist` | Checklist markers (`[x]`, `[ ]`) |

### Inline-level

| Class | Used for |
|-------|----------|
| `adoc-bold` | Bold text (`*bold*` or `**bold**`) |
| `adoc-italic` | Italic text (`_italic_` or `__italic__`) |
| `adoc-monospace` | Monospace text (`` `code` ``) |
| `adoc-highlight` | Highlighted text (`#marked#`) |
| `adoc-superscript` | Superscript (`^super^`) |
| `adoc-subscript` | Subscript (`~sub~`) |
| `adoc-link` | Links and URLs |
| `adoc-xref` | Cross-references (`<<ref>>`) |
| `adoc-inline-macro` | Inline macros (`icon:`, `kbd:`, `btn:`, etc.) |
| `adoc-passthrough-inline` | Inline passthroughs (`pass:[]`) |
| `adoc-index-term` | Index terms |

## Optional elements

These elements are optional but enable additional features:

- `#btn-copy` - Button that copies rendered HTML to clipboard
- `#link-issue` - Link that opens a pre-filled GitHub issue with the current source

## Building from source

### Prerequisites

- [Rust](https://rustup.rs/) with the `wasm32-unknown-unknown` target
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

### Build

```bash
cd acdc-editor-wasm
wasm-pack build --target web --release
```

Output goes to `pkg/` directory.

## Example syntax highlighting theme

```css
.adoc-title { color: #1a1a2e; font-weight: bold; }
.adoc-heading { color: #16213e; font-weight: bold; }
.adoc-attribute { color: #7f5539; }
.adoc-block-title { color: #6c757d; font-style: italic; }
.adoc-comment { color: #6c757d; font-style: italic; }
.adoc-delimiter { color: #9d4edd; }
.adoc-list-marker { color: #e85d04; font-weight: bold; }
.adoc-admonition { color: #d00000; font-weight: bold; }
.adoc-bold { font-weight: bold; }
.adoc-italic { font-style: italic; }
.adoc-monospace { color: #d63384; background: #f8f9fa; }
.adoc-highlight { background: #fff3cd; }
.adoc-link { color: #0077b6; text-decoration: underline; }
.adoc-xref { color: #7209b7; }
.adoc-code-content { color: #495057; }
.adoc-callout { color: #e85d04; font-weight: bold; }
.adoc-table-delimiter { color: #9d4edd; }
.adoc-table-cell { color: #9d4edd; font-weight: bold; }
```

## Running the Example

A ready-to-use example is located in the `www/` directory.

To run it locally:

1. Build the WASM package:
   ```bash
   wasm-pack build --target web --release
   ```

2. Serve the directory (you need a web server to handle WASM MIME types correctly):
   ```bash
   # Using Python
   python3 -m http.server
   
   # Open browser at http://localhost:8000/www/
   ```

The example is also deployed to GitHub Pages: https://nlopes.github.io/acdc/

## License

MIT
