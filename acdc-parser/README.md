# `acdc-parser`

The implementation here follows from:

* https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/modules/ROOT/pages/lexicon.adoc[Lexicon]
* https://docs.asciidoctor.org/asciidoc/latest[AsciiDoc Language Documentation]

I'll try to keep as close as possible to:

* the nomenclature of elements in the grammar by following the https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/modules/ROOT/pages/lexicon.adoc[Lexicon]
* the behaviour/layout as described in the https://docs.asciidoctor.org/asciidoc/latest[AsciiDoc Language Documentation]

WARNING: I don't yet follow the format of the ASG as described https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/tree/main/asg?ref_type=heads[here].

NOTE: I took a few of the grammar rules from https://github.com/kober-systems/literate_programming_toolsuite/blob/master/asciidoctrine/src/reader/asciidoc.pest[here] - some were already a perfect match to what I had, others I adapted, others I had in different forms and simplified. Credit where credit is due!

## Features supported

* [*] Document Headers
* [*] Section
* [*] Delimited Block
** [*] Comment
** [*] Example
** [*] Listing
** [*] Literal
** [*] Open
** [*] Sidebar
** [*] Table
** [*] Pass
** [*] Quote
* [*] Paragraph _(constrained not supported well!)_
** [*] Bold
** [*] Italic
** [*] Monospace
** [*] Literal Monospace
** [*] Highlight
** [*] Subscript / Superscript
* [*] Image
* [*] Lists
** [*] Ordered
** [*] Unordered
* [*] Thematic Break
* [*] Page Break
* [ ] Tables
* [*] Admonition
* [*] Anchors
* [*] Attributes
* [*] Titles
* [*] Footnotes
* [*] Includes
** *Advanced*
** [*] Offsets
** [ ] Tagged regions
* [ ] Conditionals
** [*] ifdef
** [*] ifndef
** [ ] ifeval
