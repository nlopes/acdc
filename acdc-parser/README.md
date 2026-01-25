# acdc-parser

The implementation here follows from:

* [Language Lexicon](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/modules/ROOT/pages/lexicon.adoc): nomenclature of elements
* [Language Outline](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/outline.adoc): behaviour/layout
* [Asciidoctor Language Documentation](https://docs.asciidoctor.org/asciidoc/latest): behaviour/layout

## Features supported

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
    * [ ] Nested tables (not supported)
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

## Parser options

* **Safe mode** - `Safe`, `Secure`, `Server`, `Unsafe`
* **Strict mode** - Stricter parsing rules
* **Setext headers** - Optional feature flag for two-line underlined headers
* **Manpage doctype** - `doctype=manpage` with derived attributes

## See also

- [CHANGELOG](CHANGELOG.md) for detailed feature history and version notes
