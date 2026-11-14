# acdc-parser

AsciiDoc parser written in Rust. It parses source into an abstract syntax tree
(AST), collects warnings, and serializes the native AST as JSON. Includes,
conditionals, and document attributes are processed before block and inline parsing.

The implementation here follows from:

* [Language Lexicon](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/modules/ROOT/pages/lexicon.adoc): nomenclature of elements
* [Language Outline](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/outline.adoc): behavior/layout
* [Asciidoctor Language Documentation](https://docs.asciidoctor.org/asciidoc/latest): behavior/layout

These are compatibility references, not a claim of complete conformance.

Requires Rust 1.88 or later.

## Quick start

```rust
use acdc_parser::{Options, parse};

let parsed = parse("= Example\n\nHello, *world*.\n", &Options::default())?;
let document = parsed.document();
assert!(document.blocks().next().is_some());

for warning in parsed.warnings() {
    eprintln!("{warning}");
}
# Ok::<(), acdc_parser::Error>(())
```

Use `parse_file` for a file, `parse_from_reader` for a reader, or `parse_inline`
for inline content. `ParseResult` owns the parsed document and its backing text;
keep it alive while reading `document()`. A successful parse can contain warnings
and recovered content. See [Source text and diagnostics](#source-text-and-diagnostics).

## Reading the tree

`document()` returns a typed handle. Match `BlockKind` and `InlineKind` to read
concrete elements without depending on how the parser stores them:

```rust
use acdc_parser::{Options, parse, ast::{BlockKind, InlineKind}};

let parsed = parse("Hello, *world*.", &Options::default())?;
for block in parsed.document().blocks() {
    if let BlockKind::Paragraph(paragraph) = block.kind() {
        for inline in paragraph.content().inlines() {
            if let InlineKind::Bold(bold) = inline.kind() {
                assert!(!bold.content().is_empty());
            }
        }
    }
}
# Ok::<(), acdc_parser::Error>(())
```

Keep the parse result alive while using its handles. Copying a handle does not
copy a node. A constructed node uses the same reads through `ParagraphRef::from`
or the corresponding concrete `*Ref` type. Formatting types such as `BoldRef`
and `ItalicRef` remain distinct.

| What you need | Where to read it |
| --- | --- |
| Direct document or container blocks | `blocks()` |
| Structural children, including list items and table cells | `children()`; `descendants()` for a depth-first walk |
| Paragraph, span, or label content | `content().inlines()`; hyperlink labels use `text()` |
| Every owned inline-content field | `content_occurrences()`; `content_fields()` for direct fields |
| Ordinary list principal text and attached blocks | `item.principal()` and `item.blocks()` |
| Table rows, cells, and resolved decoration | Typed table reads; authored column settings remain separate from presentation |
| Image alt or inline tooltip | `alt()` or `tooltip()`, each returning optional content |
| Link, URL, mail, or autolink destination | `target()`; `as_hyperlink()` provides their common reads |
| Registered targets and TOC entries | `document.references()` and `document.toc_entries()` |
| Element attributes | `attributes().get(name)`, `contains(name)`, or `iter()` |
| Original source coordinates | `location()` and the parse result's source-resolution methods |

`content_occurrences()` visits each retained content field once, including titles,
captions, alt, tooltips, and nested labels. Each occurrence identifies its `owner`,
`role`, and `content`. It excludes derived catalog copies. Use `skip_descendants()`
to omit nested fields without losing siblings. These walks do not render text or
register macros.

Direct children and flat walks allocate no memory. Nested walks use a growable
stack proportional to nesting. Iterators support early stopping; there is no need
to collect a document just to find one element. Handles can be used by scoped
worker threads while their owner stays alive. `ParseResult` is `Send`, but is not
`Sync`; sharing the owning result needs synchronization. These are API properties,
not a promise of a particular parse time or memory layout.

For blocks with metadata, the last authored block ID selects the primary anchor
in both compatibility modes, matching Asciidoctor. This includes stacked
`[[id]]` anchors and named `id` attributes. Earlier bracket anchors remain
available through `anchors()`; they do not register reference aliases. A later
ID replaces an earlier named ID. An ID without a label keeps the preceding
reference text; an explicit label replaces it. `id()` and `primary_anchor()` read
the selected identity without changing the document or its reference catalog.

Native JSON retains metadata declarations. Read `metadata.id` when present;
otherwise, select the final `metadata.anchors` entry. The named acdc ASG profile
writes the selected identity as `id`. For caller-built `BlockMetadata`, an
explicit `id` takes precedence over the anchor list. After editing a prepared
document, call `rebuild_references_and_toc()` to update registrations. Reads and
exports do not register anchors or invent source locations.

Thematic breaks also select the last authored ID. Their native JSON keeps IDs
in the `anchors` array, including a surviving named `id`. The selected anchor
owns its reference text; read it through `AnchorRef::reftext()`. Inherited labels
keep their original source locations. The same selection applies to caller-built
thematic breaks. Earlier declarations do not
register aliases. HTML keeps an ID on the horizontal rule so links have a target;
Asciidoctor omits that ID.

### Attributes and content

Quote and verse expose `attribution()` and `citetitle()`; `caption_content()`
reads an authored prefix separately from its generated label and ordinal.

A title, alt, attribution, citation, caption, or reference label has one semantic
owner. The attribute inventory reads that same value; it does not keep another
raw copy that can become stale. Controls and unknown extension attributes remain
scalar values. Document assignments have their own header snapshot and ordered
body events.

`AttributeValueRef` distinguishes text, booleans, valueless attributes, inline
content, tokens, and substitution settings. `as_str()` reads exact scalar text,
including empty strings and meaningful quotes. It does not render inline content.
`roles` and `options` alias `role` and `opts`; iteration yields each canonical name
once, in unspecified order. `TokensRef::is_present()` distinguishes an absent
assignment from an explicit empty one; `is_empty()` reports whether it has tokens.
An explicit empty assignment remains in the attribute inventory and native JSON.
Token equality compares presence and token order. An empty role on a formatting
highlight makes it a generic span rather than marked text.

Use `TokenAssignment::default()` for absent prepared roles/options and
`TokenAssignment::new(Vec::new())` for explicit emptiness. Convert prepared
vectors with `.into()`. Transfers retain presence; `into_tokens()` returns
`None` for absence and `Some(Vec::new())` for an empty assignment.

Missing content is `None`; explicitly empty content is `Some` with no inlines.
`Title::default()` is absent; `Title::new(Vec::new())` is present and empty.
An image's alt stays separate from its caption and tooltip. Converters choose
filename fallbacks and escape output attributes. The parser resolves supported
content substitutions using the selected compatibility mode and declaration
snapshot; reading or serializing the tree does not parse the value again.

Named `reftext` belongs to the referenced element. Xrefs keep their destination,
explicit label, and automatic-label policy separate. Footnote occurrences separate
references from definitions: `content()` is absent on a reference and present,
possibly empty, on a definition. Each AsciiDoc table cell owns a separate catalog,
read with `TableCellRef::footnotes()`; `DocumentRef::footnotes()` contains only the
outer document's definitions. Names do not cross outer, sibling, or nested cell
boundaries. Ordinary cells share their containing owner. Numbers remain unique
across the document in both compatibility modes. Empty and forward source
references do not create definitions or bind to a later declaration.
Native JSON marks a cell owner with `footnote_scope: true` and retains each body
in its defining occurrence. Named ASG puts canonical bodies in the owning cell's
`footnotes` array, including an empty array for an owner without definitions.
Prepared edits can select an owner with `TableCell::set_footnotes()`; changing
a cell's style does not change ownership. `rebuild_footnotes()` repairs each
owner separately and can reuse its retained catalog after a definition is removed.
Index terms keep display labels separate from registration-time labels.

Locations identify recorded source ranges. Expanded text may point to the
authored attribute reference rather than an editable literal. IDs have their own
ranges and distinguish authored from generated section IDs. `ElementIdRef::location()`
returns `None` for generated IDs and caller-built IDs without a recorded range;
use the owning node's location for navigation. Changing a location does not move
child locations. Use the original source before offering an edit.

### Construction and editing

Copy only what you need with `Document::from(document_ref)`, `Block::from(block_ref)`,
or a concrete owner's `From` conversion. Copies are independent trees but can
still borrow text from the parse result. Use constructors, setters, and `take_*`
methods to transfer prepared values. `ContentAttribute::from(content_ref)` copies
selected content; `into_inlines()` transfers it without copying.

Owned fields are private. Use checked metadata/scalar setters so controls cannot
shadow a dedicated content field. A rejected replacement leaves the owner
unchanged. Setters accept prepared values: they do not parse markup, replay
document assignments, encode destinations, or fetch resources.

Catalogs remain snapshots after an edit. Reparse changed source to recompute
declaration-order substitutions and registrations. For prepared tree edits, choose
the explicit operation you need:

| Operation | What it updates |
| --- | --- |
| `rebuild_references_and_toc()` | Target/TOC membership, labels, aliases, and section numbers; keeps assigned IDs |
| `rebuild_captions()` | Caption ordinals and caption snapshots on existing references; keeps stored prefixes |
| `rebuild_footnotes()` | Bindings, registered definitions, and note numbers; keeps definition provenance |
| `rebuild_author_attributes()` | Author-derived document values; keeps explicit assignments and caller policy |
| `renumber_sections()` / `renumber_captions()` | Existing numbers only; no membership discovery or complete catalog repair |

These operations do not reinterpret source. Reference, caption, and footnote
rebuilds return an error without partial changes when required prepared context
is missing or a binding is ambiguous. When composing repairs after structural edits,
run them in this order:

1. `rebuild_footnotes()` can restore a retained definition body into a surviving
   occurrence, including anchors in that body.
2. `rebuild_references_and_toc()` discovers those anchors and repairs target and
   TOC membership.
3. `rebuild_captions()` refreshes caption ordinals and their snapshots on the
   rebuilt references.

Each call is atomic on error; the sequence is not a transaction. If a later repair
fails, earlier successful repairs remain applied. There is no general source-level
attribute editor.

### Choosing reads for a consumer

Converters match block/inline kinds and render resolved content. The LSP and
editor combine those reads with original source ranges; previews use the HTML
converter. MCP uses structural children, attributes, and registered targets for
navigation and selected rendering. Lint and command discovery use the same typed
walks. None of these consumers need access to the backing vectors or scalar maps.

Run `cargo run -p acdc-parser --example read_document --all-features` from
the workspace root for a complete example of nested reads, attributes, image
content, source coordinates, and a prepared copy.

## Cargo features

| Feature | Default | Effect |
| --- | --- | --- |
| `asg` | Off | Checked export to the pinned ASG draft subset. |
| `network` | Off | Compiles built-in HTTP(S) include support outside bare WebAssembly. Caller permission and safe mode still apply. |

Block `subs=` settings work in every parser build and both compatibility modes.
Draft mode supports these explicit controls as an acdc extension.
The `pre-spec-subs` and `setext` Cargo features have been removed. Remove them
from dependency feature lists; language behavior follows `CompatibilityMode`.

## Compatibility modes

Parsing defaults to `CompatibilityMode::Asciidoctor`. Its goal is maximum
compatibility with Asciidoctor, with documented exceptions for bug fixes and
deliberate improvements to the user experience. Unintentional differences remain
compatibility gaps to fix. Select `CompatibilityMode::Draft` with
`OptionsBuilder::with_compatibility_mode()` to use the supported draft rules.
Both modes use the same AST and are available in every build. The selected mode
applies to includes and nested content. Each parse keeps its own mode.

Choose the mode through parser options:

```rust
let options = acdc_parser::Options::builder()
    .with_compatibility_mode(acdc_parser::CompatibilityMode::Draft)
    .build()?;
let parsed = acdc_parser::parse("A *bold* paragraph.", &options)?;
# Ok::<(), acdc_parser::Error>(())
```

The same options work with string, file, reader, and inline entry points.
Document attributes cannot select a different mode. Safe mode, include loaders,
resource limits, and output format stay separate.
Use `asciidoctor` and `draft` when parsing or serializing the mode as a string.

See [the architecture document](../ARCHITECTURE.adoc#compatibility-mode-contract) for the reasoning.

In ordinary inline content, Asciidoctor mode processes formatting before
attribute references. Draft resolves references first, so a value such as
`*Bold*` creates bold content when inserted. Protected passthrough text stays
literal. Explicit substitution lists keep their requested order; incremental
block modifiers start from the selected mode's defaults.

Unsupported inline `pass:` lists use the same recovery rules as formatted
attribute values. Unknown names, callouts, and escaping after formatting or
macros retain the complete source and report `ContentRecovery`. For example,
`pass:q,c[*Bold*]` stays literal. Asciidoctor can instead display escaped
generated markup or ignore unknown names. acdc keeps backend markup out of the
parser. Disabled macro substitutions and escaped or protected passthroughs do
not trigger these warnings.

Formatted document attributes honor explicit substitution order. With
`:word: *Bold*`, `:value: pass:q,a[{word}]` keeps the asterisks literal;
`pass:a,q[{word}]` produces bold content. References use the values available
at definition time, including when the value is reused through an alias.
Later definitions do not fill previously missing references. If formatting
runs before an empty reference expands, the empty formatted span remains.

Explicit formatted-attribute lists run once. Asciidoctor may apply surrounding
macro and replacement substitutions again when a value is inserted into normal
prose. acdc preserves the explicit list's interpretation without generating
backend markup or nested links.

For named block titles, Draft parses inline content only in single-quoted
values. It also accumulates distinct block roles and options in source order. Unquoted and
double-quoted named titles stay literal, including formatting markers,
typography, character references, and macro text. Dot titles continue to support
inline formatting in both modes.

Draft keeps block image alt text literal, including single-quoted values. Attribute
references still expand, but formatting, typography, character references, and
macro-like text are preserved without registering macros. In Asciidoctor mode,
single-quoted alt in a preceding block attribute list supports inline content;
other preceding alt values and alt inside `image::...[]` remain scalar.

Inline image and icon alt text and tooltips support rich content in both modes
as a shared acdc extension; the draft has not settled these inline rules. Alt and
tooltip fields retain their existing content types. In native JSON, Draft block
alt uses protected literal nodes instead of formatted nodes or ordinary text.
Absent and explicitly empty alt values stay distinct, and converters choose any
filename fallback. A block image's title is its caption, not a tooltip.

Link, URL, mailto, and explicit xref labels support rich inline content in both
modes. Quoted label and tooltip values keep their own escape rules; bare URLs
inside a label stay text. Mailto subject/body and scalar controls do not become
inline content. Only selected labels and tooltips register macros. Bracketed
xref labels preserve whitespace; shorthand labels skip leading ASCII whitespace
and keep trailing and nonbreaking spaces, matching Asciidoctor. An absent or
empty xref label uses automatic reference text.

Empty inline `stem:[]`, `asciimath:[]`, and `latexmath:[]` formulas are shared
acdc extensions. Both modes recognize them as math with empty content, while
Asciidoctor leaves isolated empty forms literal. Whitespace inside a formula
is retained. Escaped forms stay literal, and adjacent macros remain separate.

Keyboard sequences choose their first plus/comma separator after the first key
character. A doubled final separator gives a literal key, as in `kbd:[Ctrl++]`.
Menus use `>` when present, otherwise commas. Nested labels and escaped brackets
stay inside their owning macro. Empty UI macros and formatted menu targets are
shared acdc extensions; Asciidoctor can leave them literal or capture adjacent
macros. UI availability still follows the selected mode and `experimental`.
`kbd:[]` has no keys; `kbd:[ ]` has one empty key. `menu:File[]` has no items;
`menu:File[ ]` has one empty item, which Asciidoctor omits. Empty and
whitespace-only button labels both remain empty labels. Caller-built empty
formulas and UI components also remain valid.
Ordinary attributes can introduce key and menu separators. When `subs=` puts
macros before attributes, introduced separators stay inside the existing label.
Explicit attribute profiles keep their parsed content together as one label;
acdc preserves this protection where Asciidoctor splits the inserted text.
Menu separators follow the selected substitutions. Under `subs=macros`, write
`&gt;` for an item separator; a raw `>` stays in the label. The usual substitutions
accept both. Numeric references such as `&#62;` remain label text, as in Asciidoctor.
Quoted sequences such as `"File > Open"` use the same UI rules. The first character
must be a Unicode word character or `&`, with a space or line feed on each side
of at least one item separator. Tabs and leading spaces leave the quotes literal.
A separator inside a child macro stays with that macro, preserving valid nesting.

Draft block attribute lists reject leading and trailing spaces or tabs, including
whitespace introduced by attribute references. A closing quote must be followed
by a comma, space, tab, or the end of the list; otherwise the value keeps its
literal quotes. These rules also apply to block macros. Inline macro lists keep
their existing rules in both modes. Asciidoctor mode accepts adjacent text after
a closing quote and trailing whitespace in block lists. A leading space or tab
prevents a block attribute line from being recognized in either mode.

Draft counts backslashes before attribute references and matching quotes in block
lists, and before closing brackets in block anchor labels. Each pair becomes one
literal backslash; an odd final backslash escapes the next token. Other
backslashes stay literal. Asciidoctor mode retains its compatible escape rules,
including literal backslashes before brackets in anchor labels.

Draft leaves quoted first-position values and shorthand with empty components
literal. It also supports an anchor prefix followed by other shorthand, such as
`[[intro,Introduction]note#chosen.example]`. The hash ID wins and the reference
text is retained. Block macro lists do not interpret shorthand in either mode.

Stacked attribute lists keep the original positional slots. Named entries leave
earlier positional values intact. An empty position clears an earlier value in
Asciidoctor mode; Draft keeps that value. A named role replaces earlier roles in
Asciidoctor mode, including when it is empty. Shorthand roles in the same list
are added after named roles. Options from preceding lists remain active on block
media macros in both modes. Draft splits options on commas or whitespace;
Asciidoctor splits on commas and removes spaces when a comma is present.

After the block form is known, Draft maps positional values to names in document
order: the later value wins. This includes positions from preceding image and
video attribute lists. Asciidoctor mode keeps the reference rules: source and
quote positions override named values, while media dimensions and video posters
come from the macro itself. A preceding image style is only a fallback for alt
text. Both modes retain positional slots for callers after mapping.

Draft matches block boundaries before applying a style. For example, `[literal]`
above `* Item` produces a list; Asciidoctor mode keeps a literal paragraph.
Draft also lets block metadata, delimiters, and headings end styled paragraphs.
This follows [SDR-3](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/sdrs/sdr-003-block-parsing-and-style.adoc); discrete headings remain the explicit exception. The
Asciidoctor rules for styled code and verse paragraphs stay unchanged.

Heading styles follow the first positional value in Asciidoctor mode. For
example, `[discrete]` followed by `[style=normal]` stays discrete, while
`[style=discrete]` alone remains a section. Draft uses the last selected style,
including named styles. Role-only and empty lists keep an earlier style.

Asciidoctor mode ignores an unquoted named `None` entry, preserving any earlier
value. Draft treats it as ordinary text. Quoted and positional `None` remain
text in both modes. This policy also applies to inline-image attributes.

The attribute-list differences follow SDR-2 and SDR-6 at revision
`68ed0b22e8d9b919897542d8ae14f03e7dbdd2e4`; Draft does not mean complete draft
specification conformance. Both modes honor explicit block `subs=` settings.
Asciidoctor mode recognizes legacy underlined headings; Draft requires ATX
headings. Safe mode, network access, and output format stay separate.

The former `Options::setext` field and `OptionsBuilder::with_setext()` method
have been removed. Select `CompatibilityMode::Asciidoctor` for legacy
underlined headings, or `CompatibilityMode::Draft` for ATX-only headings.

Both modes support Markdown-style hash headings, `>` blockquotes, and fenced
code blocks. acdc accepts fences of three or more backticks with a matching
closing length; Asciidoctor recognizes three-backtick fences. Both modes also
support nestable open blocks with four or more tildes, as proposed by [SDR-1](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/sdrs/sdr-001-open-block-delimiter.adoc).
These shared extensions do not require Draft mode. The legacy `--` open-block
form stays available in both modes without a deprecation warning.

`[float]` remains an alias for `[discrete]` in both modes and reports
`WarningKind::LegacyFloatDiscreteHeading`; use `[discrete]` for new documents.
`[normal]` turns indented text into a paragraph in both modes. `[paragraph]`
does the same in Draft; Asciidoctor mode keeps it literal, matching Asciidoctor.
Uppercase `[X]` marks an unordered item as checked in both modes, alongside
`[x]` and `[*]`. Ordered items keep these bracketed markers as ordinary text.

Asciidoctor mode requires `:experimental:` for `kbd:[]`, `btn:[]`, and
`menu:` macros. Draft recognizes them without that attribute. This is an acdc
choice inspired by the unfinished draft outline, not a settled draft rule.

Asciidoctor mode honors supported `compat-mode` effects on reference labels,
natural targets, and include fallback roles. Draft preserves the attribute and
reports `WarningKind::UnsupportedCompatMode`, without applying those effects.
Document attributes cannot change the selected compatibility mode. Ordinary
settings such as doctype and hard breaks still work in both modes.

Sibling and nested list items interrupt principal text in both modes. In an
attached paragraph, Asciidoctor lets a sibling or ancestor of the same list
family and numbering form start the next item. Draft keeps that line as text
until a blank line or another paragraph boundary. A standalone `+` interrupts ordinary
paragraphs in both modes. Unordered and ordered markers alone do not.
Metadata before a nested list keeps it attached to the current item.
Description lists nest under ordinary items with or without a blank line or `+`.
A term on the same line as the ordinary marker remains principal text.
A blank line before `+` returns an attachment to the ordinary parent;
without that blank line, the attachment belongs to the description definition.
A description-list term and its marker must appear on the same line. A marker
on a later line does not turn an ordinary paragraph into a description list.

Consecutive description terms share one optional definition. Read all terms with
`item.terms()` and read their definition with `item.definition()`. Each term owns
its content, anchors, delimiter, and source span. The definition owns optional
`principal()` content and attached `blocks()`. An absent principal differs from
one that was authored but expands to empty. Blank lines and line comments keep a
pending term group; a delimited comment block ends the list.

Structural traversal visits the group, its terms in order, then the definition
and its blocks once. Owned edits use `DescriptionListTerm` and
`DescriptionListDefinition`; moving a definition does not copy it into each term.
Native JSON stores `terms` records and an optional `definition`, with optional
`principal` and one `blocks` collection. An empty principal is an empty array;
a retained empty passthrough still has its raw node.

Named titles parse the expanded attribute value. For example, with
`:word: *Bold*`, `[title='{word}']` produces bold text in both modes.
Asciidoctor mode also parses double-quoted and unquoted titles. Draft keeps
those values literal and does not implicitly expand introduced references
again. Asciidoctor mode performs the additional reference expansion;
explicit passthrough substitutions remain available in both modes.

Both modes use the document-attribute values in scope when the named title was
declared, including for the additional expansion. Later metadata lines cannot
change that title. This deliberately differs from Asciidoctor when an introduced
reference resolves to a value changed after the title declaration. Dot titles
use the ordinary inline substitution order and the attributes in scope after
the block's metadata.

Quote attributions, citation titles, and custom caption prefixes retain the
attributes in scope when selected values were declared. In both modes, single-quoted values support
inline formatting independently of the body's `subs=` setting. Unquoted and
double-quoted values stay literal. Asciidoctor mode expands references introduced
by the first attribute-list expansion; Draft keeps them literal unless an
explicit passthrough requests expansion. Named and positional quote values
follow the same content rules. Captions inside block media macros stay literal,
including single-quoted values. Custom prefixes do not consume caption numbers.

Reference labels use restricted formatting in Asciidoctor mode: link and
footnote macros stay literal. In Draft mode, single-quoted named labels support
inline content; unquoted and double-quoted labels stay literal. Natural-target
lookup still uses the expanded label's markup, so `Custom *Label*` and
`Custom Label` are different targets.

Named index macros retain scalar controls separately from their labels.
Use `IndexTermRef::attributes()` to read identity, roles, options, `terms`,
`see`, `see-also`, and extensions. Hierarchy and see-also values use
`AttributeValueRef::ContentList`; iterate each value's inline content separately.
`catalog()` exposes registration-time labels without creating another occurrence.
Controls freeze when macros are recognized; display labels may still undergo
later substitutions. An index macro ID is retained but does not register an anchor.
Native JSON uses the same node form for an index term and its enclosing inline node.

## Native AST JSON

With the optional `asg` feature, `asg::export_document(parsed.document())` and
`asg::export_inlines(parsed.content())` export a separate, checked draft ASG value.
Unsupported semantic values return typed errors with JSON Pointer paths; export
never changes the AST. The current subset covers headers and authors, unnumbered
ordinary sections, paragraphs, promoted titles/reftext/IDs, plain or raw content,
resolved typography, named/numeric character references, and the draft's four
inline span types. Literal reference-shaped text stays text. See
[the support contract](schemas/asg/README.adoc) and
[coverage guide](schemas/asg/COVERAGE.adoc) for exact limits.

For the named acdc extension, call
`asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)`.
Its output identifies `urn:acdc:asg:1` and preserves resolved captions,
attribution, citation content and images. Use
`asg::export_inlines_with_profile(parsed.content(), asg::Profile::Acdc)` for standalone
inline content. This choice is independent of
`CompatibilityMode`; unsupported nested values still fail export.

Native JSON preserves acdc's model; it is not a claim of ASG conformance.

Run `cargo run -p acdc-parser --example asg_schema_audit --all-features --locked --offline`
from the workspace root for an offline shape audit against the pinned upstream
schema. It does not rewrite fixtures or export native JSON as ASG. The report
separates schema failures from invalid or unavailable source locations.

An inline macro uses the same node shape when serialized directly, through its
concrete owner, or inside an `InlineNode`. Mail links retain unencoded `subject`
and `body` values, including explicit empty strings. An angle-bracketed autolink
sets `bracketed` to `true`; omission means `false`.

Standalone passthrough nodes use `name: "pass"`, their optional text, and any
requested substitutions. Document parsing normally resolves passthroughs into
inline content first. Native serialization does not interpret their text or
apply ASG restrictions.

Document attributes keep their semantic JSON types: presence is `true`, validated
integers are numbers, and text stays text, even when it contains only digits.
The header is a snapshot; accepted body assignments remain ordered nodes, with
`false` for an unset. Text reads still retain an explicit integer's original spelling.

Content attributes appear only in their dedicated fields as inline arrays.
Absent content is omitted; explicit empty content is `[]`. Generated caption
labels and counters are omitted. Scalar maps retain controls and extensions;
serialization rejects reserved block keys that conflict with dedicated fields.
Raw inline nodes retain their literal text and nonempty `substitutions` list in
order. An omitted list means no substitutions. Serialization never reinterprets
this text.

## Migrating from 0.11

The public tree API now separates reads from construction and editing. This
migration describes the last released API, 0.11.0.

| In 0.11 | Now |
| --- | --- |
| `parsed.document().blocks` and other public storage fields | `parsed.document().blocks()` and typed `ast::*Ref` reads; match `BlockKind` / `InlineKind` |
| Borrowed slices and `Deref` on content/title | `ContentRef::from(&content)` or `title.content()`, then `inlines()` |
| Inline-result backing slices | `parsed.content()` |
| Struct literals or field writes | Constructors and prepared setters/transfers; use `From` to copy a selected typed read |
| Common metadata maps for all values | `attributes()` for one typed inventory; dedicated content retains its semantic owner |
| Scalar image alt and title/tooltip handling | `InlineImage` for inline images, `Image` for blocks; optional alt/tooltip content stays separate from caption titles |
| String UI labels | Content values for button labels, keyboard keys, and menu targets/items |
| Scalar reference labels | Optional parsed content on anchors and registered targets |
| `Video::sources` | One complete source, including provider playlists; the provider stays in `poster` |
| Feature-controlled language behavior | `CompatibilityMode`; remove `pre-spec-subs` and `setext` from dependency feature lists |

Public names now identify cells, reference targets, formatting spans, and the
scope of text operations explicitly. Update imports and calls as follows:

| Previous name | Current name |
| --- | --- |
| `TableColumn` | `TableCell` |
| `CurvedApostrophe` / `CurvedQuotation` | `SingleQuotedSpan` / `DoubleQuotedSpan` |
| `Reference` | `ReferenceTarget` |
| `Form` | `FormattingForm` |
| `Pass` | `Passthrough` |
| `DiscreteHeader` | `DiscreteHeading` |
| `Keyboard` / `Menu` / `Url` | `KeyboardShortcut` / `MenuPath` / `UrlMacro` |
| `CalloutRef` / `CalloutRefKind` | `CalloutMarker` / `CalloutMarkerKind` |
| `ParseResult::source()` / `ParseInlineResult::source()` | `preprocessed_source()` on both owners |
| `ElementAttributes::insert()` / `merge()` | `insert_if_absent()` / `merge_defaults()` |
| `BlockMetadata::is_default()` | `is_empty_for_serialization()` |
| `Reference::section_name()` | `ReferenceTarget::section_category()` |
| `Source::get_filename()` | `Source::file_name()` |
| `Anchor::with_xreflabel()` | `Anchor::with_reftext()` |
| `substitute()` | `apply_attribute_substitutions()` |
| `parse_text_for_quotes()` | `parse_inline_formatting()` |

The corresponding typed handles and enum variants use the same names. The inline
passthrough handle is `InlinePassthroughRef`; `PassthroughRef` still reads a
passthrough block. A callout marker is read through `CalloutMarkerRef`.
`OptionsBuilder::with_strict()` remains the general strictness switch.
ASG exports use `export_document()` / `export_inlines()` and their
`_with_profile` variants. These name changes preserve parsing, rendered output,
and serialized node names.

Keep `ParseResult` alive for text borrowed by reads and selected copies.
Use `DocumentRef::from(&document)` to read a constructed document. To edit a
parsed tree independently:

```rust
use acdc_parser::{Document, Options, parse, ast::DocumentRef};

let parsed = parse("First paragraph.", &Options::default())?;
let mut edited = Document::from(parsed.document());
let blocks = edited.take_blocks();
edited.set_blocks(blocks);
assert_eq!(DocumentRef::from(&edited).blocks().count(), 1);
# Ok::<(), acdc_parser::Error>(())
```

Prepared edits leave catalogs unchanged until an explicit rebuild. See
[Construction and editing](#construction-and-editing) for each operation's scope.
The default safe mode remains Secure, as in 0.11.0.

Read an admonition's source form with `AdmonitionRef::form()`. It distinguishes
paragraphs, indented paragraphs, and the actual opening delimiter. Library-built
compound admonitions start with `AdmonitionForm::Unspecified`; use `with_form`
when the source form is known. Prepared copies and body edits retain it.

Native JSON is a separate migration from the Rust layout:

- Titles, alt, tooltips, credits, caption prefixes, and reference labels use
  inline arrays at their dedicated fields. Consumed scalar duplicates are gone.
  Absence is omitted; present empty content is `[]`.
- Inline macro owners serialize in the same node shape as their enclosing inline
  node. Identity, role/option tokens, and scalar controls use consistent metadata.
  Mail subject/body values remain unencoded, including explicit empty strings.
- Footnote references omit `inlines`; definitions retain it, including `[]`.
  Bibliography labels and index registration labels remain distinct from their
  displayed or citation text. Autolinks retain whether angle brackets were written.
- Resolved typography uses symbol nodes. Character references use `charref` nodes
  with named values or canonical decimal numeric spelling. Source ranges still
  identify the authored spelling.
- Document presence and unsets use `true` and `false`; validated integers are
  numbers. Numeric-looking ordinary text stays text. Header values remain a
  snapshot and body assignments remain ordered nodes.
- Admonitions write `form` and retain the actual `delimiter` only for delimited
  forms. Source-free compound admonitions omit both.
- Video JSON uses `source`, preserving commas and empty playlist slots. Raw nodes
  retain nonempty ordered substitution lists; an omitted list means none.

Do not treat native JSON as upstream ASG. Enable `asg` for checked export with
`asg::export_document(parsed.document())` or `asg::export_inlines(parsed.content())`. The default
profile uses the pinned upstream subset and rejects unsupported meaning. Select
`Profile::Acdc` explicitly for the versioned content/image extension. See
[the schema support contract](schemas/asg/README.adoc) for exact coverage and
permitted source-only omissions. Export never reparses content or repairs catalogs.


## Migrating from 0.10

The parser now defaults to `SafeMode::Secure`, matching the Asciidoctor API.
This applies to `Options::default()`, the options builder, and all parse functions.
Include directives become links. The parser does not read their targets.
`parse_file` still reads the entry file that the caller selects.

To enable includes in a trusted project, select Safe and set the include base:

```rust
use acdc_parser::{Options, SafeMode};

let options = Options::builder()
    .with_safe_mode(SafeMode::Safe)
    .with_base_dir("/workspace/docs")
    .build()?;
# Ok::<(), acdc_parser::Error>(())
```

Select `SafeMode::Unsafe` to allow local includes outside the base directory.
`IncludeLoader::System` remains the default loader.
A custom provider does not change the safe mode.
For remote includes, the caller must also set `allow-uri-read`.

The CLI keeps its existing defaults. `convert`, `lint`, and `inspect` use Unsafe.
`execute` uses Safe. Use `--safe-mode secure` to prevent include reads.

Secure controls include reads and access to local path attributes.
It does not sanitize rendered HTML or sandbox commands.
Safe and Server restrict include paths to the base directory, but do not resolve symlinks.
A symlink can point outside that directory.
The 10 MiB limit applies to selected text from each include.
All includes in one parse also share a budget after indentation, with a default of 64 MiB.
These limits do not cap total parser memory use or scan time.

## Migrating from 0.9

Attribute configuration now validates inputs and returns `Result`. Pass an
iterator instead of building a mutable `DocumentAttributes` map:

```rust
use acdc_parser::{DocumentAttributeAssignment, Options, parse};

let options = Options::builder()
    .with_attribute("max-include-depth", "8")
    .with_attribute("sectnums", true)
    .with_default_attribute("imagesdir", "images")
    .build()?;
let parsed = parse(":project: acdc\n\nContent.\n", &options)?;
let attributes = parsed.document().attributes();
assert_eq!(attributes.get("project").and_then(|v| v.as_str()), Some("acdc"));

if let Some(DocumentAttributeAssignment::Set(value)) = attributes.assignment("project") {
    assert_eq!(value.text(), Some("acdc"));
}
# Ok::<(), acdc_parser::Error>(())
```

Use `Options::document_attributes()` to read configured values and
`Options::into_builder()` to change configuration. `with_attributes()` replaces
earlier application inputs; `with_defaults()` replaces earlier defaults.
`DocumentAttributes::into_inputs()` exports values for fresh configuration.
`Options::with_document_attributes()` instead reuses an existing snapshot without
changing its assignment precedence.

`DocumentRef::attributes()` stops at the end of the header. Read later
`Block::DocumentAttribute` nodes in source order and match `assignment()` on
`Set(value)` or `Unset`. Attribute maps no longer expose mutation methods or
`get_string()`. Use `as_str()` for text, `as_integer()` for validated numbers,
`is_presence()` for a value set without text, and `text()` or `write_text()` for
the attribute's written representation.

Other API and JSON changes:

| In 0.9 | In 0.10 |
| --- | --- |
| `Position` line and column as `usize` | `u32`; use `Position::from_line_col()` for `usize` inputs. |
| `SourceLocation::positioning` and `Positioning` | `SourceLocation::location`; use `at_position()` or `at_location()`. |
| `Location::shift`, `shift_inline`, `shift_line_column` | Removed. Locations identify original source; use `ParseResult::source_location()` to resolve files. |
| `TocEntry::numbered` and `style` | `kind: SectionKind` and `number()`. |
| Index labels as strings | Inline-node slices from `term()`, `secondary()`, and `tertiary()`; JSON labels are arrays. `InlineMacro::IndexTerm` contains a `Box`. |
| Only `Explicit` and `Modifiers` substitution specifications | Also handle `SubstitutionSpec::Source`; parsed lists and JSON retain source entries. |
| `macros_disabled()` and `attributes_disabled()` | `BlockMetadata::uses_substitution()` or `SubstitutionSpec::resolve()`, with the block's defaults. |
| Unconditional substitution configuration types | `SubstitutionSpec`, `SubstitutionOp`, and `BlockMetadata::substitutions` require `pre-spec-subs`. |
| `inlines_to_string()` | Removed; walk inline nodes with the text policy your application needs. |

`Position` JSON adds a `file` array for included content. Index relationships add
an optional `relationship` field. Document-attribute JSON keeps its existing shape.
The `ParseResult` ownership model is unchanged from 0.9.

<details>
<summary>Features supported</summary>

* [x] Document Headers
    * [x] Author parsing (first/middle/last name, email)
    * [x] Revision info
* [x] Section
    * [x] ATX-style (`=` markers)
    * [x] Setext-style (underlined, Asciidoctor mode)
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
    * [x] Link macro with custom schemes and paths that contain colons
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

## STEM notation

STEM blocks and inline macros keep formula text for converters. The parser does
not validate AsciiMath or LaTeX formulas. The converter decides how to handle
unknown commands and incomplete formulas.

The following cases were checked against Asciidoctor 2.0.26 with the `html5`
backend. See [STEM syntax](https://docs.asciidoctor.org/asciidoc/latest/stem/).

| Input | Selected notation |
|-------|-------------------|
| `stem:[...]` or `[stem]` without `:stem:` | AsciiMath |
| Empty `:stem:` or an unknown value | AsciiMath |
| `:stem: asciimath` | AsciiMath |
| `:stem: latexmath`, `:stem: latex`, or `:stem: tex` | LaTeX |
| `latexmath:[...]` or `[latexmath]` | LaTeX, overriding the document |
| `asciimath:[...]` or `[asciimath]` | AsciiMath, overriding the document |
| `[stem,latexmath]`, `[stem,latex]`, or `[stem,tex]` | LaTeX, overriding the document |
| `[stem,asciimath]` or `[stem,unknown]` | AsciiMath, overriding the document |
| `[stem,]` | The document notation |

Body assignments change the notation for following content. Caller-supplied
attribute values retain the normal attribute locking rules.

By default, formula bodies keep attribute references and formatting markers
as formula text. Inline passthroughs run before STEM macros: `stem:[x=+y+]`
contains `x=y`, while a literal `+` remains part of the formula.
A STEM block can opt in to attribute expansion with
`[stem,subs="+attributes"]`. The original block body remains available through
`source_text()`. STEM supports attribute expansion and output escaping; other
requested stages produce a source-recovery warning and leave their markup as
formula text. This differs from Asciidoctor, which can insert formatting tags
into formulas. HTML conversion escapes formula text by default;
`[stem,subs=none]` preserves authored raw HTML in every build.

Source locations cover the original macro or block, including escaped brackets
and dollar delimiters. Nested includes and line selections retain the source
filename and positions.

acdc keeps these differences from Asciidoctor 2.0.26:

- Empty inline macros remain STEM nodes. Asciidoctor leaves an isolated empty
  macro as plain text and can match a later closing bracket instead.
- `\[` and `\]` become literal brackets, and `\\` becomes one backslash.
  Asciidoctor only removes the escape before `]` and can reject a macro whose
  closing bracket follows a doubled backslash.
- Inline LaTeX removes one enclosing `$...$` pair. Additional dollar pairs
  remain in the formula; Asciidoctor can also remove a nested pair.
- Inline substitution lists such as `stem:attributes[...]` are not supported.
  Their complete source is retained with a content-recovery warning; nested
  formatting, attribute references, and macros stay literal. Asciidoctor accepts
  these lists. Escaping an unsupported form keeps its leading backslash and
  does not warn. STEM block substitutions can expand attributes; quote and macro
  substitutions inside a block formula are not interpreted as inline nodes.
- HTML character escaping runs on the final formula text, including text from
  nested passthroughs. Asciidoctor keeps the escaping rules of nested
  passthroughs. It can also escape an attribute value twice when an explicit
  substitution list puts `attributes` before `specialchars`.

## Parser options

* **Safe mode** - `Safe`, `Secure`, `Server`, `Unsafe`
* **Strict mode** - Rejects manpage titles that do not use `name(volume)`;
  other recoverable warnings remain warnings.
* **Base directory** - Entry-input include resolution through
  `Options::builder().with_base_dir(path)`
* **Document attributes** - `with_attribute` and `with_attributes` set application
  overrides; `with_default_attribute` and `with_defaults` set values the document
  can replace. Built-in read-only and API-only names remain protected.
* **Setext headers** - Two-line underlined headers in Asciidoctor mode
* **Manpage doctype** - `doctype=manpage` with derived attributes

Document configuration accepts standard Rust iterators. Application overrides
and document-overridable defaults use the same input values; callers do not need
to track parser locks or assignment policy.

```rust
use acdc_parser::{Options, parse};

let options = Options::builder()
    .with_attributes([("max-include-depth", "064")])
    .with_defaults([("imagesdir", "images")])
    .build()?;
let parsed = parse("= Example\n\nContent.\n", &options)?;
let depth = parsed.document().attributes().get("max-include-depth").unwrap();
assert_eq!(depth.as_integer(), Some(64));
assert_eq!(depth.text(), Some("064"));
# Ok::<(), acdc_parser::Error>(())
```

`with_attributes` replaces earlier application inputs; `with_attribute` sets one
input. `with_defaults` replaces earlier defaults. Use `false` or `()` to unset an
attribute, including one supplied by defaults.

## Parsing and conversion

`subs="attributes+"` moves an
already-enabled attribute stage to the front. Groups such as `normal+` move
their members together in group order. Modifiers apply left to right; append
syntax such as `+attributes` leaves an existing stage in place, matching
Asciidoctor. Use `-attributes,+attributes` to move it to the end.

A plain first entry replaces block defaults, including when later entries have
modifiers: `subs="quotes,+attributes"` enables only quotes and attributes.
A modifier first entry retains block defaults: `subs="+quotes,attributes"`
adds to those defaults. `none,+quotes` enables only quotes. These rules match
Asciidoctor; later entries do not reset the starting list.

Removing a group also disables its members during parsing. For example,
`-normal,attributes+` enables attributes without restoring formatting or macros;
`-verbatim,+callouts` restores callouts without restoring special-character
substitution. Asciidoctor leaves these callout markers literal without the
special-character stage. acdc retains source-based recognition of enabled callouts
under its SDR-5 parsing/conversion policy; SDR-5 does not specify this experimental
substitution combination.

AST and JSON substitution metadata retain the list entries. For example,
`verbatim,-macros` stays `["verbatim", "-macros"]`. Groups, aliases, duplicates
and modifiers are preserved; group expansion and removal happen only when the
effective stages are resolved. Metadata parsing still expands attribute references
in the list and trims whitespace around each entry.

Standalone `subs="none"` and `subs=""` retain `[]`, meaning no substitutions.
Absent substitution metadata uses block defaults. Mixed lists retain `none`:
`none,+quotes` starts empty and enables only quotes; removing `none` would
incorrectly restore block defaults.

Nonempty parsed lists use `SubstitutionSpec::Source`, except standalone `none`,
which retains `Explicit([])` like an empty value. Consumers that match this enum must
handle that variant; `resolve()` and the existing typed `Explicit`/`Modifiers`
constructors remain available.

acdc follows [SDR-5's separation of parsing and conversion](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/sdrs/sdr-005-formal-grammar-for-inline-syntax.adoc).
The parser expands source references and constructs inline nodes. Converters
produce output markup and apply the escaping required by their output format.
The parser may use explicit `backend` conditions, but must not render an attribute
to HTML or roff to decide its value.

Block substitution order also retains structured formatting and links when
escaping runs last, where Asciidoctor can display generated markup as text.
In source blocks with `subs="quotes,+attributes"`, acdc PDF keeps formatted text;
Asciidoctor PDF displays generated tags when special-character substitution is
absent. These retained differences keep the parsed structure independent of the
output backend.

Constrained formatting uses source punctuation as its boundary. For example,
`>*Bold*` produces a literal `>` followed by bold text; each converter escapes
the `>` for its output format. Asciidoctor's default substitutions escape it
before formatting and therefore leave the asterisks literal. In `pass:q[...]`,
both processors recognize formatting directly after a raw tag. Literal entity
references such as `&gt;*Bold*` retain their semicolon boundary and need doubled
formatting marks.

This follows [SDR-5's decision on special characters](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/sdrs/sdr-005-formal-grammar-for-inline-syntax.adoc#special-characters),
which assigns their encoding to converters after parsing. SDR-5 does not
prescribe this exact example; the behavior follows acdc's boundary rule and
deferred escaping.

Attribute text reads are independent of the output format. For example,
`:label: pass:c[Research & <Development>]` retains `Research & <Development>`
for `text()`, `write_text()`, native JSON, conditions, and include targets.
`DocumentAttributeValue::parts()` returns ordered `ast::TextPartRef` pieces. Each
piece exposes `text()` and `substitutions()` for its retained escaping stages.
Use this view for displayed attributes such as TOC titles;
use scalar text reads for paths and configuration. HTML applies these stages,
while terminal and PDF can write the literal text. An explicit `pass:[]` span
has no escaping stages. Aliases, body assignments, and cloned snapshots preserve
this distinction. `RawRef::substitutions()` can retain repeated escaping stages
without embedding generated HTML entities in `RawRef::text()`.
`RawRef::text_part()` exposes the same view for raw inline content.
Both views borrow existing text; attribute `text()` remains a contiguous scalar
read. Reading pieces does not apply a formatted attribute's deferred inline rules.

Definition-time escaping and later inline replacements remain separate.
`:label: pass:c[Literal &amp; and &#169;]` displays literal `&amp;` and `&#169;`
as a TOC title, while ordinary paragraph text `{label}` resolves them to `&`
and `©`. `(C)` remains a semantic copyright symbol when replacements are enabled;
converters choose its output representation. Asciidoctor stores encoded attribute
strings instead, so scalar comparisons and include paths containing `&`, `<`,
or `>` can differ. Native JSON shows neutral text; it is not a lossless export of
prepared attribute profiles. Reuse parsed values with
`Options::with_document_attributes()`; `DocumentAttributes::into_inputs()`
intentionally discards preparation when creating fresh configuration.

Formatted attributes retain AsciiDoc source text
and supported inline parsing profiles. For example, `:value: pass:q[*Bold*]`
retains `*Bold*` for `text()`, JSON, conditions, and include paths, while its
references can produce bold inline content. Asciidoctor instead stores generated
backend markup at definition time; acdc deliberately does not adopt that behavior.

References selected by `a` freeze at definition time. Explicit lists keep their
declared order when that source is used. Aliases retain their profiles and protect raw text.
An explicit `a` list imports profiles while keeping surrounding text literal;
an explicit structural list controls the complete expanded source.
Unused values register no macros; anonymous footnotes register separately at each
use, while named notes share their ID. Lists such as `q,c` or `m,c` that need
rendered markup, unknown substitution names, and text-only `a`/`c` combinations
that reference profiled values stay literal with a structured parser warning.
Profiles resolve enabled typography when their content is parsed, including code
and backtick spans. For example, `:value: pass:r[(C)]` supplies a copyright symbol
at each use; text reads return `©`. `:value: pass:r[\(C)]` supplies protected
literal `(C)`, including code that enables replacements. Asciidoctor removes
that escape during assignment and can later render `©` in ordinary prose and
replacements-enabled code. acdc's behavior follows its source-first policy under
SDR-5; SDR-5 does not prescribe these custom-list semantics.
Source arrows in ordinary and verbatim blocks follow the same substitution
order: `subs="replacements,specialchars"` keeps `->` literal, while
`subs="specialchars,replacements"` resolves it to `→`. Enabled signs, ellipses,
apostrophes, and dashes also resolve in verbatim-only profiles. Their Unicode
meaning survives later escaping; Asciidoctor may instead display its generated
entity spelling as text.

Both modes share the same inline boundary and escape rules. Escaping a complete
macro keeps its inner formatting literal and prevents it from registering notes or
references; Asciidoctor may still format text inside the escaped macro. Escaping
an unconstrained span keeps the complete span literal in acdc. Constrained spans
remove escapes from both markers.

Formatting treats ASCII letters, digits, and `_` as word characters. For non-ASCII
text, it uses Rust's alphabetic and numeric properties. Thus `é*word*` stays
literal, while a combining acute accent before `*Mark*` allows bold text.
Asciidoctor's broader word class leaves the latter literal. These shared rules remain stable
while the draft's word-boundary rules are open.

Visible index shorthand preserves balanced internal parentheses. For example,
`((Term (R)))` contains the complete label `Term (R)` and displays `Term ®`
when replacements are enabled. `((fn((x)) tail))` keeps its nested pairs;
`((word)))` leaves an extra `)` outside the term. Unbalanced literal parentheses
retain the existing compatibility delimiter rules. Unlike Asciidoctor's legacy
macro stage, acdc also keeps complete labels when replacements are disabled
or run after macros. This source-based delimiter policy keeps parsing separate
from conversion under SDR-5; SDR-5 does not specify these exact index rules.
Authored encoded arrows such as `pass:r[-&gt;]` also resolve to semantic symbols
when no earlier escaping stage protects their ampersands. Escaped arrows stay
literal. As with resolved signs, a later escaping stage preserves an arrow's
Unicode meaning; Asciidoctor HTML instead exposes its generated entity spelling.

The parser resolves word-internal apostrophes and complete word-bounded dash
pairs using neighboring text around ordinary anchors, index labels, and protected
passthrough text. For example, `Sam'pass:[s]` exposes the apostrophe as a resolved
symbol. Passthroughs keep their own substitution settings, including when the
surrounding text disables replacements. Formatting remains a boundary. Later attribute values cannot supply earlier word context,
and catalog labels retain their registration-time content. Enabled signs and
ellipses in visible index labels are also resolved before conversion. Dash pairs split across ordinary text, anchors, and index labels also resolve in
the parser. Each symbol stays with the first dash's owner. Its source range covers
only the text consumed from that owner; other owners retain their own ranges. Empty
passthroughs also preserve this context without contributing source characters.
Asciidoctor retains more passthrough boundaries.

Converters retain neighboring text across visible index labels,
hidden index terms, and ordinary anchors. With replacements enabled,
`prefixindexterm2:[--]tail` displays `prefix—tail` in HTML, PDF, manpages, and
terminal output, including highlighted code. The same context handles a split
dash pair in `prefix-indexterm2:[-]tail`, consumes one space on each side of
`prefix indexterm2:[--] tail`, and renders `Saindexterm2:[m]'s` as `Sam’s`.
Repeated spaced pairs share their source delimiters: `-- -- --` replaces the
first and third pairs, matching Asciidoctor. Escaped dashes and disabled
replacements remain literal; registration-time catalog text stays unchanged.
This matches Asciidoctor HTML and manpages when replacements run after macros
and there are no intervening generated anchors. Asciidoctor's default order
leaves this example literal, and Asciidoctor PDF retains more macro boundaries.
acdc deliberately uses parsed word context during conversion under its selected
SDR-5 policy; SDR-5 does not prescribe these exact typography rules. Formatting
and link boundaries still separate replacement runs. Passthroughs retain their
own substitution profiles: their text can supply word context, but replacements
cannot consume spaces, escapes, or dash halves from another profile. For example,
`prefix indexterm2:[pass:r[--]] tail` keeps its outside spaces and inserts the
spaced em dash resolved within the passthrough.

Explicit substitution controls are shared by both modes. They are an acdc
extension in Draft mode, not a claim of complete SDR-5 conformance.
SDR-5 leaves the exact mapping of custom `pass:` lists unresolved; support for
common profiles is an acdc policy. Earlier text-only escaping and ordinary
attribute-introduced formatting remain separate migration work. See the
[architecture document](../ARCHITECTURE.adoc) for the boundary and migration scope.

## Source text and diagnostics

`Paragraph::source_text()` and `DelimitedBlock::source_text()` return the body
after preprocessing and before inline substitutions. Preprocessing normalizes
line endings, removes trailing whitespace, and processes includes and conditionals.
Delimited text excludes metadata and delimiters but retains the newline before
the closing delimiter. Programmatically constructed blocks return `None` unless
they have retained text. This text does not change JSON or semantic equality.

Use `Block::metadata()` and `Block::location()` without matching each block
variant. `BlockMetadata::uses_substitution()` checks enabled substitutions against
the defaults for that block. `substitute_attributes()` performs one text-only
attribute pass; it does not apply other inline substitutions or shell quoting.

`ParseResult::source_location()` resolves an AST location to its original file,
including partial and nested includes. Each location boundary has its own include
chain. A span across files resolves to the file at its start. Use
`Location::byte_len()` for an inclusive byte length; it returns `None` across files.
For reindented includes, line and column are original-source coordinates but byte
offsets remain in preprocessed coordinates. Do not use them to slice the original
file.

`ParseResult::conditionals()` reports preprocessing decisions in source order,
including selected includes. Each decision retains its original file and opening
line, its closing line when present, and whether its content was active after
all enclosing conditions. These decisions use attributes at the directive, not
the end-of-header snapshot. Locations have line-level precision. They describe
the parsed source and remain separate from copied or edited document content.

`ParseResult::source_recovery()` returns the first warning about omitted or
recovered content, including disabled includes, incomplete tables, unmatched
block delimiters, and unresolved footnotes rendered as text. It remains available after `take_warnings()` and excludes
presentation warnings. Applications that need complete input must check it before
acting on recovered content. Rendering can continue with the recovered document.

Recoverable document problems return a document and located warnings by default
in both compatibility modes. Unknown include options are ignored while valid
options still apply, matching Asciidoctor.
Invalid `indent=` overrides are ignored with a warning. Indentation over
4,096 spaces is rejected before allocation and the include keeps
its original indentation. Malformed include syntax, invalid URLs, and invalid
`lines=` selections preserve the directive without reading the target. An invalid
selection never expands to the whole file. Include read or decoding failures
produce an unresolved marker and discard partial content. `opts=optional` silently
omits a missing local file; other nonfatal failures still warn and preserve a
marker. Explicit provider `Fatal` failures remain fatal.

A table closes only on its exact opening delimiter on a separate line. Without
one, parsing continues to the end of input and reports `UnterminatedTable`.
Column widths above `u32::MAX` warn and use proportional width 1, preserving the
other column controls. Table row, column, span, and cell expansion limits remain
fatal. List markers whose recognized depth exceeds 255 warn and remain literal
text.

Malformed conditionals and `ifeval` comparisons between different operand types
exclude their unevaluable content. Block forms retain an inactive frame, including
nested conditionals, until a matching `endif`. Invalid or mismatched `endif`
directives warn and leave the current frame open; an unclosed frame also warns at
the end of input. Invalid conditions in inactive branches cannot expose content
or trigger includes. These conservative recoveries deliberately differ from
Asciidoctor where it exposes content after a malformed condition. acdc also
preserves invalid include selections even where Asciidoctor ignores the malformed
option, preserves invalid URLs instead of producing links, recovers include
decoding failures, and bounds column widths and list depths.

A non-conforming manpage title returns `WarningKind::NonConformingManpageTitle`
and keeps the fallback name and volume 1. `Options::strict` still rejects it.
The title fallback does not indicate lost source content through
`source_recovery()`. CLI users can apply `acdc convert --fail-on-warnings` to all
structured warnings: conversion retains successful output, finishes the selected
inputs, and exits with status 1.

Entry-document read and decoding failures, invalid caller configuration, include
byte/metadata/transfer budgets, explicit provider fatal errors, and broken internal
state or source mapping still fail parsing. Generic grammar errors remain fatal
unless a specific recovery can preserve source and make forward progress.

Parser tracing records sizes, counts, positions, and diagnostic categories.
It does not dump document text, options, AST nodes, paths, or URLs.
Read full diagnostics from `ParseResult::warnings()`,
`ParseInlineResult::warnings()`, and returned errors.
These diagnostics can contain source text and paths.

## Intrinsic document attributes

acdc initializes the intrinsic backend, input, time, safe-mode, and environment
attributes before preprocessing. Later document assignments apply in source order.
File input derives `docdir`, `docfile`,
`docfilesuffix`, `docname`, and the document timestamp from the entry file. Server
and Secure modes conceal the directory and home path. `SOURCE_DATE_EPOCH` makes
both the document and conversion timestamps deterministic and formats them in UTC.

Cross-references do not load source files. After explicit includes are expanded,
references to the current file or fully included sources resolve within the same
document. By default, partial includes remain external, as in Asciidoctor. `CrossReference::target`
contains the effective fragment ID, and `target_is_local` distinguishes it from an
external resource even when the ID contains punctuation. An empty local target
addresses the document top; the empty entry in `DocumentRef::references()` supplies its
reference text. Converters must check the local flag before treating a target as
a filename. When changing a destination, update both fields.

Document `reftext` at the end of the header takes precedence over the label
on the header's explicit ID. Both targets use the document title when that
attribute is empty or unset. Explicit text on a cross-reference still wins.
The authored header label remains separate in the AST and named ASG output.
Caller attribute locks also apply to these labels; Asciidoctor can let header
metadata override a locked `reftext`.
In both modes, accepted body `reftext` entries select the label for following
references to either document target. Empty and unset values select the title.
Nested blocks and includes follow the same source order; AsciiDoc table-cell
changes remain local to their cell. Attribute references in a label use the values
at its declaration. Natural aliases retain their header declaration binding;
Asciidoctor's alias cache can instead depend on which reference is resolved first.

`CrossReferenceRef::document_reftext()` reads the retained body context. `None`
uses the header snapshot, while present empty content selects the title.
`CrossReference::set_document_reftext()` replaces or clears that context on an
owned reference. Native JSON and the named ASG profile expose `document_reftext`
separately from explicit inline text; the pinned upstream ASG profile rejects it.
Known label locations identify the winning attribute value, including its include
chain. Caller values have no source location.

Prepared reference repair retains both the header label and each occurrence's
body context when IDs, titles, attributes, or block order change. It does not
replay source declarations. `DocumentRef::attributes()` stays the end-of-header
snapshot.

For combined or partially included sources that keep file-qualified references,
enable `Options::builder().with_force_local_file_xrefs()`.
This turns `<<chapter.adoc#installation>>` into `<<installation>>` and keeps custom
link text. It requires a nonempty fragment. Multi-letter URI schemes and `//`
prefixes keep their normal behavior. A single letter before `:` is treated as a
Windows drive. Page-only references are unchanged.

Use this override only when the fragment identifies the intended local anchor.
It also redirects genuine external file references and does not scope duplicate
IDs. Missing anchors produce unresolved-reference warnings; target files are
never loaded. This option does not resolve Antora resource IDs. Antora Assembler
scopes anchors and resolves references before conversion; its prepared source
does not need this override. The option is disabled by default.

The parser records the effective attributes at the end of the document header in
`DocumentRef::attributes()`. Later accepted set and unset entries appear as ordered
`Block::DocumentAttribute` nodes and do not change that header snapshot. Each node
exposes `assignment()`, which returns `DocumentAttributeAssignment::Set` or `Unset`.
A set value provides `as_str()`, `as_integer()`, and `is_presence()` for semantic
reads, and `text()` or `write_text()` to retain its written representation.
`get()` borrows a value without copying it. `assignment()` and `assignments()` on
the header snapshot also expose explicit unsets. Rejected and invalid entries do
not become semantic AST events.

After a document title, the first empty line ends the header in both
compatibility modes. Before the title, Asciidoctor mode ignores empty lines
between attribute entries, comments, block anchors, and block attribute lists, as
Asciidoctor does. In a document without a title, the leading attribute entries
stay in the header however many empty lines separate them. Unlike Asciidoctor,
an entry that follows the first block's anchor, attribute list, or block title
is applied in the body. Draft mode follows the
language documentation: the first empty line ends the header, with or without a
title, and a title after that line is a level-0 section. Without empty lines,
both modes accept attribute entries, comments, block anchors, and block attribute
lists above the title in any order. To group header
attributes in either mode, separate them with comment lines instead of empty
lines.

`DocumentRef::header_location()` returns the source span of the header, with or
without a title. With a title it equals `HeaderRef::location()`;
`DocumentRef::header()` remains `None` without one.

Custom converters can use `acdc-converters-core::TraversalContext` with the shared
visitor to read attributes at each body position. The context applies events in
source order and restores parent attributes after nested AsciiDoc table cells.
Parser consumers that do not use the visitor can interpret the ordered set/unset
events directly; the parser exposes no mutable traversal or precedence state.

The header column in the Asciidoctor document-attribute reference identifies where a
feature normally reads its setup value. It is not a general ban on later assignments.
acdc accepts supported body assignments in source order, while setup-only consumers
continue to use the header snapshot. Caller values, API-only attributes, and read-only
intrinsic values remain protected. A source `backend` value can be visible as document
text, but it does not change the converter that the API or CLI already selected.
`outdir` and `outfile` are output metadata exposed by the converter result; they are
not available to source substitution.

## Recoverable document warnings

An accepted `toc2` setting produces one `WarningKind::DeprecatedToc2Attribute`
warning per parse. Use `:toc: left` in the document or set `toc` to `left`
through the API or CLI. TOC behavior is unchanged. The warning identifies the
first setting, with a source location for document entries and no location for
caller input. Unsets, ignored assignments, and the valid `toc2` CSS class do
not warn. Asciidoctor accepts the deprecated alias without this warning.

An unset entry with text after it, such as `:!foo: ignored` or
`:foo!: ignored`, still unsets the attribute and ignores the text. It adds a
located `WarningKind::UnsetAttributeValue` warning, including when the caller
locked the attribute. Asciidoctor ignores the text without a warning.

A body entry for an attribute that only takes effect in the header adds a
located `WarningKind::HeaderOnlyAttribute` warning. The entry still applies for
later attribute references, but the feature is unchanged. This covers document
setup (`doctype`, `toc`, `toclevels`, `toc-title`, `toc-class`),
`source-highlighter`, `eqnums`, HTML page settings, `untitled-label`, manpage
metadata, and document metadata such as `description`, `author`, and
`revnumber`. Attributes that acdc applies in the body, such as `icons`, `stem`,
`sectnums`, and `nofooter`, don't warn. Caller-supplied values and entries a
caller lock rejects don't warn either. Asciidoctor doesn't warn.

A section directly nested in a `[bibliography]` section remains in the AST and
adds a located warning to `ParseResult::warnings()`. Parsing and conversion
continue. Asciidoctor keeps the same content but reports the condition at error
severity.

## Include attribute lists

Include directives accept quoted values, whitespace around assignments, unknown
fields, and `options=` as an alias for `opts=`. Attribute references in the raw
list are expanded before fields are parsed. Single and double quotes protect
commas; unknown and positional fields are ignored, matching Asciidoctor.

`lines=` accepts open-ended ranges such as `12..`. A comma takes precedence
over a semicolon as the range separator. Malformed numbers remain errors so an
invalid selection cannot silently include the whole file. This strict validation
differs from Asciidoctor's leading-decimal conversion.

An include's unsigned `leveloffset` replaces the active offset. A signed value
adjusts it. Include attribute references are expanded before interpreting the
offset in both compatibility modes. Both modes honor caller-locked offsets and
`max-attribute-value-size` when an include requests an override. Unsigned values
retain their text until the limit applies: `leveloffset=01` with a one-byte cap
sets offset `0`.

When an include expands inside a listing or an existing paragraph, its
`:leveloffset:` entries can remain visible as literal text, as in Asciidoctor.
Those literal entries do not change the active offset.

For document entries, Asciidoctor mode interprets literal signed offsets before
expanding attribute references. Starting at offset `1`, `:leveloffset: {delta}`
with `delta=+1` sets offset `1`, and `:leveloffset: +{number}` with `number=1`
also keeps offset `1`. Draft mode expands references first, following
[SDR-2's attribute-entry preprocessing model](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/sdrs/sdr-002-block-attrlist-parsing.adoc).
Both entries therefore set offset `2`.

After an explicit include offset, Asciidoctor mode reapplies the previous value
as a document entry. A signed value introduced through a reference can therefore
adjust the offset again on return, matching Asciidoctor. Draft restores the
previous effective offset. Changes inside an include without an explicit offset
affect following content in both modes.

## Attribute value limits

The caller attribute `max-attribute-value-size` caps document-defined text values
in UTF-8 bytes. Values are truncated at a complete character. Secure mode defaults
to 4096 bytes; other modes have no default cap. The cap is fixed when parsing
starts. Document content cannot change this caller-only control.
Truncated values also affect subsequent include targets.
Setting the caller attribute without a value applies a zero-byte cap; explicitly
unsetting it disables the cap, including in Secure mode. Decimal digit separators
are accepted, so `max-attribute-value-size=4_096` sets a 4096-byte cap. Relative
`leveloffset` assignments are resolved before the cap applies.

## Include targets

Include targets may contain internal spaces. Leading or trailing ASCII whitespace
still makes the line invalid as an include directive, so we leave it for ordinary
document parsing.

Local targets with spaces resolve normally below Secure mode. Targets beginning with
an ASCII URI scheme are classified before local paths, so unsupported schemes never
fall through to filesystem access. A scheme must contain at least two characters,
preserving Asciidoctor's Windows drive-path disambiguation. Scheme names are
case-insensitive. In Secure mode, and for URI targets without caller-supplied URI
authority, we preserve the complete target in the same link fallback as Asciidoctor.
For authorized HTTP(S) reads, the target follows `ureq`'s URI rules: a raw space takes
the located unreadable-URI recovery path. Authorized targets with other schemes
produce the same unresolved-URI recovery without a transport attempt.

## Partial includes

The `lines`, `tag`, and `tags` attributes select content from the original target
before that content is processed. An include or conditional outside the selected
lines does not run and does not produce a warning.

If a directive has more than one kind of selector, `lines` takes precedence over
`tag`, and `tag` takes precedence over `tags`. For repeated attributes of the same
kind, the last value wins. Line selections are sorted and deduplicated, and a
negative range end means the end of the file.

Tag selection supports nested tags, repeated tag names, `*` and `**`, and negated
selectors. Missing tags and malformed selected tag boundaries produce located
warnings and parsing continues.

## Include depth

Built-in includes have a trusted `max-include-depth` attribute that defaults to `64`
and is visible as `{max-include-depth}` in the parsed document. The fallback
participates in attribute lookup, membership, effective iteration, substitution, and
conditionals. On the borrowed value, `as_integer()` returns its numeric value, `text()` returns the
original spelling of an explicit value, and `write_text()` writes either that spelling
or the formatted default. Use `is_explicit()` to test whether an assignment exists,
including an unset assignment. The default is not serialized unless the caller supplied
a value. Set it through the parser options when a different limit is needed:

```rust
let options = acdc_parser::Options::builder()
    .with_attribute("max-include-depth", "8")
    .build()?;
# Ok::<(), acdc_parser::Error>(())
```

The entry document does not count toward the limit; each currently open included file
counts as one level. A value of `0` disables built-in include processing and leaves
each directive as literal content with a located source-recovery warning.
Boolean `true` also selects zero; use a decimal string for a numeric limit.
A string value can have
surrounding Unicode whitespace, but the complete trimmed value must be a non-negative
ASCII decimal integer. Malformed, empty, decimal-fraction, and negative values return
an `InvalidDocumentAttribute` configuration error when options are built. The original
spelling of a valid value remains visible as the document attribute, and very large
positive values saturate safely without overflow. At a positive limit, the blocked
directive is preserved, a located diagnostic is added to `ParseResult::warnings()`,
and parsing continues. Only an exact `include::` directive is processed; block macro
names that merely begin with `include` remain ordinary content without include
diagnostics. Declarations in document content are consumed but cannot change or unset
the trusted value and do not appear as document-attribute events in the AST.

An include can set `depth=N` to limit nesting below its target. The target itself
is read; `depth=0` preserves its nested directives with warnings. This limit
cannot raise the active global or parent include limit. `depth=` accepts a leading
signed decimal value, matching Asciidoctor, while `max-include-depth` retains the
complete nonnegative integer validation described above.

## Include indentation

The `indent` attribute on an include accepts a non-negative integer from `0` through
`4096`. Larger, malformed, and negative values produce a located warning and
ignore the override, preserving the existing indentation. The cap is checked before
allocating a prefix, so one directive cannot request an unbounded number of spaces
for every non-empty included line.

The `4096` cap is an intentional acdc security policy, not a requirement of the
AsciiDoc language. Asciidoctor coerces the value using Ruby's `to_i` and does not impose a
documented or implemented maximum before allocating the indentation prefix.

## Table limits

Tables accept at most 100 logical columns and 1,000 rows, bounding an accepted table
to 100,000 materialized cells. The column bound also applies to `cols` multipliers,
cell duplication counts, and column spans; row spans are bounded by the row limit.
Larger requests and oversized CSV, TSV, DSV, or PSV dimensions return a located
parse error before unbounded expansion. Numeric column widths must be between
0 and 4,294,967,295; larger values produce a located warning and use proportional
width 1 while retaining the other column controls. Asciidoctor accepts larger
integer widths.

These are fixed internal safety limits. They cannot be changed through parser options
or document attributes. This intentionally diverges from asciidoctor, which has no
equivalent table dimension cap.

## Include loading

`SafeMode` sets processing restrictions.
`Options::include_loader` selects where include content comes from.
The default is `IncludeLoader::System` for string, reader, and file input.
The default safe mode is Secure, so the parser does not read include targets.
Select a lower mode to enable include reads.

- `IncludeLoader::Disabled` preserves literal include directives in modes below
  Secure without reading their targets. Skipped includes produce source recovery diagnostics.
  `parse_file` still reads the entry file selected by the caller.
- `IncludeLoader::System` reads local files.
  With `network`, it also reads permitted HTTP(S) targets outside bare WebAssembly.
- `IncludeLoader::custom(provider)` reads sources through an
  `IncludeSourceProvider`, such as unsaved editor buffers or a virtual filesystem.

```rust
use acdc_parser::{IncludeLoader, Options, SafeMode, parse};

let options = Options::builder()
    .with_safe_mode(SafeMode::Server)
    .with_include_loader(IncludeLoader::Disabled)
    .build()?;
let document = parse("include::part.adoc[]", &options)?;
assert!(document.source_recovery().is_some());
# Ok::<(), acdc_parser::Error>(())
```

Providers receive resolved paths or absolute HTTP(S) URIs and return byte streams.
The parser applies safe-mode restrictions before it calls a provider.
It also handles attribute substitution, decoding, line/tag selection, nested
includes, diagnostics, and source locations.
Custom URI providers do not require the `network` feature, but still require
caller-supplied `allow-uri-read`. Secure mode never calls a provider and always
produces the usual link fallback, including when the loader is `Disabled`.

The explicit `Disabled` option differs from Asciidoctor's Secure-mode link fallback.
At equivalent safe modes, `System` retains the existing Asciidoctor include
behavior. Both APIs default to Secure; both conversion CLIs default to Unsafe.

## Include base directory

When include loading is enabled, string and reader input resolve relative includes
from the current working directory. File input uses the entry file's parent directory.
`Options::builder().with_base_dir(path)` overrides the include base for all input types.
`parse_file` still reads the entry file from the path that the caller supplies.

The effective base resolves includes in the entry input. Once a file is included,
nested relative includes resolve from the directory containing that file. In `Safe`
and `Server` modes the effective base also remains the local-include boundary.

## Local include confinement

In `Safe` and `Server` modes, the effective include base directory is the local
include boundary. For file input it defaults to the entry document's directory; for
string and reader input it defaults to the current working directory.

For example, assume the entry document is `/workspace/docs/main.adoc`, so the
boundary is `/workspace/docs`:

| Directive location | Include target | Path opened | Result |
| --- | --- | --- | --- |
| `/workspace/docs/main.adoc` | `chapters/intro.adoc` | `/workspace/docs/chapters/intro.adoc` | No warning |
| `/workspace/docs/main.adoc` | `../shared.adoc` | `/workspace/docs/shared.adoc` | The `..` that would leave the boundary is discarded, and a warning is emitted |
| `/workspace/docs/main.adoc` | `/workspace/docs/appendix.adoc` | `/workspace/docs/appendix.adoc` | No warning because the absolute target is already inside the boundary |
| `/workspace/docs/main.adoc` | `/tmp/shared.adoc` | `/workspace/docs/tmp/shared.adoc` | The outside absolute path is moved beneath the boundary, and a warning is emitted |
| `/workspace/docs/chapters/part.adoc` | `../../shared.adoc` | `/workspace/docs/shared.adoc` | The first `..` reaches the boundary, the second is discarded, and a warning is emitted |

Nested includes resolve from the directory of the file that contains the directive,
but continue to use `/workspace/docs` as their boundary. With `opts=optional`, the
target is transformed first, the recovery warning is retained, and a missing
transformed file is then skipped without a missing-file warning.

`Unsafe` mode does not apply these transformations: from
`/workspace/docs/main.adoc`, `../shared.adoc` attempts to read
`/workspace/shared.adoc`, and `/tmp/shared.adoc` remains `/tmp/shared.adoc`.

The boundary checks the path as written but does not resolve symlinks. If
`/workspace/docs/linked.adoc` points to `/private/secret.adoc`, including
`linked.adoc` reads `/private/secret.adoc` without a boundary warning. These
transformations match asciidoctor; they are not strict symlink containment.

## Remote includes

The parser checks for a URI before it treats a target as a local path.
URI reads need a safe mode below `Secure` and the caller's `allow-uri-read` attribute.
A document cannot grant itself permission to read a URI.

`IncludeLoader::System` supports HTTP(S) and needs the `network` feature.
Custom providers can supply permitted HTTP(S) sources without that feature.

On bare WebAssembly targets such as `wasm32-unknown-unknown`, `network` does not provide an HTTP client.
If the system loader attempts an HTTP(S) include, the parser reports a warning at the directive and leaves it unresolved.
Fetch the content in the host application before parsing, then supply it through a custom `IncludeSourceProvider`.
The same permissions and safe modes apply.

`IncludeLoader::System` uses `ureq` for HTTP framing, redirects, TLS, and timeouts.
Asciidoctor uses Ruby's OpenURI.
We do not try to match all OpenURI behavior.

If a source cannot open or a read fails before selection completes, the parser issues a warning at the include directive.
It keeps the unresolved directive, discards partial text, and continues parsing.
Encoding errors in required text and provider errors marked `Fatal` stop parsing.

In Asciidoctor mode, `compat-mode` makes denied URI includes omit the
`include` role from their fallback link. Draft keeps the role.

## Include source size limit

Each include can select up to 10 MiB of normalized UTF-8 text, including newlines.
The parser checks this limit before and after `indent=`, before it processes nested directives.
If the selected text exceeds the limit, parsing fails with `Error::IncludeSourceTooLarge`.
A document attribute cannot change the limit.
The limit does not apply to the entry document.

Use `lines=`, `tag=`, or `tags=` to select a small part of a larger source.
For example, `include::large.log[lines=100..120]` selects lines 100 to 120.
If these lines fit within the limit, the source file can be larger than 10 MiB.
The parser decodes the source in chunks and discards skipped lines.

A finite `lines=` selection stops at its highest requested line, as Asciidoctor does for local files.
Encoding errors or read failures after that line do not affect the result.
Full includes, open-ended ranges such as `lines=100..-1`, and tag selections read to the end of the source.
A tag can occur again later in the source.
The text limit does not limit scan time.

All includes in one parse share a budget, with a default of 64 MiB.
Each include counts its selected UTF-8 text after `indent=`, plus one byte for a separating newline if the text is not empty.
The parser counts this text before it processes nested directives or removes comments and inactive conditionals.
Repeated includes count each time. Nested includes use the same budget; their expanded output is not counted again in the parent.
The entry document does not count toward this budget.
If an include exceeds the remaining budget, parsing fails with `Error::IncludeExpansionTooLarge` at that directive.
Set `OptionsBuilder::with_max_total_include_bytes(bytes)` to change the budget.
For example, `.with_max_total_include_bytes(256 * 1024 * 1024)` permits 256 MiB for trusted input.
Zero permits only empty include selections. Read the limit with `Options::max_total_include_bytes()`.
Document attributes cannot change this setting, and it does not change safe-mode permissions or the 10 MiB per-include limit.
The CLI exposes this setting as `--max-total-include-bytes BYTES` on `convert`, `lint`, `inspect`, and `execute`.
The LSP uses the default 64 MiB budget.

Line selections remove trailing whitespace from each line independently.
This includes Unicode whitespace such as U+2003, which Asciidoctor preserves.
Later text cannot change the selected text or its source locations.
A selected blank final line counts the same whether more lines follow or not.
Earlier versions normalized the whole source, which made this count depend on later lines.

The built-in HTTP(S) loader has a separate 10 MiB limit for bytes read after decompression.
This includes bytes read ahead of the selection.
When a finite line selection is complete, the loader closes the response without reading or checking the rest.
Thus, a small selection near the start of a larger response can succeed.
A selection near the end can exceed the transfer limit before the reader reaches it.
Custom providers control their own transport limits, but the parser still limits their selected text.

Tag names, active tag state, and diagnostics also have fixed 10 MiB metadata limits.
These limits prevent unbounded tag data when the reader scans a large source.
If tag data exceeds a limit, parsing fails with `Error::IncludeSelectionTooComplex`.

These resource limits deliberately differ from Asciidoctor.

## Include encodings

acdc decodes include bytes before it selects lines or tags.
A byte order mark (BOM) identifies UTF-8, UTF-16LE, or UTF-16BE.
Explicit UTF-16LE and UTF-16BE labels also work without a BOM.
The parser accepts WHATWG single-byte encoding labels and aliases through `encoding_rs` and converts their text to UTF-8.
If an encoding label is unknown, acdc uses BOM detection or UTF-8, as Asciidoctor does.

Invalid text in a local or remote include produces a located recovery warning.
The parser discards partial include content, keeps an unresolved marker, and continues.
This also applies to UTF-8 without transcoding and to `opts=optional` includes.
Invalid text in the entry document remains fatal. Asciidoctor can abort conversion
for include decoding failures.

A finite `lines=` selection ignores invalid bytes after its last requested line, even in the same read buffer.
acdc keeps the same decoder across reads, so BOM detection also works with `lines=` and `tag=`.
Asciidoctor 2.0.26 uses a separate line reader for selections.
Its UTF-16 behavior differs when only a BOM specifies the encoding.

## Parser integration suites

Related tests are grouped in `tests/<suite>/`, with one automatically discovered
target per suite: `attributes`, `blocks`, `inlines`, `substitutions`, `references`,
`preprocessor`, `read_api`, `editing`, and `asg`. Each original test file remains
a module, so cases are independently selectable through nextest filters:

```console
cargo nextest run -p acdc-parser --all-features --test substitutions -E 'test(/^typography_arrows::/)'
```

Add new modules to the appropriate suite's `main.rs`. Allocation measurements,
tracing tests, fixture support tests, and custom harnesses remain separate targets
to preserve their isolation. The fixture runner's case names are unchanged.

## Parser fixtures

We use two fixture styles, and they are intentionally different.

### `fixtures/tests`

These are general parser and AST fixtures. The runner in `tests/fixtures.rs`
discovers `.adoc` files recursively, excluding `includes` folders. Each case has
one shared `.json` expectation, or a pair named `.asciidoctor.json` and
`.draft.json`. An expected parse failure uses `.error` in place of `.json`.
Missing partners, mixed shared/paired files, and conflicting outcomes are errors.

Optional `.diagnostics.json` companions record warnings, source recovery, and
locations. Paired cases use `.asciidoctor.diagnostics.json` and
`.draft.diagnostics.json` together. A failed parse has no returned diagnostics,
so its companion contains `null`. Diagnostics do not change the AST JSON.

Test names use the relative case name and expectation variant, such as
`inline_image::asciidoctor` and `inline_image::draft`. Every case runs in both
modes, including cases with a shared expectation. The runner and generator
share discovery, parser options, and serialization.

After approval to update expected output, regenerate only the affected fixtures
from the workspace root and review the diff:

```console
cargo run -p acdc-parser --example generate_parser_fixtures --all-features -- example.adoc
```

The generator accepts exact paths relative to `fixtures/tests`. Add
`--diagnostics` to record diagnostics. Use `--pair` with exact filenames to
generate both mode expectations, replacing a shared expectation when present.
Use `--mode asciidoctor` or `--mode draft` to update one existing partner.
Adding paired diagnostics requires `--pair`, so both companions are created.
A shared expectation can be generated only when both modes agree. A missing
build feature or unexpected parse outcome fails a scoped request before any
expectations are written.

With no filenames, the generator attempts every fixture available in this build.
Shared expectations and single-mode updates need separate requests.

### `fixtures/preprocessor`

These are focused scenarios for includes, conditionals, encoding, warnings, and
source mapping. They are not discovered automatically and usually do not have a
JSON companion. A Rust test must open a root fixture directly; any supporting
fixtures are then reached through ordinary `include::` directives.

For example:

```text
Rust test
  └─ main.adoc
       └─ include::target.adoc[]
            └─ include::inner.adoc[]
```

Adding a file to `fixtures/preprocessor` does not add test coverage by itself. Either
reference it from a Rust test or include it from a root fixture that a test already
opens. Use this style when the assertion needs more than a serialized AST, such as
exact warnings, missing-file behavior, or file, line, column, and include-chain
attribution.

Run `acdc lint` over new or changed AsciiDoc fixtures before handing them off.

## Deliberate divergences from asciidoctor

acdc uses the [AsciiDoc Language draft specification](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/)
and [Asciidoctor](https://asciidoctor.org) as references. Some behavior deliberately
differs. These choices do not imply complete draft-specification conformance.

* **Nested inline markup**: Formatting, links, footnotes, and index labels retain
  a nested AST. acdc keeps complete labels where Asciidoctor's substitution order
  can truncate them or produce crossed markup. Formatting boundaries use source
  characters before output escaping. Balanced parentheses inside visible index
  shorthand remain part of the label, even when replacement substitutions are off.
* **Attribute values**: Supported `pass:` values retain AsciiDoc text, not generated
  HTML or roff. Output-dependent substitution lists stay literal with a warning.
  Escaped attribute names remain protected from formatting. Unused values and
  discarded titles do not register footnotes, anchors, or index terms.
* **Quoted conditions**: `ifeval` respects operators inside quoted strings and
  removes both quote delimiters. Asciidoctor 2.0.26 can split at an operator inside
  a quoted value or retain its closing quote.
* **Callouts**: Enabled callouts are recognized without special-character
  substitution. Nested callout lists keep their outer validation context.
* **References**: Link IDs enter the reference catalog. Anchors retain labels in
  contexts where Asciidoctor can fall back to `[id]`; valid IDs can also include
  non-ASCII symbols. In Asciidoctor mode, title-based references with
  `compat-mode` remain unresolved and warn. Draft resolves their natural targets. Cross-references never trigger file reads.
* **Recovery warnings**: Disabled includes and ignored `subs=` settings report
  source recovery. Conflicting named footnotes warn and keep the first body.
  Nested bibliography and index sections remain in the AST with warnings, while
  Asciidoctor reports their structural restriction at error severity.
* **Protected attributes**: Document entries cannot change read-only or API-only
  attributes, including derived names that Asciidoctor does not consistently lock.
  An explicitly selected backend name remains visible even if it is not a known
  Asciidoctor backend; selecting an output converter is the application's job.
* **Unicode boundaries**: Formatting and passthrough boundaries use Rust's Unicode
  tables. Characters added in Unicode 17 can differ on Ruby versions with newer
  tables.

* **Uppercase checklist marker**: acdc treats `[X]` as checked, alongside `[x]`
  and `[*]`. Asciidoctor leaves `[X]` in the item's text. This acceptance
  difference is an intentional acdc extension.

* **Include source limit**: Each include can select up to 10 MiB of UTF-8 text.
  All includes in one parse also share a configurable budget after indentation, with a default of 64 MiB.
  The built-in HTTP(S) loader also limits bytes read after decompression to 10 MiB.
  See [Include source size limit](#include-source-size-limit).
* **Remote include transport**: We use `ureq` and don't try to reproduce the URI
  transport behavior that Asciidoctor's Ruby implementation inherits from OpenURI.
  URI classification uses the portable ASCII scheme syntax and Asciidoctor's
  two-character Windows-path carve-out. MRI Asciidoctor also accepts non-ASCII
  scheme-like prefixes as a side effect of Ruby's Unicode-aware regular expressions;
  acdc treats those prefixes as paths. Only HTTP(S) is fetched; FTP and other ASCII
  schemes take URI recovery and never local-file handling.
  If a request or read fails before selection completes, the parser discards partial text and keeps the unresolved directive.
  See [Remote includes](#remote-includes).
* **Include encoding labels and selection**: acdc uses the WHATWG-oriented
  `encoding_rs` label set rather than Ruby's exact aliases.
  The parser decodes bytes in chunks. Finite line selections ignore encoding errors after the last requested line.
  See [Include encodings](#include-encodings).
* **Include indentation limit**: As an intentional acdc security policy,
  `include::file[indent=N]` accepts at most `4096` spaces instead of allowing an
  unbounded allocation. AsciiDoc does not require this cap, and asciidoctor does not
  impose it. See [Include indentation](#include-indentation).
* **Table dimension limits**: Tables accept at most 100 logical columns and 1,000
  rows, with fixed internal limits that cannot be raised by parser options or
  document attributes. See [Table limits](#table-limits).
* **Boolean include-depth value**: Boolean `true` as `max-include-depth` selects
  zero and disables includes with a warning. Asciidoctor raises an error for this
  input. Use a decimal string for a numeric limit; see
  [Include depth](#include-depth).
* **Strict include-depth validation**: acdc trims surrounding Unicode whitespace and
  requires the complete `max-include-depth` string to be a non-negative ASCII decimal
  integer. Malformed, empty, fractional, and negative values return a structured
  configuration error. Asciidoctor 2.0.26 instead accepts a
  leading signed decimal prefix, so a value such as `8notes` silently becomes `8`.
  The strict rule follows the documented [integer (≥ 0) domain](https://docs.asciidoctor.org/asciidoc/latest/attributes/document-attributes-ref/#security-attributes)
  and is an intentional compatibility divergence. See [Include depth](#include-depth).
* **Symmetric escape of constrained markers**: `\*foo\*`, `\_foo\_`, `` \`foo\` ``, `\#foo\#` all emit the literal marker pair (`*foo*`, `_foo_`, etc.). asciidoctor strips only the opening backslash and leaves the trailing `\` in the output. The draft spec's backslash-escaping section (`spec/outline.adoc`) states: "a backslash in front of a reserved markup character will be removed, regardless of whether the text would have been interpreted or not" — acdc follows that rule symmetrically.

## See also

- [CHANGELOG](CHANGELOG.md) for detailed feature history and version notes
- [Architecture](../ARCHITECTURE.adoc) for parsing and conversion boundaries
