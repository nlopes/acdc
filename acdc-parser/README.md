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

## Include depth

Built-in includes have a trusted `max-include-depth` attribute that defaults to `64`
and is visible as `{max-include-depth}` in the parsed document. The fallback
participates in attribute lookup, substitution, and conditionals without being
reported as caller-stored by `DocumentAttributes::iter()` or `contains_key()`, and it
is not serialized unless the caller supplied a value. Set it through the parser
options when a different limit is needed:

```rust
let options = acdc_parser::Options::builder()
    .with_attribute("max-include-depth", "8")
    .build();
```

The entry document does not count toward the limit; each currently open included file
counts as one level. A value of `0` or a negative value disables built-in include
processing and leaves each directive as literal content without a diagnostic. String
values use their leading signed decimal while the original value remains visible as
the document attribute; for example, `" 8"` and `"8notes"` both set a limit of 8. At
a positive limit, the blocked directive is likewise preserved, a located diagnostic
is added to `ParseResult::warnings()`, and parsing continues. Leading Unicode
whitespace is treated as whitespace, so a non-breaking space (`U+00A0`) before `8`
also sets a limit of 8. Only an exact `include::` directive is processed; block macro
names that merely begin with `include` remain ordinary content without include
diagnostics. Declarations in document content are consumed but cannot change or unset
the trusted value and do not appear as `Block::DocumentAttribute` nodes in the AST.

## Include indentation

The `indent` attribute on an include accepts a non-negative integer from `0` through
`4096`. A larger value returns a located error before the target is read, preventing
one directive from requesting an unbounded space prefix for every non-empty included
line.
Malformed and negative values remain invalid.

The `4096` cap is an intentional acdc security policy, not a requirement of the
AsciiDoc language. It deliberately creates an acceptance divergence from
asciidoctor, which coerces the value using Ruby's `to_i` and does not impose a
documented or implemented maximum before allocating the indentation prefix.

## Table limits

Tables accept at most 100 logical columns and 1,000 rows, bounding an accepted table
to 100,000 materialized cells. The column bound also applies to `cols` multipliers,
cell duplication counts, and column spans; row spans are bounded by the row limit.
Larger requests and oversized CSV, TSV, DSV, or PSV dimensions return a located
parse error before unbounded expansion.

These are fixed internal safety limits. They cannot be changed through parser options
or document attributes. This intentionally diverges from asciidoctor, which has no
equivalent table dimension cap.

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

We use `ureq` for HTTP framing, redirects, TLS, and timeouts. We don't try to
reproduce the URI transport behavior that Asciidoctor's Ruby implementation inherits
from OpenURI. If opening a request or reading its body fails, we emit a located
warning, preserve the include as an unresolved directive, and continue parsing. We
don't process partial response bodies. Character-encoding errors remain fatal.

This limit is an intentional security divergence from asciidoctor, which has no
equivalent per-response limit.

## Deliberate divergences from asciidoctor

acdc's references are the [AsciiDoc Language draft specification](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/) and [asciidoctor](https://asciidoctor.org). A handful of parser behaviours intentionally differ from asciidoctor where the draft spec and asciidoctor diverge, or where asciidoctor's output is an implementation artifact.

* **Remote include response limit**: Each decoded HTTP(S) include response is limited
  to 10 MiB. See [Remote includes](#remote-includes) for the authority requirements
  and limit behavior.
* **Remote include transport**: We use `ureq` and don't try to reproduce the URI
  transport behavior that Asciidoctor's Ruby implementation inherits from OpenURI.
  Request and response-body I/O failures recover without processing partial content.
  See [Remote includes](#remote-includes).
* **Include indentation limit**: As an intentional acdc security policy,
  `include::file[indent=N]` accepts at most `4096` spaces instead of allowing an
  unbounded allocation. AsciiDoc does not require this cap, and asciidoctor does not
  impose it. See [Include indentation](#include-indentation).
* **Table dimension limits**: Tables accept at most 100 logical columns and 1,000
  rows, with fixed internal limits that cannot be raised by parser options or
  document attributes. See [Table limits](#table-limits).
* **Boolean include-depth value**: Passing boolean `true` as `max-include-depth`
  safely disables built-in includes instead of reproducing asciidoctor's Ruby
  `NoMethodError`. Use a decimal string for a numeric limit; see
  [Include depth](#include-depth).
* **Unicode whitespace in include-depth values**: acdc treats leading Unicode
  whitespace, including a non-breaking space (`U+00A0`), as whitespace when deriving
  the numeric limit. asciidoctor's Ruby conversion skips only ASCII whitespace, so
  the same value is converted to `0` and disables built-in includes. See
  [Include depth](#include-depth).
* **Symmetric escape of constrained markers**: `\*foo\*`, `\_foo\_`, `` \`foo\` ``, `\#foo\#` all emit the literal marker pair (`*foo*`, `_foo_`, etc.). asciidoctor strips only the opening backslash and leaves the trailing `\` in the output. The draft spec's backslash-escaping section (`spec/outline.adoc`) states: "a backslash in front of a reserved markup character will be removed, regardless of whether the text would have been interpreted or not" — acdc follows that rule symmetrically.

## See also

- [CHANGELOG](CHANGELOG.md) for detailed feature history and version notes
