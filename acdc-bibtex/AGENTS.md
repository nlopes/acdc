# acdc-bibtex — Developer Guide

A port of the `asciidoctor-bibtex` gem. The upstream Ruby source sits in
`asciidoctor-bibtex/` at the repository root and is the reference for
behaviour.

## Architecture

`asciidoctor-bibtex` is a block macro plus a treeprocessor. acdc's parser has
no extension registry, so this crate is a **post-parse pass over the finished
AST**, run between parsing and conversion. Every backend therefore renders
citations without knowing they were generated, and no converter needed
changing.

The pass needs `ParseResult::with_document_mut`, which hands it the document
and the `DocumentArena` the AST's `&'a str`s are allocated from — a pass that
generates new text has no other way to produce strings with the AST's
lifetime.

| Module | Responsibility |
|---|---|
| `processor.rs` | The three passes: gather keys, order them, replace macros and the `bibliography::[]` placeholder. Builds the AST nodes. |
| `macros.rs` | Scanner for `cite:`, `citenp:` and `bibitem:` in a line of text. |
| `walk.rs` | Reaching every list of inline nodes in a document, including the footnote catalog. |
| `database.rs` | A hand-written BibTeX reader. |
| `latex.rs` | Decoding LaTeX markup in field values. |
| `names.rs` | Reading and arranging a BibTeX name list. |
| `style.rs` | `ieee`, `apa` and `chicago-author-date`, as spans rather than markup. |
| `settings.rs` | The `bibtex-*` document attributes. |

## Things that bite

- **Unknown block macros arrive as paragraphs.** `bibliography::[]` is not a
  macro the parser knows, so it reaches the pass as a `Block::Paragraph`
  holding one `InlineNode::PlainText`. Inline macros likewise arrive as plain
  text in the middle of a paragraph.
- **Footnotes exist twice.** The document carries each footnote where it was
  written *and* in `Document::footnotes`, which is what the backends render
  the definitions from. Both copies have to be rewritten; `walk::document`
  does this, `walk::inline_containers` alone does not.
- **`true` and `false` are not strings.** acdc normalises them to
  `AttributeValue::Bool`, so `get_string` returns `None` for
  `:bibtex-throw: true`. Yes-or-no attributes go through `settings::flag`.
- **Nodes are built, not re-parsed.** The pass constructs `Highlight`,
  `Italic`, `Link`, `CrossReference` and `InlineAnchor` nodes directly rather
  than generating AsciiDoc and parsing it again. That sidesteps the arena
  mismatch a second parse would cause, and makes the gem's `&#44;` and `+[+`
  source-escaping tricks unnecessary.
- **Generated text is not re-parsed either**, so a bare URL in it would stay
  plain. `processor::push_text` splits web addresses out into link nodes so
  that a DOI in a bibliography behaves like a URL written in the document.

## Comparing against the gem

The gem is the reference. Install it (`gem install asciidoctor-bibtex`) and
compare one source at a time:

```console
asciidoctor -r asciidoctor-bibtex -a bibtex-style=apa -o ref.html sample.adoc
acdc convert --backend html sample.adoc
```

Compare the visible text of the two documents, not the bytes. The upstream
samples in `asciidoctor-bibtex/samples/` all match; two known divergences are
deliberate:

- citeproc drops the `booktitle` from an `@conference` entry, and from an
  `@inproceedings` entry that also has a publisher. This crate keeps it.
- Citations are not merged into ranges (`[1]-[3]`). The gem merges them only
  when it is not linking entries, which never happens when it runs as an
  extension.

Two differences that look like this crate's fault but are not: acdc does not
strip the trailing `==` from a closed ATX heading, and it does not start a new
description-list term on the line after a nested list without a blank line.
Both reproduce on a document with no citations in it.

## Testing

```console
cargo nextest run -p acdc-bibtex --all-features
```

Unit tests live beside each module; `tests/documents.rs` runs whole documents
through the pass. The expectations in `style.rs` and `tests/documents.rs` were
captured from the gem, so a change that drifts from it fails a test rather
than passing quietly.
