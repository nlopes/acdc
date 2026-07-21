# acdc-parser

Fast AsciiDoc parser written in Rust. Parses AsciiDoc source into a structured AST that mirrors the draft AsciiDoc Language specification's Abstract Semantic Graph (ASG), using a PEG grammar with a preprocessor stage for includes, conditionals, and attribute substitution.

The implementation here follows from:

* [Language Lexicon](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/modules/ROOT/pages/lexicon.adoc): nomenclature of elements
* [Language Outline](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/outline.adoc): behaviour/layout
* [Asciidoctor Language Documentation](https://docs.asciidoctor.org/asciidoc/latest): behaviour/layout

<details>
<summary>Features supported</summary>

* [x] Document Headers
    * [x] Author parsing (first/middle/last name, email)
    * [x] Revision info
* [x] Section
    * [x] ATX-style (`=` markers)
    * [x] Setext-style (underlined, optional feature)
    * [x] Discrete headers
* [x] Delimited Block
    * [x] Comment
    * [x] Example
    * [x] Listing
    * [x] Literal
    * [x] Open
    * [x] Sidebar
    * [x] Table
    * [x] Pass
    * [x] Quote
    * [x] Verse
* [x] Paragraph
    * [x] Bold (constrained & unconstrained)
    * [x] Italic (constrained & unconstrained)
    * [x] Monospace (constrained & unconstrained)
    * [x] Literal Monospace
    * [x] Highlight (constrained & unconstrained)
    * [x] Subscript / Superscript
    * [x] Curved quotes and apostrophes
    * [x] Passthrough (inline and macro)
* [x] Image (block and inline)
* [x] Video
* [x] Audio
* [x] Lists
    * [x] Ordered
    * [x] Unordered
    * [x] Description Lists
    * [x] Checklist items
    * [x] List continuation (`+`)
    * [x] Ancestor list continuation
* [x] Thematic Break
* [x] Page Break
* [x] Tables
    * [x] Header and footer rows
    * [x] Column formatting (`cols` attribute with alignment, width, style)
    * [x] Cell spanning (colspan `2+|`, rowspan `.2+|`)
    * [x] Cell duplication (`3*|`)
    * [x] Cell-level alignment (`<|`, `^|`, `>|`, `.<|`, `.^|`, `.>|`)
    * [x] Cell-level style (`s|`, `e|`, `m|`, `a|`, etc.)
    * [x] CSV, PSV, DSV formats
    * [x] AsciiDoc content in cells (`a` style)
    * [x] Nested tables (`!===` delimiter in AsciiDoc cells)
* [x] Admonition
* [x] Anchors
    * [x] Block anchors (`[[id]]`)
    * [x] Inline anchors (`[#id]`)
    * [x] Bibliography anchors (`[[[anchor]]]`, `[[[anchor,label]]]`)
* [x] Attributes
    * [x] Document attributes
    * [x] Attribute references
    * [x] `:leveloffset:` for includes
    * [x] Substitution control (`subs` with `+quotes`, `-callouts` modifiers)
* [x] Titles
* [x] Footnotes (including inline content)
* [x] Cross References
    * [x] xref macro
    * [x] Shorthand notation (`<<id>>`)
    * [x] Attribute substitution in targets and text
* [x] Links and URLs
    * [x] Link macro
    * [x] URL detection and autolinks
    * [x] Autolink syntax (`<https://...>`)
    * [x] Mailto macro
* [x] Inline Macros
    * [x] Button
    * [x] Keyboard
    * [x] Menu
    * [x] Icon
    * [x] Pass
* [x] Stem/Math
    * [x] `stem:[formula]` inline
    * [x] `latexmath:[...]` and `asciimath:[...]`
    * [x] Stem blocks
* [x] Index terms
    * [x] Visible `((term))`
    * [x] Concealed `(((term,secondary,tertiary)))`
* [x] Callouts
    * [x] Callout markers in source blocks (`<1>`, `<2>`, etc.)
    * [x] Callout lists
* [x] Table of contents (`toc::[]` macro)
* [x] Includes
    * [x] Offsets
    * [x] Tagged regions (`tag=`, `tags=`, wildcards `*`/`**`, negation `!tag`)
    * [x] `:leveloffset:` adjustment
* [x] Conditionals
    * [x] ifdef
    * [x] ifndef
    * [x] ifeval
* [x] Line breaks (+)

</details>

## Parser options

* **Safe mode** - `Safe`, `Secure`, `Server`, `Unsafe`
* **Strict mode** - Stricter parsing rules
* **Setext headers** - Optional feature flag for two-line underlined headers
* **Manpage doctype** - `doctype=manpage` with derived attributes

## Local include confinement

For file input, `Safe` and `Server` modes use the entry document's directory as the
local include boundary.

For example, assume the entry document is `/workspace/docs/main.adoc`, so the
boundary is `/workspace/docs`:

| Directive location | Include target | Path opened | Result |
| --- | --- | --- | --- |
| `/workspace/docs/main.adoc` | `chapters/intro.adoc` | `/workspace/docs/chapters/intro.adoc` | No warning |
| `/workspace/docs/main.adoc` | `../shared.adoc` | `/workspace/docs/shared.adoc` | The `..` that would leave the boundary is discarded, and a warning is emitted |
| `/workspace/docs/main.adoc` | `/workspace/docs/appendix.adoc` | `/workspace/docs/appendix.adoc` | No warning because the absolute target is already inside the boundary |
| `/workspace/docs/main.adoc` | `/tmp/shared.adoc` | `/workspace/docs/tmp/shared.adoc` | The outside absolute path is moved beneath the boundary, and a warning is emitted |
| `/workspace/docs/chapters/part.adoc` | `../../shared.adoc` | `/workspace/docs/shared.adoc` | The first `..` reaches the boundary, the second is discarded, and a warning is emitted |

Nested includes continue to use `/workspace/docs` as their boundary; they do not
switch to the nested file's directory. With `opts=optional`, the target is transformed
first, the recovery warning is retained, and a missing transformed file is then
skipped without a missing-file warning.

`Unsafe` mode does not apply these transformations: from
`/workspace/docs/main.adoc`, `../shared.adoc` attempts to read
`/workspace/shared.adoc`, and `/tmp/shared.adoc` remains `/tmp/shared.adoc`.

The boundary checks the path as written but does not resolve symlinks. If
`/workspace/docs/linked.adoc` points to `/private/secret.adoc`, including
`linked.adoc` reads `/private/secret.adoc` without a boundary warning. These
transformations match asciidoctor; they are not strict symlink containment.

## Remote includes

HTTP(S) includes require the optional `network` feature, a safe mode below
`Secure`, and a caller-supplied `allow-uri-read` attribute. A document cannot grant
itself this authority. Each response is limited to 10 MiB after transport decoding;
larger responses return an HTTP request error. The limit is fixed, applies separately
to each response, and cannot be changed by a document attribute.

This limit is an intentional security divergence from asciidoctor, which has no
equivalent per-response limit.

## Deliberate divergences from asciidoctor

acdc's references are the [AsciiDoc Language draft specification](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/) and [asciidoctor](https://asciidoctor.org). A handful of parser behaviours intentionally differ from asciidoctor where the draft spec and asciidoctor diverge, or where asciidoctor's output is an implementation artifact.

* **Remote include response limit**: Each decoded HTTP(S) include response is limited
  to 10 MiB. See [Remote includes](#remote-includes) for the authority requirements
  and limit behavior.
* **Symmetric escape of constrained markers**: `\*foo\*`, `\_foo\_`, `` \`foo\` ``, `\#foo\#` all emit the literal marker pair (`*foo*`, `_foo_`, etc.). asciidoctor strips only the opening backslash and leaves the trailing `\` in the output. The draft spec's backslash-escaping section (`spec/outline.adoc`) states: "a backslash in front of a reserved markup character will be removed, regardless of whether the text would have been interpreted or not" — acdc follows that rule symmetrically.

## See also

- [CHANGELOG](CHANGELOG.md) for detailed feature history and version notes
