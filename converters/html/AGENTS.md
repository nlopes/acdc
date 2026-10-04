# HTML Converter — Developer Guide

Architecture docs for the `acdc-converters-html` crate, focused on the stylesheet and CSS pipeline.

For `[subs="…"]` plumbing (the `pre-spec-subs` feature, `effective_subs` call
sites, fixture-naming conventions), see `converters/AGENTS.md`.

## Feature-specific fixtures

Keep canonical `.html` expectations for builds with highlighting enabled.
Fixtures with a `source-highlighter` header attribute also need
`.no-highlighting.html` expectations in each variant directory. The harness
selects them when `highlighting` is disabled; it uses the parsed attributes,
so an explicit unset selects the canonical expectation. Both modes exercise
the same source inputs. Fixtures whose stems contain `subs` remain skipped
when `pre-spec-subs` is disabled.

The generator follows the same selection rules. With highlighting disabled,
it writes only the plain alternatives, preserving canonical expectations.
With highlighting enabled, it writes canonical expectations. Create source
fixtures directly and get approval before updating existing expectations.
From the workspace root, scope regeneration by source stem:

```console
cargo run -p acdc-converters-html --example generate_html_fixtures --all-features -- <fixture_name>
cargo run -p acdc-converters-html --example generate_html_fixtures --no-default-features --features pre-spec-subs -- <fixture_name>
```

CI checks all four combinations of `highlighting` and `pre-spec-subs`.
The all-features converter step covers both enabled; HTML-specific steps
cover substitutions alone, highlighting alone, and neither feature.

## Stylesheet rendering pipeline

The entry point is `render_head()` in `html_visitor.rs`, which calls `render_stylesheet()` as part of the `<head>` output. In embedded mode, `visit_document_start()` returns early and `render_head()` is never called, so none of the stylesheet pipeline runs. Similarly, `after_write()` returns early when embedded, skipping `copycss` and syntax CSS file writes.

The flow:

```
render_head()
  ├── metadata (charset, viewport, generator, color-scheme)
  ├── header metadata (title, authors, description)
  ├── render_stylesheet(dark_mode)
  │     ├── :!stylesheet: → early return (no CSS, no fonts)
  │     ├── webfonts: disabled → skip / custom → custom link / default → Google Fonts link
  │     ├── :linkcss: set → link_css() → <link> to external file
  │     │     └── stem supplement <style> block
  │     └── :linkcss: not set → embed CSS
  │           ├── resolve_custom_css() → read custom file from disk
  │           └── fallback: load_css() → built-in CSS via include_str!
  ├── MathJax (if :stem: set)
  ├── Font Awesome (if :icons: font)
  └── maybe_emit_syntax_css() → syntax highlighting CSS (class mode only)

after_write() (post-conversion, file on disk)
  ├── handle_copycss() → write/copy main stylesheet to output dir
  └── handle_copy_syntax_css() → write highlighting CSS to output dir
```

## Key functions

### `html_visitor.rs`

| Function | Purpose |
|----------|---------|
| `render_head()` | Orchestrates the full `<head>` output including CSS, fonts, MathJax, Font Awesome |
| `render_stylesheet(dark_mode)` | Webfonts link + CSS embed/link + max-width constraint. Skipped when `:!stylesheet:` |
| `link_css(writer, attributes, default_filename)` | Writes a `<link rel="stylesheet">` tag using `stylesdir` and `stylesheet` attributes |
| `resolve_custom_css(attributes, source_dir)` | Reads a custom CSS file from disk (`:stylesheet:` + `:stylesdir:`). Returns `None` to fall back to built-in |
| `maybe_emit_syntax_css()` | Emits syntax highlighting CSS in `<head>` for class-based mode. Embeds or links based on `:linkcss:` |

### `lib.rs`

| Function | Purpose |
|----------|---------|
| `load_css(dark_mode, variant)` | Returns built-in CSS content via `include_str!`. Picks from 4 static files based on variant × dark-mode |
| `default_stylesheet_name(is_dark)` | Returns the filename for the current variant/dark-mode combination (e.g. `asciidoctor-light-mode.css`) |
| `handle_copycss(doc, html_path)` | Post-conversion: writes built-in CSS to disk (or copies custom CSS) when `:linkcss:` + `:copycss:` |
| `handle_copy_syntax_css(doc, html_path)` | Post-conversion: writes `acdc-highlight.css` when `:linkcss:` + class-based highlighting |
| `resolve_highlight_settings(document_attributes)` | Resolves theme name and inline/class mode from `:highlight-style:` and `:highlight-css:`, with deprecated `syntect-*` aliases |
| `document_attributes_defaults()` | Sets default values for `copycss`, `stylesdir`, `stylesheet`, `webfonts` |

