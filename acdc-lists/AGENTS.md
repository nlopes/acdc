# acdc-lists — Developer Guide

A Rust port of [asciidoctor-lists]. The gem is two extensions — a block macro
that leaves a placeholder and a treeprocessor that later swaps it for the
references it found. acdc's parser has no extension registry, so both halves
run here as a **pass over the finished AST**, between parsing and conversion.
The reference implementation is a single short file, `lib/asciidoctor-lists/extensions.rb`,
worth reading before changing behaviour.

[asciidoctor-lists]: https://github.com/Alwinator/asciidoctor-lists

## Layout

| Module | Responsibility |
|---|---|
| `processor.rs` | The pass: gather requests, collect, replace |
| `macro_call.rs` | Recognising `list-of::target[attrs]` inside a paragraph |
| `element.rs` | Element names and what they match in the AST |
| `entry.rs` | Collecting entries and assigning ids |
| `walk.rs` | The shared tree walk and the block accessors acdc-parser keeps private |

## Why three phases

A list names elements that may appear *after* the call — `list-of::image[]` at
the top of a document lists figures further down — so the pass cannot rewrite
as it walks. It gathers the requested element kinds, then every matching
element, then replaces the calls. Anything that changes one phase usually has
to be reflected in the others.

`hide_empty_section` is the one piece of state that travels between phases:
`replace` returns whether a call asked for its section to go, and only the
owner of that section acts on it. That is why the replacement walk is separate
from `walk.rs` — the shared walk cannot remove blocks.

## The AST this depends on

- **Captions come from the parser**, on `BlockMetadata::caption`, already
  numbered. The pass never computes a caption itself; it emits a cross-reference
  with `XrefStyle::Short` and lets the backend render the prefix. A change to
  caption numbering shows up here for free.
- **A cross-reference with no text resolves through `Document::references`.**
  An element the pass gives an id to therefore needs a matching
  `Reference::for_target` entry, or its list entry renders as `[image-1]`.
- **`acdc-parser` keeps `Block::metadata()`, `title()` and `anchor()` private**,
  so `walk.rs` has its own. If those ever become public, delete the local
  copies rather than keeping both.

## Testing

`tests/lists.rs` asserts on the rewritten AST rather than on any backend's
output, so the tests do not move when HTML changes. `summary()` flattens a
generated list into one line per entry.

For end-to-end checks the upstream samples are the fastest exercise, and the
gem is the reference:

```sh
acdc convert -o - samples/list-sample.adoc
asciidoctor -r asciidoctor-lists -o - samples/list-sample.adoc
```

Expect two differences by design: ids are readable (`image-1`) rather than
UUIDs, and a caption in an entry has no trailing period.
