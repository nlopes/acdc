# acdc-parser

The implementation here follows from:

* [Language Lexicon](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/modules/ROOT/pages/lexicon.adoc): nomenclature of elements
* [Language Outline](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/outline.adoc): behaviour/layout
* [Asciidoctor Language Documentation](https://docs.asciidoctor.org/asciidoc/latest): behaviour/layout

## Features supported

* [x] Document Headers
* [x] Section
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
* [x] Lists (see [detailed notes](#lists---detailed-support-notes))
    * [x] Ordered
    * [x] Unordered
    * [x] Description Lists (partial support)
    * [x] Checklist items
* [x] Thematic Break
* [x] Page Break
* [x] Tables (basic support)
* [x] Admonition
* [x] Anchors
* [x] Attributes
* [x] Titles
* [x] Footnotes (including inline content)
* [x] Cross References
    * [x] xref macro
    * [x] Shorthand notation (`<<id>>`)
* [x] Links and URLs
    * [x] Link macro
    * [x] URL detection and autolinks
* [x] Inline Macros
    * [x] Button
    * [x] Keyboard
    * [x] Menu
    * [x] Icon
    * [x] Pass
* [x] Includes
    * **Advanced**
    * [x] Offsets
    * [ ] Tagged regions
* [x] Conditionals
    * [x] ifdef
    * [x] ifndef
    * [x] ifeval
* [x] Line breaks (+)
* [x] Discrete headers

## Lists - detailed support notes

Lists are partially implemented. Here's what works and what doesn't.

### What works

**Ordered and unordered lists**

Basic list items with inline content work fine, including:

- Nested lists (using different marker levels like `*`, `**`, `***`)
- Checklist items (`[x]`, `[ ]`)
- Multiline text that wraps within a list item

**Description lists**

I've implemented basic description list support with several features:

- All standard delimiters (`::`, `:::`, `::::`, `;;`)
- Principal text (inline text immediately after the delimiter)
- Explicit continuation (`+`) for attaching block content
- Auto-attaching ordered/unordered lists to description list items (even with blank lines before the list)

### What doesn't work

**Multiple list continuations**

Single `+` continuation now works for attaching blocks to list items:

```asciidoc
* List item text
+
----
Block content here
----
* Next item (stays in same list)
```

However, multiple consecutive `+` continuations in the same item don't work correctly yet:

```asciidoc
* Item
+
Paragraph 1
+
Paragraph 2   <-- This doesn't attach properly
```

**List separators**

The [spec describes](https://docs.asciidoctor.org/asciidoc/latest/lists/separating/) two ways to force separate lists:

1. Line comment separator (`//`)
2. Block attribute separator (`[]`)

I don't support either. Lists with the same marker will always join together.

**Description list limitations**

While basic description lists work, there are gaps:

- No support for the experimental `[ordered]` and `[unordered]` attributes ([described here](https://docs.asciidoctor.org/asciidoc/latest/lists/description-with-marker/))
- No `.stack` role support for formatting

**Other missing list features**

- No support for `{empty}` to drop principal text
- No open block (`--`) wrapper support for grouping multiple blocks
- No ancestor list continuation (attaching blocks to parent list items with blank lines before `+`)

### Why these limitations exist

The model now supports block attachments (`ListItem` has both `principal: Vec<InlineNode>` and `blocks: Vec<Block>`), and single `+` continuations work. The remaining issues are parser grammar limitations:

1. Multiple consecutive `+` markers in the same item aren't parsed correctly
2. List separators (`//` and `[]`) aren't recognized
3. Ancestor list continuation (blank line before `+`) isn't supported