## Attribute flow

Document attributes use an "insert is no-op if key exists" pattern — the parser sets user-provided values first, then `document_attributes_defaults()` fills in missing keys. This means user attributes always win.

The `convert_to_writer()` method rebuilds the processor with the document's own attributes (which may differ from construction-time attributes due to in-document attribute definitions). This ensures the stylesheet pipeline sees the final merged attributes.

Key attributes and their defaults:

| Attribute | Default | Effect |
|-----------|---------|--------|
| `stylesheet` | `""` (empty) | Empty = use built-in CSS. Non-empty = custom file. `false` = disabled |
| `linkcss` | not set | When present, link to external CSS instead of embedding |
| `stylesdir` | `.` | Directory for stylesheet references and file copies |
| `copycss` | `""` (empty) | When key exists + `:linkcss:`, copy CSS to output dir. Non-empty value = source path override |
| `webfonts` | `""` (empty) | Empty = default Google Fonts. Non-empty = custom families. `false` = disabled |
| `dark-mode` | not set | When present, use dark variant of built-in CSS + color-scheme meta |
| `highlight-css` | not set | `class` = CSS class mode. Anything else = inline styles (default) |
| `highlight-style` | auto | Syntect theme name. Falls back to light/dark default based on `:dark-mode:` |
| `syntect-css` | not set | [deprecated] Compatibility alias for `highlight-css` |
| `syntect-style` | not set | [deprecated] Compatibility alias for `highlight-style` |

An explicit primary assignment, including an empty value or unset, takes
precedence over its alias. Body highlighting, head styles and CSS file writes
use the same resolver. Both primary and deprecated names generate and link
`acdc-highlight.css`; keep the `syntax-*` token classes compatible.
Deprecated assignments emit structured warnings with replacement advice,
once per alias per conversion, including unsets and overridden values.
The visitor checks header, body and nested cell assignments, even when the
highlighting feature is off; the `source-highlighter: syntect` value remains valid.

## Static CSS assets

Four built-in stylesheets in `static/`:

| File | Variant | Mode |
|------|---------|------|
| `asciidoctor-light-mode.css` | Standard | Light |
| `asciidoctor-dark-mode.css` | Standard | Dark |
| `html5s-light-mode.css` | Semantic | Light |
| `html5s-dark-mode.css` | Semantic | Dark |

All four are compiled into the binary via `include_str!` in `load_css()`. The filename constants are in `lib.rs` (`STYLESHEET_LIGHT_MODE`, `STYLESHEET_DARK_MODE`, etc.).

The syntax highlighting stylesheet (`acdc-highlight.css`) is generated at runtime from syntect theme data — it has no static file.

## Decision tree for stylesheet output

```
render_stylesheet():
  :!stylesheet: set?
    YES → return (no CSS, no fonts)
    NO  ↓
  webfonts:
    :!webfonts:     → skip font link
    :webfonts: X    → <link> to fonts.googleapis.com/css?family=X
    (default)       → <link> to default Google Fonts
  :linkcss: set?
    YES → link_css() → <link rel="stylesheet" href="{stylesdir}/{filename}">
    NO  → resolve_custom_css()
          found?  → embed custom CSS in <style>
          not found → embed load_css() (built-in) in <style>
  :max-width: set?
    YES → extra <style>#content { max-width: ... }</style>

after_write():
  embedded mode?
    YES → return (no file writes)
    NO  ↓
  :linkcss: set?
    NO  → return
    YES ↓
  :copycss: key exists?
    NO  → return
    YES ↓
  using built-in stylesheet?
    YES → write load_css() content to {output_dir}/{filename}
    NO  → copy source file to {output_dir}/{filename}
  class-based highlighting active?
    YES → write generated CSS to {stylesdir}/acdc-highlight.css
```

## Asciidoctor references

- [Stylesheet modes](https://docs.asciidoctor.org/asciidoctor/latest/html-backend/stylesheet-modes/)
- [Default stylesheet](https://docs.asciidoctor.org/asciidoctor/latest/html-backend/default-stylesheet/)
- [Custom stylesheet](https://docs.asciidoctor.org/asciidoctor/latest/html-backend/stylesheet-modes/#custom)
- [copycss and stylesdir](https://docs.asciidoctor.org/asciidoctor/latest/html-backend/manage-images/)
