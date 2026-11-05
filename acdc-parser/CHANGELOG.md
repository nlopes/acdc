# Changelog

All notable changes to `acdc-parser` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- Owned documents can call `rebuild_references_and_toc()` after prepared tree
  edits. It refreshes membership, labels, aliases and section numbers while
  retaining assigned IDs and the first target for duplicate IDs. Missing IDs
  or document-label context return an error without partial changes. Caption,
  footnote, index and author registrations remain separate.

- **Breaking:** reference records expose their canonical destination through
  `ReferenceRef::target()`. Supply it to `Reference::new(target, location)` when
  constructing a record. Reference titles use `Option<ContentAttribute>`;
  `None` means absent and `Some` retains empty content. Replace
  `title_is_present()` with `title().is_some()`. TOC entries keep their own title
  presence and can be checked with `ReferenceRef::matches_toc_entry()`.

- Generated section IDs honor `idprefix` and `idseparator` at each heading,
  including empty values and duplicate-title suffixes. TOC entries and natural
  cross-references use the same IDs, matching Asciidoctor.

- **Breaking:** inline parse results use `content()` for typed reads; the public
  backing-slice getter and macro content-attribute callback are removed. Copy a
  selected node with `InlineNode::from(inline_ref)`, or selected content with
  `ContentAttribute::from(content_ref)`. Serde serialization of `ContentRef`
  preserves the native inline JSON array without copying the tree. ASG export
  remains a separate checked operation.

- **Breaking:** cross-reference and index occurrence fields are private. Read
  through typed handles and use selected `From` copies for edits. Prepared text,
  hierarchy, relationship, metadata and option transfers replace backing slices
  and maps. Checked scalar replacement retains the existing reserved names.
  Xref destination changes keep classification until explicitly set. Index
  edits retain stored registration labels; they do not rebuild the catalog.

- **Breaking:** button, keyboard, and menu fields are private. Read through
  their typed handles and use selected `From` copies for edits. Prepared
  constructors and label, key, menu-item and span transfers preserve source
  lifetimes, empty positions and existing allocations. Edits do not parse
  labels, split separators or update document registrations.

- **Breaking:** link, URL, email, and autolink storage is private. Read with typed
  handles and copy selected values with `From`. Use prepared label, tooltip,
  metadata, target, display-setting, and option edits; scalar maps use checked
  replacement. Mail subjects/bodies retain absent/empty and unencoded values.
  Edits do not reparse content, fetch targets, or update document catalogs.

- **Breaking:** inline images and icons now use typed reads and selected `From`
  copies. Replace field writes with prepared content, metadata, target, and
  location edits. Use checked `set_attributes` for scalar maps, and explicit
  option transfers instead of backing slices or mutable vectors. Reserved-key
  failures leave the node unchanged; alt and tooltips retain absent/empty values.

- **Breaking:** anchors, footnotes, and callout markers use private storage.
  Read through their typed handles and copy selected values with `From`.
  Use prepared content, identity, number, and location edits for owned values.
  Citation text remains separate from bibliography display labels; editing
  either does not register notes or update document catalogs.

- **Breaking:** formatting span fields are private. The eight concrete types
  remain distinct. Read through their typed `ast` handles, copy selected spans
  with `From`, and use prepared content, role, ID, form and location edits.
  Empty role presence still distinguishes generic highlights from marked text;
  edits do not register anchors or notes. Native JSON and rendering are unchanged.

- **Breaking:** raw text and passthrough macro fields are private. Read with
  `RawRef`/`PassRef`, copy selected values with `From`, and use constructors or
  prepared text, substitution, metadata and location edits. Transfers retain
  source lifetimes, optional-value presence, substitution order and duplicates.
  Edits do not parse text or replay earlier attribute values. Native JSON and
  rendering are unchanged.

- **Breaking:** text, symbol, line-break, apostrophe, and formula fields are
  private. Read through typed `ast` handles and copy selected values with `From`.
  Use constructors and prepared text, notation, escape, or location edits to
  change owned values. Text replacement preserves the escape decision; changing
  formula notation does not translate its text. `StemContent` provides scalar
  `text()` and `notation()` reads. Native JSON and rendering are unchanged.

- **Breaking:** `ContentAttribute` and `Title` no longer provide borrowed slices,
  `Deref`, `AsRef`, or borrowed owner iterators. Read attributes with
  `ContentRef::from(&content)` and titles with `title.content()`. Copy selected
  content with `ContentAttribute::from(content_ref)` or `Title::from(content_ref)`;
  `into_inlines()` still transfers prepared nodes. Empty title copies remain
  present; use `Title::default()` for absence. Native JSON and rendering are unchanged.

- **Breaking:** reference and TOC record fields are private. Use typed catalog
  reads, selected copies and prepared content transfers for edits. Changes keep
  stored numbers and classification snapshots; they do not rename map keys or
  update the document tree. Replace `Block::metadata()` with typed attribute
  reads or `Block::clone_metadata()` for an independent metadata-only copy.

- **Breaking:** table, row, cell and column-format fields are private. Read through
  typed handles and use selected copies and prepared row/cell/content transfers
  to edit owned values. Copies retain cell scopes and resolved decoration; edits
  do not rebuild spans or reinterpret cell content. `TableRef::delimited_location`
  retains the span that starts at the table delimiter, separate from the full
  block span, which includes attributes and title.

- **Breaking:** list-item fields are private. Use typed item handles for reads,
  selected item copies for independent edits, and prepared text, block and anchor
  transfers to change owned items. Marker, nesting and checklist edits do not
  reparse content or rebuild document registrations.

- **Breaking:** the four list-container types have private fields. Read through
  typed list handles and use owned metadata, title and item edit methods.
  Item transfers retain source lifetimes, nesting, checklist state and stored
  callout numbers. Changing a list marker does not rewrite its items; edits do
  not rebuild document registrations.

- **Breaking:** discrete headings, page/thematic breaks, TOC placements, comments,
  and document-attribute event fields are private. Read through their typed `ast`
  handles. Boundary constructors and owned edits preserve empty titles, literal
  comment text and anchor order. Accepted attribute names and assignments remain
  parse decisions; reparse source to change them. `set_location` changes only a span.

- **Breaking:** `DelimitedBlock` and `Admonition` fields are private. Read through
  typed block handles and edit prepared values with owned methods. Delimited
  body transfers retain source lifetimes; edits keep the original source snapshot
  and form. Removing metadata or replacing a table body refreshes decoration
  against that table's source-position defaults. Edits do not rebuild catalogs.

- **Breaking:** block `Image`, `Audio`, and `Video` fields are private. Use their
  `ast` readers and owned edit methods. `replace_source` transfers the complete
  previous target without splitting playlists. Image alt content stays separate
  from its caption; audio and video retain scalar `alt` extension attributes.

- **Breaking:** `Section` fields are private. Read through `ast::SectionRef` and
  use owned setters and `take_*` methods to edit prepared content. Metadata edits
  classify special sections from their style and preserve assigned IDs unless the
  selected authored ID changes. Edits do not rebuild document catalogs.

- **Breaking:** `Paragraph` fields are private. Read through `ast::ParagraphRef`
  and use the owned `set_*`, `take_*`, and `push_inline` methods to edit it.
  Invalid metadata is rejected without changing the paragraph. Content edits
  keep the original source snapshot and do not rebuild document registrations.

- Copy an individual header or author with `Header::from(HeaderRef)` or
  `Author::from(AuthorRef)`. These copies preserve prepared values and source
  lifetimes without copying the document body or replaying registrations.

- **Breaking:** `Header` and `Author` fields are private. Read through `ast::HeaderRef`
  and `ast::AuthorRef`; use their owned setters to edit prepared values. Header
  `take_*` methods transfer metadata, titles, and authors with their source
  lifetimes. Invalid header metadata is rejected without changing the header;
  author edits do not recompute document attributes.

- **Breaking:** `Document` storage fields are private. Read through
  `ast::DocumentRef::from(&document)` and edit with `set_header`, `set_attributes`,
  `set_blocks`, `set_footnotes`, `set_toc_entries`, `set_references`, and
  `set_location`; use `push_block` to append. The `take_*` methods transfer parts while
  preserving source lifetimes. These edits do not repair registered snapshots.

- Clarified detached editing: changes do not rebuild registered targets, TOC
  membership, footnotes, or author-derived attributes. Explicit renumbering has
  a limited scope; reparse changed source for complete registration updates.

- **Breaking:** `ParseResult::document()` now returns `ast::DocumentRef`.
  Read fields through typed methods such as `blocks()` and `attributes()`.
  The temporary `document_ref()` method is removed. For independent edits,
  use `Document::from(parsed.document())`; source text still borrows from the
  parse result. Native JSON and checked ASG output are unchanged.

- Copy a typed document or block for independent edits with `Document::from` or
  `Block::from`. Copies preserve registered catalogs, source locations, captions,
  and table presentation. Text can still borrow from the parse result.

- Library callers can read retained paragraph source text through the typed API, before inline substitutions. Constructed paragraphs have no retained source.

- Use `ast::ReferencesRef::from(&catalog)` to read a caller-constructed reference catalog through the typed API without copying its entries.

- Serialize `ast::DocumentRef` as native AST JSON with Serde. Checked ASG document export now accepts the same typed document: pass `parsed.document()` or `DocumentRef::from(&document)` for a constructed tree. JSON shapes, schema profiles, and validation rules are unchanged.

- Library callers can read table delimiter spelling and opening/closing source ranges through typed table reads, including nested tables and selected includes.

- Preserve IDs and roles on single-, double-, and triple-plus passthroughs in Asciidoctor mode. Protected text stays escaped; raw content stays raw. Draft keeps these prefixes literal until its span syntax is defined.

- Explained how semantic regression checks complement ASG validation in both compatibility modes.

- Documented a proposed upstream ASG representation for formatted attributes and images, with a migration path from acdc's caption format. Existing export formats are unchanged.

- The acdc ASG profile preserves inline images and block-image alternative content, including empty alt/tooltips, dimensions, links, identity, roles/options and source locations. Images can occur inside supported titles, captions and other content attributes. Use `asg::inlines_with_profile` for standalone content; strict upstream export still rejects unsupported image semantics.

- `asg::document_with_profile(document, asg::Profile::Acdc)` preserves resolved captions, attribution and citation content under the versioned `urn:acdc:asg:1` extension schema. Empty values and formatted content remain structured. The existing export functions still use the pinned upstream subset and reject unsupported properties.

- Checked ASG export preserves supported block forms, literal bodies, TOC/discrete headings, breaks and media targets, including complete video playlists. Missing source forms and unsupported semantic properties fail explicitly. Strict upstream export rejects parsed captions and image alternative content rather than flattening them into scalar attributes.

- Checked ASG export supports ordered, unordered, callout and description lists, including formatted terms, attached blocks and supported identities/metadata. Callout markers retain resolved numbering and automatic form. Checklists, unrecoverable item state and unsupported nested content fail with item/property paths.

- Checked ASG export supports links, URL macros, mail links, autolinks and default cross-references. Formatted labels stay structured; automatic link labels, URI scheme hiding and encoded mail subjects/bodies retain their meaning. Unsupported reference controls and ambiguous target classification fail with property paths.

- Checked ASG export omits ordinary delimited comments, like line comments, while rejecting comment titles or semantic metadata. Unsupported section settings and unresolved content report their exact properties. Protected document-attribute errors do not invent declaration coordinates.

- Checked ASG export identifies unsupported owners and known source coordinates in its errors, with exact property paths for span IDs/roles and missing header titles. Scalar attributes cannot shadow dedicated block metadata. Schema-validation failures use the separate `asg::Error::Projection` variant; match error fields with `..` to allow diagnostic context.

- In Draft mode, character-reference names resolved by an earlier passthrough replacement stage stay intact through later formatting and macro labels. XML names containing underscores no longer become emphasis. Enclosing formatting, includes, and table cells retain their meaning.

- Intrinsic attributes such as `{quot}` and `{apos}` produce semantic character references. A later `specialchars` stage now escapes intrinsic values, matching Asciidoctor; replacement attributes still protect formatting and macro syntax.

- Checked ASG export rejects sections without a title and verbatim content without a supported literal/listing mapping. Present empty titles remain distinct from absent titles.

- Added `InlineNode::CharacterReference`, with named/numeric values and `CharacterReferenceRef` reads. Native JSON uses `name: "charref"`; numeric values use canonical decimal reference spelling. Checked ASG export accepts reference nodes and rejects invalid XML numeric characters. Parsing now identifies references during replacements. Asciidoctor mode follows its reference syntax; Draft accepts XML names and one-digit numeric references. Escapes, substitution order, expanded attributes, and source ranges retain their meaning. Numeric references no longer retain hexadecimal spelling or zero padding.

- Added the offline `asg_schema_audit` example with a pinned upstream schema, validator controls, and a node/property inventory. It reports native JSON shape differences separately from ASG export or language conformance.

- Removed the obsolete `pre-spec-subs` and `setext` Cargo features. Remove them from dependency feature lists. Language behavior follows the compatibility mode, and block `subs=` works in every build.

- Table `cols` settings now use complete column specifications, matching Asciidoctor: malformed records are ignored, count shorthand and semicolon separators work, and literal quotes cannot select widths or styles. If no valid records remain, column inference stays enabled. The existing 100-column limit still applies.

- Author/email and manpage name/purpose fields keep literal quotes. Quoted document `leveloffset` values no longer shift headings or included sections; quoted include arguments still work. The string-input `filetype` override derives its flag from the same literal value.

- Cross-reference caption labels, section signifiers and style controls retain literal quote characters from their stored values. Quoted style names use the basic fallback, matching Asciidoctor.

- Text attribute reads retain literal quotes, empty strings, and Unicode. Hyperlink IDs keep the same value in the reference catalog instead of being dequoted again.

- `parse_inline` and `Parser::parse_inline` now resolve attribute references, replacement characters, and passthroughs like document content in the selected compatibility mode. Empty fragments are accepted; surrounding whitespace is preserved. Inline passthroughs return resolved nodes or raw content with remaining substitutions instead of unprocessed `Pass` macros.

- Block image alt text now contains resolved replacement symbols in Asciidoctor mode. Draft alt text stays literal. JSON consumers must accept symbol nodes in alternative text.

### Breaking changes

- `asg::inlines` and `asg::inlines_with_profile` now accept `ast::ContentRef`.
  Pass `parsed.content()` for standalone inline parses, or pass title, alt, and
  other content reads directly. For caller-built content, use
  `ContentRef::from(&content_attribute)`. Exported JSON and validation are unchanged.

- `Raw` now has optional `metadata` for passthrough IDs and roles. Set it to `None` when constructing undecorated raw text. Native JSON includes the metadata only when present.

- Parsed references now use `InlineNode::CharacterReference` / `InlineKind::CharacterReference` instead of plain or raw text. Native JSON readers must accept `charref` nodes; numeric references use decimal spelling.

- Removed `strip_quotes`. Read parsed attribute values directly: attribute-list delimiters are already removed, and remaining quote characters are literal text.

- Native JSON now identifies bibliography anchors with `bibliography: true` and
  retains an explicitly supplied `bibliography_label` as parsed inline content,
  including `[]` for an empty label. An omitted label uses the entry ID. Citation
  `xreflabel` remains separate because its substitutions can differ from the
  displayed entry label. Ordinary anchor JSON is unchanged.

- Native JSON now writes presence attributes, including `toc`, as `true` and
  validated numeric attributes as JSON numbers. Numeric-looking text remains a
  string, and integer text reads retain their original spelling. Header values
  remain a snapshot and accepted body assignments stay in source order, with
  `false` for unsets. Raw inline nodes retain nonempty ordered `substitutions`;
  an omitted list means none. Serializing block metadata with stale reserved
  scalar keys now returns an error. Use dedicated content fields instead.

- Serializing an `InlineMacro` or its concrete owner now produces the same native
  JSON node as serializing its enclosing `InlineNode`, instead of enum wrappers
  or owner-specific layouts. Direct `LineBreak` serialization now uses the same
  `break` node too. Angle-bracketed autolinks retain `bracketed: true`; its absence
  means `false`. Native JSON also accepts standalone passthrough nodes, retaining
  their text and requested substitutions independently of strict ASG export.

- Enabled replacements now resolve copyright and trademark signs, ellipses,
  word-internal apostrophes, and em dashes in the parser. Inline trees
  represent this typography with `InlineNode::Symbol` and
  `ast::InlineKind::Symbol`. Read `SymbolRef::kind()` or `text()` for its meaning
  and `location()` for its authored span. Native JSON and text reads expose the
  resolved value. Escaped tokens stay literal, and attributes expanded after
  replacements no longer introduce new symbols or word context, matching
  Asciidoctor. Empty attribute references do not take the source spans of
  following symbols. Dash locations include their consumed whitespace. Earlier callout comment
  guards stay literal during later replacements. Raw passthroughs resolve tokens
  formed across earlier attribute expansions; later expansions cannot create
  tokens, word context, or replacement escapes. Interior `left--right` and `left -- right` dashes also
  resolve in the parser, including their consumed whitespace and source spans.
  Source arrows (`->`, `=>`, `<-`, `<=`) also resolve in ordinary text and
  passthroughs when special-character substitution runs before replacements.
  Reversing that order keeps source arrows literal, matching Asciidoctor.
  Verbatim-only listing, literal, and source profiles now expose resolved
  typography too. Reversed escaping keeps source arrows literal. As with
  passthroughs, resolved signs retain their Unicode meaning when escaping runs
  later, rather than Asciidoctor’s generated entity spelling.
  Apostrophes and complete word-bounded dash pairs also use neighboring text
  across ordinary anchors and index labels. Formatting stays a boundary, and
  index catalog labels keep their registration-time values. Visible index labels
  now expose enabled signs and ellipses consistently to every converter.
  Encoded arrows such as `pass:r[-&gt;]` also resolve before conversion, including
  their escapes and shared delimiters. Earlier escaping protects authored
  references; later escaping retains the semantic arrow rather than Asciidoctor
  HTML’s generated entity spelling.
  Protected passthrough text now supplies neighboring word context in the
  parser too, so `Sam'pass:[s]` exposes a resolved apostrophe. A passthrough's
  own replacement settings stay independent of the surrounding text.
  Dash pairs and consumed spaces split across ordinary text, anchors, and index
  labels now resolve in the parser. Symbols belong to the first dash's owner;
  their source ranges cover only that owner's consumed text. Catalog labels and
  earlier replacements stay intact.

- Draft mode keeps block image alt text literal, including single-quoted values,
  and does not register macros from it. Attribute references still expand.
  Native JSON now uses protected literal nodes for this text. Asciidoctor mode
  retains formatted alt from preceding single-quoted attributes; inline image
  and icon alt/tooltips remain rich content in both modes.

- Legacy underlined headings now follow `CompatibilityMode` in every build:
  Asciidoctor recognizes them by default; Draft uses ATX headings. The `setext`
  Cargo feature is a compatibility alias. Remove `Options::setext` and
  `OptionsBuilder::with_setext()` from callers and select the mode instead.

- Block metadata builders reject attributes that shadow dedicated fields,
  including aliases and fields that are absent. `BlockMetadata::with_attributes`
  and block `with_metadata` builders now return `Result`; propagate the error
  with `?` and set content, identity, roles, and options through their fields.

- Formatting IDs now retain their authored source ranges, including expanded
  references, nested content, includes, and table cells. When constructing
  formatting nodes, use `ElementId::new(value, location)` instead of a string
  for `id`. Typed `id()` reads and the scalar native JSON `id` stay unchanged.

- Serializing an index term directly now produces the same native JSON as its
  inline node. Read labels from `term`/`secondary`/`tertiary` and visibility from
  `visible`, rather than the former `kind` object. Nested catalog labels keep
  their existing JSON form.

- Cross-references retain IDs, options, and extension attributes. Use
  `xref.metadata.roles` instead of `xref.role`, and `attributes()` with
  `set_attribute()` / `remove_attribute()` for scalar controls. Typed reads keep
  targets, explicit labels, and automatic-label policy distinct. Native JSON
  retains metadata, local-target classification, and effective nondefault
  `xrefstyle`. `InlineMacro::CrossReference` now holds `Box<CrossReference>`.
  As with other acdc inline macros, `roles` aliases `role`; Asciidoctor recognizes
  only `role` on xrefs.

- Read link, URL, and mailto metadata through their typed handles. Use prepared
  metadata and option transfers for edits, and checked `set_attribute()` /
  `remove_attribute()` for scalar controls. `Link::with_attributes()` rejects
  dedicated metadata names. Native JSON groups metadata consistently and retains
  unencoded mail subject/body values, including empty values. Typed reads expose
  labels, destinations, tooltips, and display policy.

- Icons retain IDs with source locations, roles, and options through the same
  metadata API as inline images. Replace `icon.attributes` with `icon.attributes()`
  for scalar controls; read identity and roles through `icon.metadata` and options
  through `icon.options()`. Use `set_attribute()` / `remove_attribute()` for scalar
  edits. Native JSON groups these values under `metadata`, while alt and tooltip
  remain separate content. `InlineMacro::Icon` now holds `Box<Icon>`.

- Inline images expose identity and roles through `InlineMetadata`, with
  `ElementId` preserving the ID's source location. The owning image's
  `attributes()` map contains scalar controls such as width and link; read alt,
  tooltip, identity, roles, and options separately. Use `set_attribute()` /
  `remove_attribute()` for scalar controls and `options()` / `options_mut()`
  for macro options. Reserved names and aliases return `ReservedAttribute`
  instead of shadowing a dedicated value.

- Reference labels now expose parsed content. Read named `reftext=` through
  `BlockMetadata::reftext` instead of `metadata.attributes["reftext"]`.
  `Anchor::xreflabel`, `TocEntry::xreflabel`, and `Reference::xreflabel` use
  `Option<ContentAttribute>`; native JSON stores inline arrays.

- The parser AST now has the same substitution fields in every build.
  `SubstitutionSpec`, `SubstitutionOp`, and `BlockMetadata::substitutions` no
  longer require `pre-spec-subs`; remove feature guards used only to access
  them. Absent settings stay absent in JSON, and explicit empty settings stay
  `[]`. Block `subs=` settings now work in both compatibility modes and without
  default features. `pre-spec-subs` is a compatibility alias in the parser.
- `InlineMacro::Image` holds `InlineImage`; `Image` represents block images.
  Read `alt` and inline `tooltip` as optional `ContentAttribute` values, including
  explicitly empty content. Icons and links also expose tooltips. Button labels,
  menu labels, and keyboard keys now contain inline content. Native JSON uses
  inline arrays and omits consumed metadata attributes. `InlineMacro::Link`, `Url`, and `Mailto`
  hold boxed structs to keep ordinary inline nodes compact.
- Authored `caption=` content uses `Caption::CustomContent`; read it through
  `BlockMetadata::caption_content()`. Generated labels and counters remain
  separate from serialized content.
- `Video::source` replaces `Video::sources`, including in JSON and `Video::new`.
  It preserves commas and empty playlist slots. Read the provider from `poster`
  instead of synthetic boolean `youtube`/`vimeo` attributes.
- `Title::default()` is absent; `Title::new(Vec::new())` is explicitly empty.
  Use `is_present()` for caption presence and `is_empty()` for inline content.
  Empty titles serialize as `[]` without a synthetic empty text node.
- Named titles, quote attributions, and citation titles serialize only their
  parsed inline content. Read named titles through `title`, rather than
  `metadata.attributes["title"]`. Replacing or removing content cannot leave
  an old attribute value. `Attribution` and `CiteTitle` now share
  `ContentAttribute`; their constructors and slice access remain.

### Added

- With `asg` enabled, export supported documents and inline content to a pinned
  ASG draft, preserving empty titles, unsets, and positional values. Unsupported
  meaning returns a typed error; native JSON is unchanged.

- Applications can parse, display, and serialize `CompatibilityMode` using the
  canonical names `asciidoctor` and `draft`. Unknown names are rejected.

- Formatted document attributes now honor the declared order of quotes,
  replacements, macros, and attributes. `pass:q,a[{word}]` keeps formatting
  introduced by `{word}` literal, while `pass:a,q[{word}]` parses it, matching
  Asciidoctor. Aliases retain definition-time values and unresolved references;
  expanding an empty value after formatting preserves the empty span.
  Explicit lists run once; Asciidoctor may apply macros and replacements again
  when the value is inserted into normal prose.

- Inline `pass:` lists now preserve unsupported source and report
  `ContentRecovery`, matching formatted attribute values. This covers unknown
  substitution names, callouts, and escaping after formatting or macros.
  Asciidoctor may instead discard unknown names or display generated markup.
  Escaped, protected, and macro-disabled source remains literal without warnings.

- `CompatibilityMode::Draft` resolves attribute references before ordinary
  inline formatting, so inserted values such as `*Bold*` create bold content.
  Asciidoctor mode retains formatting before attribute expansion. Explicit
  substitution lists retain their requested order; incremental block modifiers
  start from the selected mode's defaults.

- Clarified the library's attribute reuse paths: `Options::with_document_attributes`
  retains parsed substitution behavior, while `DocumentAttributes::into_inputs`
  exports text for fresh configuration, including numeric spelling and unsets.

- Library consumers can compare source endpoints by line, column, and include
  chain without copying their source information.
- List item iterators can be cloned to retain independent positions without
  copying their items or allocating a collection.
- Library consumers can copy a video from a typed read, retaining its source,
  title, attributes, and source locations for independent edits.
- Library consumers can identify a simple admonition's paragraph while retaining
  its title, ID, and source span.
- Typed element attributes expose their recorded block metadata span for source-aware
  diagnostics. Inline attributes and blocks without a retained span return no span.

- Library callers can copy document attributes and registered TOC/reference
  entries from typed handles with `From`. Copies preserve defaults, explicit
  unsets, assignment provenance, assigned numbers, and source locations without
  revalidation or catalog updates. Source text keeps its existing lifetime.
- Library callers can read the highest assigned caption number through typed
  document handles. Nested blocks and table cells are included; reads preserve
  stored numbers and leave edited documents and reference catalogs unchanged.

- Library callers can resolve section and discrete heading IDs through typed
  handles, including title-derived fallbacks for caller-built headings. Existing
  parsed IDs and retained identity reads stay unchanged.

- Library callers can read assigned section numbers, including book parts and
  appendices, through typed section handles. Reads keep the stored numbers
  without adding signifiers or renumbering edited sections.

- Library callers can resolve table borders and row stripes from typed element
  attributes and a document snapshot or a lookup of values active at the table's
  position. Local overrides, empty values, and document defaults keep their
  existing meaning; stored table presentations stay unchanged.

- Table column-format iterators can be cloned to inspect the same formats again
  without collecting them.

- Typed source spans can be compared directly, including their byte offsets,
  endpoint positions, and include chains.

- Library callers can classify caption categories from typed blocks without
  inspecting the owner enums. Classification follows the current block form and
  style; resolved labels and ordinals retain their stored values.

- `CaptionRef::resolve` reads stored labels and ordinals, or borrows a caption
  fallback from element and document attributes without assigning an ordinal.
  Existing metadata-based caption resolution remains supported.

- Block attributes can be read through `BlockRef::attributes()` without matching
  each block kind. Line comments and document attribute assignments return `None`.

- Library callers can resolve typed block substitution settings against their
  own defaults. Groups, modifier order, and empty settings keep their behavior.

- Formatting span reads expose complete role text, including whitespace and
  explicit empty values, alongside role-token queries.

- Typed media targets expose their filename without reparsing URLs, retaining
  parsed URL path semantics and non-UTF-8 path handling.

- Standalone inline parses expose typed content through `ParseInlineResult::content()`
  without copying nodes or changing their source locations. Existing slice reads
  and native JSON remain unchanged.

- Library callers can read registered cross-reference targets and TOC entries
  through `DocumentRef::references()` and `toc_entries()`, without copying their
  content. Reads preserve labels, empty values, section numbers, resolved
  captions, and catalog snapshots without changing registrations or native JSON.

- Inline content iterators support independent cloned cursors, reverse traversal,
  and exact remaining lengths without copying the content.

- Library callers can copy a typed source span into an owned `Location` for
  indexes that outlive the parsed document, preserving include provenance.

- Library callers can walk owned inline content through
  `DocumentRef::content_occurrences()` or read a node's direct `content_fields()`.
  Each field identifies its purpose and typed owner, including titles, credits,
  captions, alt/tooltips, labels, and list content. Callers can skip nested fields;
  derived reference, footnote, TOC, and index copies are excluded.

- `ParseResult::document()` provides typed reads of documents, headers, authors,
  sections, paragraphs, delimited blocks, admonitions, lists, tables, media,
  discrete headings, TOC blocks, breaks, and comments, with lazy child and inline
  iterators. Read nested bodies, formula text and notation, source delimiters,
  and quote/caption content without rendering. Raw and verbatim text
  remain distinct from formatted content. List items separate principal text
  from attached blocks and preserve terms, markers, checklist states, anchors,
  and callout numbers. Read table declarations separately
  from resolved presentation, and preserve absent versus
  empty image alt and tooltips. Match block and inline kinds without depending on
  collection storage. Their `attributes()` reads expose titles, image content,
  IDs, styles, role/option tokens, and scalar controls without copying values.
  Lookup and iteration preserve missing, empty, false, and valueless attributes;
  `roles` and `options` aliases share their canonical entries. Existing owner
  APIs and JSON output remain unchanged.
  Document attribute reads preserve the header snapshot, explicit unsets, and
  validated values with retained text. Body assignments and table-cell initial
  assignments remain separate ordered sequences, with existing parser policy.
  Video targets retain commas and empty playlist slots. Breaks expose only their
  retained metadata, and comment reads preserve literal text and comment form.
  Formatting spans expose nested content, constrained/unconstrained form, IDs,
  and ordered role tokens through their concrete types. Unadorned marks remain
  distinct from attributed generic spans.
  Index entries expose hierarchy levels, see/see-also targets, and catalog
  labels captured before later substitutions.
  Footnote reads separate occurrence content and shared numbers from registered
  definitions. Bibliography anchors expose entry labels separately from citation
  text, with their original source locations.
  Button, keyboard, and menu reads preserve formatted labels and ordered key/item
  sequences. Inline formulas expose protected text and resolved notation without
  rendering or further substitutions.
  Text reads retain escape protection, ordered passthrough substitutions, and
  verbatim whitespace. Standalone inline parsing exposes retained passthrough
  content separately from the text produced by document parsing.
  Hyperlinks share destination, label, and location reads while retaining their
  concrete forms, display settings, metadata, and unencoded mail subject/body.
  Block attribute reads also expose attribution, citation, authored captions,
  named reference text, and retained `subs` settings, including explicit emptiness
  and unsupported entries. Generated captions remain separate.
  Structural traversal visits nested blocks, list items, table rows, and cells
  in a documented order, with early stopping and no allocation for direct children
  or flat walks. Nested walks use temporary memory proportional to nesting.
  Block titles and primary IDs are available without matching each block kind.
  Document and section ID reads now agree with reference resolution, including
  generated section IDs. Retained anchors and inline element IDs remain separate
  from registered targets; reads preserve their locations without assuming an
  editable source spelling.

- Parser fixtures support shared expectations or explicit Asciidoctor/Draft
  pairs, optional diagnostics, and stable test names. The runner rejects missing
  or conflicting expectations. The generator shares its parsing rules and fails
  before writing if a selected case has an unexpected outcome. Every case runs
  in both modes; a shared expectation requires identical results.

- Select `CompatibilityMode::Draft` through `OptionsBuilder::with_compatibility_mode()`
  for the supported draft rules, including attribute expansion before ordinary
  inline formatting and literal unquoted/double-quoted named titles. Asciidoctor
  mode is the default, with documented fixes and deliberate extensions. Both
  modes use the same AST in every build. Includes and nested content retain the
  caller's mode, while safe mode and resource controls stay independent. The
  parser READMEs describe supported rules, recovery, and migration from the
  released API without claiming complete draft-specification conformance.

- `OptionsBuilder::with_force_local_file_xrefs()` resolves file-qualified
  references by their local anchors in combined or partially included documents.
  It preserves URLs and custom text and requires a nonempty fragment. This
  opt-in override can redirect genuine external file references. It does not
  scope duplicate IDs, resolve Antora resource IDs, or change page-only
  references. It deliberately differs from Asciidoctor for files that are not
  fully included.

- Explicit `toc2` settings now produce one deprecation warning per parse.
  Use `:toc: left` or supply `toc=left` through the API or CLI. TOC behavior is
  unchanged. Unsets, ignored assignments, and the `toc2` CSS class do not warn.
  Asciidoctor accepts the deprecated alias without this warning.

### Fixed

- Formatted named block titles, single-quoted quote credits, and custom captions now resolve typography at both text edges, matching asciidoctor. Literal values and nested inline boundaries stay unchanged.

- Recognize page-break lines at end of file, with or without a final newline,
  including CRLF input. Runs of three or more `<` characters match Asciidoctor.
  Page-break source ranges end on the marker instead of the next character.

- Bibliography anchors now accept empty and whitespace-only labels, including
  labels in includes and AsciiDoc table cells. Supplied ASCII spaces follow
  Asciidoctor's leading-space rules and remain visible in citations. Empty labels
  stay present as `[]` in native JSON. HTML keeps a complete `[]` display envelope
  instead of Asciidoctor's unmatched bracket for a zero-character label.

- Visible bibliography labels resolve typography with the surrounding block's
  substitution profile and retain authored symbol locations.

- Inline `[[id,label]]` anchors accept single closing brackets inside their
  labels, matching Asciidoctor. Nested macro syntax stays in the reference label
  instead of rendering or registering as body content.

- Reference labels resolve copyright and trademark signs, punctuation, and
  escaped tokens in the parser. Asciidoctor-mode labels keep macros literal;
  Draft's unquoted and double-quoted named `reftext` values remain literal.
  Render resolved label nodes without applying replacements again.

- Index catalog labels now expose resolved typography and passthrough policies
  in both modes. Catalog punctuation uses the label's own boundaries and
  registration-time substitutions, independently of the displayed prose.
  Consumers should render these nodes without applying replacements again.

- Passthrough blocks and `[pass]` paragraphs now honor explicit `subs=` settings
  in both compatibility modes and feature configurations. Inline formatting,
  replacements, and macros follow the requested order; default and `subs=none`
  content stays raw.

- Each unresolved footnote now reports its authored source range and marks the
  document as recovered. Repeated references report separate warnings, including
  inside links, passthroughs, selected titles, includes, and table cells. Their
  existing `[id]` fallback text is unchanged.

- Block image, audio, video, and TOC macros with text after their closing bracket
  stay paragraph text instead of discarding the trailing content, matching
  Asciidoctor. Whitespace after a complete macro remains valid.

- Escaped counter references stay literal instead of being removed. Unsupported
  active counters still warn and omit their value, and now set
  `ParseResult::source_recovery()` so callers can detect the loss. Match
  `WarningKind::UnsupportedCounter` to identify these warnings.
- Inline-preprocessor warning ranges end at the last authored character instead
  of the following character, including passthrough, STEM, and counter warnings.

- Inline passthroughs finish replacements before later attributes or macros.
  In `pass:r,a[...]`, a later value cannot introduce signs, dashes, or apostrophe
  context for the earlier stage. In `pass:r,m[footnote:n[(C)\]]`, the footnote
  captures the resolved copyright sign; reversing the stages keeps it literal.
  Edge punctuation still uses acdc's documented parsed word context.

- Typography around empty passthroughs is available in the parsed tree, with
  each symbol located only in the source text that owns it. Rendered behavior
  follows acdc's existing parsed-context policy in both modes; Asciidoctor keeps
  more passthrough boundaries.

- Warning byte offsets now refer to the original file after includes and
  preprocessing edits, including selected and nested includes. Cross-file
  diagnostics retain each endpoint's source context. Reindented includes keep
  merged byte offsets; use their original line and column instead.

- Quoted menu sequences such as `"File > Open"` now work with `experimental`
  in Asciidoctor mode and by default in Draft mode. Rich labels, escapes,
  named entities, and explicit `subs=` settings follow the existing UI rules.

- Menu items recognize `&gt;` separators. With `subs=macros`, a raw `>` stays
  inside the label, matching Asciidoctor. Labels after entities keep their
  authored source locations.

- Unsupported inline STEM substitution lists now preserve their complete source
  and report a warning, instead of formatting or registering inner content.
  Asciidoctor supports inline lists; acdc supports block attribute expansion only.

- Keyboard sequences expand ordinary attributes before choosing their keys.
  Explicit `subs="macros,attributes,..."` freezes keyboard and menu boundaries
  before those attributes expand. Keyboard passthroughs stay inside one macro.

- Keyboard macros recognize plus/comma keys such as `kbd:[Ctrl++]` and
  `kbd:[Ctrl,,]`, choose one separator per sequence, and preserve nested labels.
  Menus accept comma-separated items and empty final items. Keyboard, menu,
  and button labels keep escaped closing brackets inside the enclosing macro.
  These forms follow Asciidoctor where it recognizes them.

- Cross-reference shorthand removes only leading ASCII whitespace from its
  label, retaining trailing and nonbreaking spaces like Asciidoctor. Label
  locations point to the displayed text after the skipped prefix.
- Image alt content introduced by an attribute reference now points to that
  reference, including selected includes and table cells. In Asciidoctor mode,
  preceding single-quoted alt values parse introduced formatting and resolve
  nested references using the attributes available when declared, matching
  Asciidoctor.

- A description-list term and its marker must be on the same line. Ordinary
  wrapped paragraphs that contain a later `Term::` line stay paragraphs in both
  modes, matching Asciidoctor. This also applies after a description list and
  inside includes and table cells.

- Unordered and ordered list markers no longer split ordinary paragraphs.
  Attached paragraphs follow the selected mode: Asciidoctor accepts sibling and ancestor items from the
  same list family; Draft keeps those lines as text until a paragraph boundary.
  Standalone `+` lines still interrupt paragraphs in both modes. Metadata before
  a nested list keeps it attached to the current item.

- A list continuation keeps the next block attached when line or block
  comments precede it, including comments from selected includes. Comment-only
  continuations still retain their source. This matches Asciidoctor in both modes.

- Legacy heading underlines must differ from their title by at most one
  character, matching Asciidoctor. Metadata and punctuation-only lines no
  longer become accidental underlined headings.

- Indented paragraphs honor `normal`, quote, verse, example, sidebar, and
  admonition styles instead of falling back to literal text. `normal` removes
  common indentation; explicit verbatim styles preserve it. Draft also accepts
  `paragraph` as an alias for `normal`; Asciidoctor mode keeps it literal.
  Implicit literal paragraphs now remove the full common indentation.

- Open blocks honor example, admonition, quote, verse, sidebar, and passthrough
  styles like Asciidoctor. Verbatim, raw, and comment styles preserve slash
  lines and keep the selected content behavior across includes in both modes.

- Delimited listing and literal blocks honor `[literal]`, `[listing]`, and
  `[source,language]` styles like Asciidoctor, including source language and
  explicit `subs=` settings. Original delimiters and source locations stay intact.

- Draft mode recognizes lists, block macros, admonitions, and paragraph boundaries
  before applying `source`, `listing`, `literal`, or `verse` styles. Metadata,
  delimiters, and headings can end these paragraphs. Asciidoctor mode keeps its
  style-dependent boundaries and code-comment preservation.
- A block that ends in an included file keeps that file's actual end line, even
  when it is lower than the start line in the parent file.

- Positional image alt text expands document attributes once, matching named
  macro alt text and Asciidoctor. Literal references retained by an alias stay
  literal instead of resolving to a later body value, including in table cells
  and selected includes.

- Replacing delimited-block metadata with `with_metadata` or `set_metadata` keeps
  table `frame`, `grid`, and `stripes` in sync with rendered decoration. Removing
  an override restores the document defaults active at that table, including
  defaults set in the body or an AsciiDoc cell.

- Custom caption content in the reference catalog retains its original source
  locations through includes, including selected regions and table cells.

- Inline image IDs now point to their authored values after earlier blocks,
  formatting, attribute expansion, includes, and AsciiDoc table cells. Nonempty
  ID ranges exclude the closing quote or bracket.

- Named index macros keep controls out of visible labels and hierarchy levels,
  matching Asciidoctor. Discarded controls no longer register nested footnotes.
  IDs, roles, options, and accepted extensions are retained in native JSON;
  typed attributes expose hierarchy and the selected see/see-also relationship.
  As with other acdc inline macros, `roles` aliases `role`.

- Reference labels retain their authored locations through attribute expansion,
  escaping, includes, and nested blocks. Block anchors use attribute values from
  their declaration point. Asciidoctor mode keeps restricted label formatting;
  Draft treats unquoted and double-quoted named labels as literal text and
  interprets single-quoted content. Empty labels still select title fallback.

- Asciidoctor mode now keeps `[discrete]` and `[float]` headings discrete when a
  later attribute list supplies `style=normal`. Named `style=discrete` alone
  remains a section, matching Asciidoctor. Draft uses the last selected style.
  Section nesting and legacy-float warnings follow the selected heading style.

- Positional block attributes now use the matched block form. Quote delimiters
  accept positional attribution without a `quote` style, and `[,language]` on a
  listing selects that source language. Unrelated block forms no longer acquire
  source or quote attributes from their style alone.
- Draft resolves mapped positional and named attributes in document order,
  including preceding image/video positions. Asciidoctor mode keeps the
  reference's block precedence and limits media dimensions/posters to the macro.
  Later positional macro dimensions now override earlier named values.
- Markdown fence languages no longer shift preceding positions or turn them
  into line-number settings. Only selected attributions register inline macros.

- Stacked block attributes retain earlier positional values across named gaps.
  Draft also retains them across empty positions; Asciidoctor mode clears an
  explicitly empty position. Values keep the location of the selected entry.
- Asciidoctor mode now replaces earlier roles when a later list supplies `role=`,
  including an empty value, and adds same-list shorthand roles after named roles.
  Block media macros retain options from preceding attribute lists. Option
  splitting follows the reference comma/space rules in Asciidoctor mode.
- Ignored duplicate caption attributes no longer replace the selected caption
  or suppress its footnotes when the list contains an attribute reference.
- Draft mode keeps quoted and malformed shorthand literal and supports combined
  anchor/style shorthand. A hash ID overrides an anchor-prefix ID. Block macros
  continue to treat shorthand punctuation as text.
- Draft treats unquoted named `None` as text, including inline-image attributes.
  Asciidoctor mode still ignores it without erasing an earlier value. Quoted and
  positional `None` remain text in both modes.
- Shorthand IDs retain their authored ranges after attribute expansion and no
  longer include the following delimiter in their source location.
- Draft mode uses odd/even backslash rules for attribute references and quotes in
  block lists, and for brackets in block anchor labels. Other backslashes stay
  literal. Asciidoctor mode keeps its compatible escape rules and now recognizes
  block anchor labels containing right brackets, including escaped brackets.
- Raw block attribute locations exclude closing quotes and following commas. Expanded
  values point to their authored references, including Unicode, selected includes,
  and nested table cells.

- Draft mode rejects outer spaces and tabs in block attribute lists and requires
  a comma, space, tab, or list end after a closing quote. Invalid quote enclosures
  stay literal. Block macros use the same rules; inline macro lists are unchanged.
  Both modes now preserve leading-space block attribute lines as content, matching
  Asciidoctor. Includes and source-paragraph comment handling use the selected mode.

- Supported block `subs=` settings no longer produce a specification warning.
  Unknown stages report source recovery and remain in the substitution metadata.
  STEM blocks warn for stages other than attributes and output escaping, while
  preserving formula text and applying the supported stages. Unlike Asciidoctor,
  acdc does not insert formatting tags into formulas.

- Paragraph admonition styles such as `[NOTE,caption='Care']` retain their label and admonition layout.

- Image and icon alternative content, tooltips, custom captions, and UI labels
  retain supported inline formatting, escapes, substitutions, and source locations.
  Only selected values register footnotes. Literal quote attributions and citation
  titles retain dashes; single-quoted values support normal substitutions even
  when the body uses `subs=none`. Named and positional quote values retain their
  declaration-time attributes. Asciidoctor mode expands introduced references;
  Draft leaves them literal unless explicit passthrough substitutions request it.
  Explicitly empty attribution and citation content serializes as `[]`.
- Single-quoted `caption=` prefixes parse formatting introduced by attribute
  expansion and retain the declaration's attribute values. Asciidoctor mode
  expands introduced references; Draft keeps them literal unless explicitly
  requested. Literal and media-macro captions stay literal, and caption source
  locations cover the authored references and escapes.
- Icon positional values always select size; named alternative text remains
  independent. Repeated icon attributes use the last value and retain link options.

- Named titles parse formatting introduced by attribute expansion, such as
  `[title='{word}']` with `:word: *Bold*`. Asciidoctor mode also expands references
  introduced by that first expansion; Draft does not do so implicitly and parses
  only single-quoted titles. Both modes retain declaration-time values, even
  where Asciidoctor uses a later value for an introduced reference.
- Inline source spans cover complete attribute references and escaped characters.
  Single-character content no longer includes a following delimiter in its byte range.

- Named `title=` attributes now supply formatted titles on paragraphs, lists,
  delimited blocks, tables, audio, and video. Media macro titles override preceding
  titles. Repeated named attributes use the last value; repeated `opts` accumulate.
- Images honor named `alt` over positional alt text and retain literal quotes
  and empty positional slots. Inline `title=` remains a tooltip and no longer supplies alt text.
  Video `poster=` overrides its positional value, including platform names.

- Block images accept `title=` captions in the macro or a preceding attribute
  list. Macro titles override preceding titles, retain inline formatting, and
  supply cross-reference text, matching Asciidoctor. Inline-image `title=`
  tooltips remain independent of caption titles.

- Errors and warnings in reindented includes report columns in the original
  source, including nested and selected includes.

- STEM notation now uses AsciiMath when `:stem:` is absent, empty, or unknown.
  The `latex` and `tex` aliases select LaTeX. Explicit `[latexmath]` and
  `[asciimath]` blocks and positional notation override the document value,
  matching Asciidoctor.
- Inline STEM keeps attribute references as formula text by default.
  LaTeX removes one enclosing dollar pair.
  `[stem,subs="+attributes"]` expands block attributes while retaining the
  original source text and locations.

## [0.11.0] - 2026-10-10

### Breaking changes

- The parser API now defaults to `SafeMode::Secure`, matching the Asciidoctor API.
  Includes become links without reading their targets. Select Safe or Unsafe
  to enable includes. `parse_file` still reads the entry document.
  Custom providers do not change the safe mode.

### Added

- `OptionsBuilder::with_include_loader()` selects custom include sources or
  disables loading with `IncludeLoader::Disabled`. Custom providers can supply
  unsaved editor buffers, virtual files, or authorized HTTP(S) content. Safe modes,
  caller attribute policy, and source recovery also apply to custom providers.

### Changed

- Clarified the shared inline escape, Unicode boundary, and balanced index-label
  rules, including the intentional differences from Asciidoctor in both modes.

- Documented shared block extensions in both compatibility modes: nestable tilde
  open blocks, longer backtick fences, and uppercase checklist markers. The
  `paragraph` style on indented text remains specific to Draft mode.

- Draft mode recognizes keyboard, button, and menu macros without
  `experimental`. This is an acdc choice inspired by the unfinished draft
  outline. Asciidoctor mode still requires explicit activation.
- Draft preserves `compat-mode` and warns that it has no legacy language
  effects. Reference labels, natural targets, and URI include fallback roles
  follow Draft rules. Asciidoctor mode keeps the supported legacy behavior.
  Neither attribute can change the caller's selected compatibility mode.

- `[pass]` paragraphs retain raw content without interpreting inline macros.
  Simple admonitions own their title and attributes once; native JSON marks
  their source form with `form: "paragraph"`. Use `Admonition::from_paragraph`
  to construct that form and `AdmonitionRef::simple_paragraph()` to read it.
  `Admonition::new` constructs a compound block.

- All includes in one parse now share a budget, with a default of 64 MiB. Each include counts
  its selected UTF-8 text after `indent=`, plus a separating newline for nonempty
  text. Nested and repeated includes use the same budget. Exceeding it stops
  parsing with `Error::IncludeExpansionTooLarge` at the include directive.
  Set `OptionsBuilder::with_max_total_include_bytes()` to change the budget in
  bytes. Document attributes cannot change it. The entry document does not count
  toward this budget; safe modes and the per-include limit still apply.
  This limit deliberately differs from Asciidoctor.
- Each include can select up to 10 MiB of normalized UTF-8 text, before and after `indent=`.
  Use `lines=`, `tag=`, or `tags=` to select a small part of a larger source.
  Larger selections fail with `Error::IncludeSourceTooLarge`.
  Excess tag metadata causes `Error::IncludeSelectionTooComplex`.
  The built-in HTTP(S) loader also limits bytes read after decompression to 10 MiB.
  These fixed limits differ from Asciidoctor and do not apply to the entry document.
- Finite `lines=` selections stop at the highest requested line, as Asciidoctor does for local files.
  Errors after that line do not affect the result, and HTTP responses close early.
  Each line loses its trailing whitespace independently, so later text cannot change the selected text or its source locations.
  This includes Unicode whitespace, which Asciidoctor preserves.
  Selected blank final lines count the same whether more lines follow or not.
  Full includes, open-ended ranges, and tag selections still read to the end of the source.

### Fixed

- Parser tracing no longer dumps document text, options, AST nodes, paths, or URLs.
  Warning logs contain categories and positions. Read full warnings from the
  parse result and full errors from the returned `Error`.
- The parser now builds for `wasm32-unknown-unknown` with `network` or `--all-features`.
  This target has no built-in HTTP client.
  Attempts to load HTTP(S) includes produce a warning and keep the unresolved directive.
  To use remote includes, fetch content before parsing and supply it through a custom `IncludeSourceProvider`.

## [0.10.0] - 2026-10-08

### Breaking changes

- The minimum supported Rust version is now 1.88, up from 1.85.
- Attribute configuration now uses iterators. `OptionsBuilder::build()` and
  `Options::with_attributes()` return `Result`; add `?` to handle invalid values.
  Read `Options::document_attributes()` instead of its former public field.
- `DocumentAttributes` is read-only. Its `get()` and `iter()` methods borrow
  `DocumentAttributeValue`, with `as_str()`, `as_integer()`, `is_presence()`, and
  `text()` for reads. Configure values through `Options::builder()` instead of
  `insert()`, `set()`, `remove()`, or `merge()`; `get_string()` is removed.
- `Document::attributes` contains the end-of-header values. Later changes are
  ordered `Block::DocumentAttribute` events. Read each event with `assignment()`
  instead of the removed `value` field, and handle both set and unset assignments.
- `Position::line` and `column` are `u32`. Use `Position::new()` or
  `Position::from_line_col()` to construct positions. Each boundary can carry its
  include chain. `SourceLocation::location` replaces `positioning`, and
  `Positioning` is removed. The three `Location::shift*()` methods are removed.
- `TocEntry::kind` replaces `numbered` and `style`. Use `SectionKind` for special
  sections and `TocEntry::number()` for the assigned number.
- Index terms contain inline nodes instead of strings. Update matches on
  `IndexTermKind` and calls to `term()`, `secondary()`, and `tertiary()`.
  `InlineMacro::IndexTerm` is boxed. JSON index labels are now inline-node arrays.
- `SubstitutionSpec`, `SubstitutionOp`, and `BlockMetadata::substitutions` require
  the default-on `pre-spec-subs` feature. Parsed lists use the new
  `SubstitutionSpec::Source` variant and retain their entries in JSON.
  Replace `macros_disabled()` and `attributes_disabled()` with
  `BlockMetadata::uses_substitution()` or `SubstitutionSpec::resolve()`.
- `inlines_to_string()` is removed. Applications that need plain text must choose
  how to handle formatting and macros when walking inline nodes.

See the [migration guide](README.md#migrating-from-09) for examples.

### Added

- A reference catalog for sections, blocks, inline anchors, and bibliography
  entries. Title-based cross-references and references to fully included files
  resolve within the document without loading more files.
- Section kinds and numbers, block captions, table presentation, and `xrefstyle`
  metadata for custom converters. Sections and captions can be renumbered after
  editing the document.
- Index `see` and `see also` relationships, `anchor:id[text]` macros, and
  positional `mailto:` subject and body values.
- Block metadata access, retained positional attributes, and block source text
  before inline substitutions. Source locations resolve to the original included
  files, including partial includes.
- `ParseResult::source_recovery()` reports content that was omitted or recovered,
  even after `take_warnings()`. Duplicate IDs and conflicting named footnotes
  report both source locations; the first definition wins.
- `OptionsBuilder::with_base_dir()` controls include resolution.
  `skip-front-matter` removes YAML front matter and exposes it as an attribute.
  `SOURCE_DATE_EPOCH` gives reproducible UTC timestamps.

### Changed

- Application attributes take precedence over document entries. Use
  `with_defaults()` for values the document can replace. Read-only and API-only
  attributes are protected; `outdir` and `outfile` are unavailable during parsing.
  Literal `true` and `false` values remain text; use `:name!:` to unset a value.
- Substitutions follow their written order. Normal content processes quotes before
  attributes. `subs="quotes,+attributes"` replaces block defaults;
  `subs="+quotes,attributes"` keeps them. Keyboard, button, and menu macros require
  the `experimental` attribute.
- Includes default to a depth limit of 64. Set `max-include-depth` through parser
  options using a non-negative decimal string. Invalid values fail configuration;
  `0` leaves directives literal and reports source recovery.
- Secure mode does not read includes. Remote reads require the `network` feature
  and caller-supplied `allow-uri-read`; a document cannot grant this permission.
  Remote content cannot include local files. Safe and Server modes keep local
  paths beneath the include base, but do not resolve symlinks.
- Fixed limits protect against excessive allocation: 10 MiB per decoded remote
  include, 4,096 include-indent spaces, and 100 columns by 1,000 rows per table.
  These limits intentionally differ from Asciidoctor and cannot be raised.
- Parsing uses less memory for attributes and repeated cross-references, and is
  faster for large verbatim blocks.

### Fixed

- Include selection, nested tags, conditionals, encodings, and source locations.
  Missing or unreadable includes retain an unresolved directive and a warning;
  optional missing files are skipped. Disabled includes report source recovery.
- Attribute precedence, header author and revision parsing, comment handling,
  source-order body attributes, and attribute isolation in AsciiDoc table cells.
- Inline formatting, passthroughs, Unicode boundaries, escaped delimiters, links,
  footnotes, and index terms. Nested labels retain their text and source locations.
- List nesting and continuations, callouts, styled code and verse paragraphs,
  multiline table cells, spans, and repeated cells. Unterminated blocks and
  incomplete tables now retain usable content with located warnings.
- Generated section IDs, reference labels, bibliography entries, captions, and
  section numbering. Repeated headings get unique IDs, and explicit duplicate
  IDs keep the first target.

Most fixes follow Asciidoctor. Deliberate differences, including nested inline
markup, stricter include settings, and recovery warnings, are documented in the
[README](README.md#deliberate-divergences-from-asciidoctor).

## [0.9.0] - 2026-04-26

### Packaging

- Shrunk the published tarball by excluding developer-only files: `AGENTS.md`,
  `README.adoc` (duplicate of `README.md`), `benches/`, `examples/`, `fixtures/`,
  `proptest-regressions/`, and `tests/`.

### Added

- **`preprocess` and `grammar_parse` tracing spans** in all parse entry points (`parse`,
  `parse_file`, `parse_from_reader`) for phase-level timing visibility.
- **First-section level validation** - emit a warning when a titled document's first
  section skips level 1 (e.g. starts with `===` instead of `==`), matching `asciidoctor`'s
  "section title out of sequence" check. Title-less documents still accept any
  first-section level.
- **Unterminated table recovery** - when a table's opening delimiter (`|===`,
  `!===`, `,===`, `:===`) runs to end of input without a matching close, emit
  a `WarningKind::UnterminatedTable { delimiter }` warning carrying the
  literal opening token and still produce a table block from the content
  (matching `asciidoctor`'s "unterminated table block" warning and recovery).
- **`ParseResult` and `ParseInlineResult`** — new return types from `parse_*`. Each
  bundles the AST, source text, and any non-fatal warnings. Access via `.document()` /
  `.inlines()`, `.source()`, `.warnings()`, and `.take_warnings()`. Marked `#[must_use]`
  so warnings aren't silently dropped.
- **`Warning` and `WarningKind`** — non-fatal parser diagnostics are now returned as
  typed values with `.source_location()` and `.advice()` accessors, mirroring `Error`.

### Changed

- **BREAKING**: `parse*()` now returns `Result<ParseResult, Error>` (and
  `Result<ParseInlineResult, Error>` for `parse_inline`) instead of
  `Result<Document, Error>` / `Result<Vec<InlineNode>, Error>`. Use `.document()`
  / `.inlines()` to borrow the AST.
- **Warnings are exposed on `ParseResult::warnings()`** instead of flowing only
  through `tracing`. Every warning — grammar (unknown table format, bad row column
  count, callout gaps, anchor whitespace, first-section level, trailing macro
  content, counter references, experimental `subs=`) and preprocessor (missing
  include files, URL includes blocked by safe-mode, missing `allow-uri-read`,
  `network` feature disabled for remote includes, invalid include line numbers,
  `ifdef`/`endif` attribute mismatches) — now carries a `SourceLocation` with an
  optional file path. `tracing::warn!` emission is kept as a fallback.
- **Long-running consumers no longer leak memory per parse.** Embedders like
  `acdc-lsp` and `acdc-editor-wasm` can parse repeatedly with a flat memory
  footprint.
- **BREAKING**: `Link.text` changed from `Option<&'a str>` to `Vec<InlineNode<'a>>` so
  inline markup inside `link:` macros is preserved as structured nodes. `Link::new`
  initialises it as `Vec::new()`; `Link::with_text` now takes `Vec<InlineNode<'a>>`.
- `Link`, `Url`, and `Mailto` now serialize their text under an `inlines` key on the ASG
  (consistent with `xref`/`footnote`), only when non-empty. Previously the text was not
  serialized at all.

### Fixed

- **Whitespace-only text in `link:`, URL, `mailto:`, and `xref:` macros is
  preserved literally** instead of falling back to the target.
  `link:https://example.com[ ]` now renders as `<a href="https://example.com"> </a>`
  and `xref:section[ ]` renders as `<a href="#section"> </a>`, matching
  asciidoctor; previously acdc dropped the whitespace and emitted the URL or
  section title as the anchor's visible text. The shorthand form `<<section, >>`
  still falls back to the section title, also matching asciidoctor.
- **Inline markup inside `link:` macro text** — `link:url[*bold*]` now parses nested
  formatting (bold, italic, monospace, passthroughs, etc.) through the full inline
  grammar, matching `url:` / `mailto:` behaviour.
- **Diagnostics inside `a`-style table cells now resolve to the offending
  line, not the cell's `a|` style prefix.** The recursive parse over an
  AsciiDoc cell anchored at the cell-token offset rather than the
  content-byte offset, so warnings (e.g. `UnterminatedTable` from a nested
  `!===`) and inline locations were reported one or more lines off and
  one column off. Inner-cell positions are now correct, which also fixes
  off-by-one columns in nested-table fixtures.
- **Trailing text after a completed table row is now a continuation
  paragraph of the last cell**, matching asciidoctor. Previously the text
  was collapsed into the preceding line (top-level tables) or dropped
  entirely (nested tables inside `a`-cells). The non-`a` cell block parser
  also now consumes blank lines between iterations, so multiple
  paragraphs in a cell render as separate `<p class="tableblock">` blocks.

### Removed

- **BREAKING**: `impl FromStr for Source<'static>` is removed. It leaked a boxed
  `String` to `'static` on every call, accumulating to multi-GB RSS in long-running
  LSP sessions. If you need an owned copy, convert to your own type.

### Performance

- **Parsing large documents is dramatically faster.** Hot paths in inline parsing and
  attribute handling skip work entirely when the input has no relevant syntax.
- **Macro-heavy documents parse much faster.** Share `FootnoteTracker` across inline
  sub-parses via `Rc<RefCell<_>>` to avoid quadratic deep-clone cost on every nested
  `process_inlines`.
- **Broad parser speedup (~15–20% on large docs)** from a four-part sweep of the
  `position()` / `LineMap` hot path: a byte-only fast path in
  `section_level_at_line_start` (runs as a negative lookahead on every paragraph
  continuation line), a monotonic last-line cache in `LineMap` (consecutive offsets skip
  the binary search), a signature change on `process_inlines` /
  `preprocess_inline_content` to take a raw `usize` offset instead of a
  `&PositionWithOffset`, and byte-lookahead guards on every alternative of the inline
  macro alternation in `non_plain_text()` to prune speculative dispatch.
- **Macro-heavy documents parse ~14% faster**; prose-heavy documents 3–6% faster.

## [0.8.0] - 2026-03-28

### Added

- **Fragment support in `xref:` and `link:` macros** — targets like `xref:file.adoc#anchor[text]`
  and `link:page.html#section[text]` now parse correctly. The `#fragment` is optional and only
  applies to `xref:` and `link:` macros (not `image::`, `video::`, etc.).
- **Public `InlineNode::location()` method** — provides direct access to the source location of
  any inline node without requiring the `Locateable` trait.
- **`Locateable` trait for `InlineNode`** — `InlineNode` now implements `Locateable`, providing
  direct access to location information without pattern matching on variants.
- **Rich inline markup in document titles and subtitles** — document titles now support bold,
  italic, monospace, links, macros, and other inline markup, matching section title behavior.
  Previously, titles were always rendered as plain text.
- **`subs=macros` substitution type** — `[subs=-macros]` and explicit lists without `macros`
  now gate macro grammar rules at parse time. When macros are disabled, inline macros
  (links, xrefs, images, footnotes, index terms, etc.) are treated as plain text.
  Requires the `pre-spec-subs` feature flag.
- **Include `indent` attribute** — `include::file.rb[indent=2]` now re-indents included content
  to the specified level, matching asciidoctor behavior. Strips existing leading whitespace and
  prepends the specified number of spaces. `indent=0` removes all leading whitespace.
- **`strip_quotes` utility function** — centralised helper to strip matching single or double
  quotes from attribute values, replacing scattered `trim_matches('"')` calls throughout the
  codebase.
- **Single-quoted attribute values** — attribute values can now use single quotes (`'value'`)
  interchangeably with double quotes (`"value"`), matching asciidoctor behavior. Applies to
  block attributes, macro positional/named values, link titles, and table column specs.
- **Compound author names** — author lines now support multi-word first, middle, and
  last names (e.g., `Jan de Groot`), matching asciidoctor behavior.
- Bidirectional sync between `Header.authors` and document attributes: the `:author:`
  document attribute now populates `Header.authors`, and parsed author lines now set
  `author`, `authors`, `firstname`, `lastname`, `middlename`, `authorinitials`, `email`,
  and `authorcount` document attributes

### Performance

- **Inline parsing up to 39% faster** — added character-class pre-filter and lookahead guards
  to `plain_text` and `quotes_plain_text` rules. Characters that cannot start any inline
  construct are now consumed in bulk without running 28+ negative lookahead checks per
  character. Remaining trigger characters use grouped character-class guards to skip
  irrelevant rule evaluations.

### Changed

- **Roles are now space-separated** — `role='a b'` produces two roles (`a`, `b`) instead of
  one, matching asciidoctor's space-separated role semantics.
- **`parse_comma_separated_values` simplified** — no longer handles quote stripping internally
  since quotes are now stripped upstream by `strip_quotes`.

### Fixed

- **`specialcharacters` not recognized as a substitution name** — `[subs="specialcharacters"]`
  now works as an alias for `specialchars`, matching asciidoctor behavior.
- **Stacked block attributes overwrite previous values** — when block attributes are spread
  across multiple lines (e.g., `[source,ruby]` followed by `[subs="+attributes"]`), the second
  line no longer overwrites the style, positional attributes, id, substitutions, attribution,
  or citetitle from the first line. Only explicitly provided values are merged.
- **Incorrect locations for inline text inside `xref:`, `url:`, and `mailto:` macros** — text
  nodes inside these macros (e.g., "Section Title" in `xref:file#id[Section Title]`) had wrong
  line and column numbers. The line was always reported as 1 regardless of actual position, and
  the column was relative to the start of the macro instead of the text content. Both issues are
  now fixed: grammar rules capture the correct content start position, and the location mapper
  remaps nested text nodes to document-absolute coordinates.
- **Roles with spaces were not split** — `image::foo.jpg[role="thumb bordered"]` now correctly
  produces two separate roles (`thumb`, `bordered`) instead of one combined string.
- Boolean/valueless attributes (e.g., `:set-attr:`) now expand to an empty string when
  referenced as `{set-attr}`, matching asciidoctor behavior
- Constrained formatting (bold, monospace, highlight) no longer incorrectly expands
  inside constrained italic `_..._` delimiters, matching asciidoctor behavior. The
  underscore is a word character, so it prevents nested constrained marks at the boundary.
- Backslash escaping of character replacements (`\--`, `\...`, `\->`, `\<-`, `\=>`, `\<=`,
  `\(C)`, `\(R)`, `\(TM)`) now correctly suppresses typography substitutions, matching
  asciidoctor behavior
- Expand attributes inside `pass:a[]` content when macros disabled via `subs=-macros`,
  matching asciidoctor behavior
- Fixed passthrough preprocessor bypassing `subs=-macros` gating — `pass:[]` macros and
  inline passthrough syntax (`+...+`, `++...++`, `+++...+++`) are now treated as literal
  text when macros are disabled, matching asciidoctor behavior
- Fixed non-monotonic inline positions for subscript/superscript text preceded by short plain text

## [0.7.0] - 2026-02-25

### Added

- **`latexmath:[]` and `asciimath:[]` inline macros** — explicit notation overrides that
  set the stem notation directly instead of resolving from the `:stem:` document attribute.
- **`BlockMetadata.location`** — metadata blocks now carry an `Option<Location>` tracking
  their source position (attribute lines, anchors, titles).
- **`DelimitedBlock.open_delimiter_location` / `close_delimiter_location`** — delimited
  blocks now carry precise locations for both the opening and closing delimiter lines.
- **`DescriptionListItem.delimiter_location`** — description list items now carry the
  source location of their delimiter (`::`, `:::`, etc.).

## [0.6.0] - 2026-02-23

### Added

- **Attribution and CiteTitle types** — blockquote attributions and citation titles are now
  stored as `Attribution` and `CiteTitle` types (containing `Vec<InlineNode>`) on `BlockMetadata`,
  instead of plain strings in the attributes map. This enables inline content (links, formatting)
  in attributions. ([#357])
- **Quoted paragraphs produce DelimitedQuote** — the `"text" -- Author` shorthand now produces
  a `DelimitedBlock` with `DelimitedQuote` content, matching the markdown blockquote and
  `[quote]` block output structure.

### Fixed

- **`stem:[]` backslash escaping for brackets** — `stem:[\]` and `stem:[\[]` now correctly
  strip the backslash and preserve the bracket. Previously, `balanced_bracket_content` treated
  `\]` as a closing bracket, breaking expressions like `stem:[[[a,b\],[c,d\]\]((n),(k))]`.
  Stem now uses a dedicated `escaped_bracket_content` rule.
- **Block delimiters in source block content** — lines inside a delimited block containing
  a longer sequence of the same delimiter character (e.g., `--------------------` inside a
  `----` block) are no longer incorrectly treated as closing delimiters. The parser now
  requires an exact match of the opening delimiter length. ([#349])

## [0.5.0] - 2026-02-14

### Added

- **`style` field on `TocEntry`** — TOC entries now carry the section's style (e.g.,
  `"appendix"`, `"bibliography"`), enabling converters to handle special section rendering
  like appendix letter numbering. ([#343])
- **Book doctype level-0 section (parts) support** — documents with `:doctype: book` now
  accept level 0 sections (parts) in both ATX (`= Part Title`) and setext styles. Added
  `doctype` module with `is_book_doctype()` helper (moved `is_manpage_doctype()` there too).
  ([#312])

### Changed

- **Passthrough quote processing now uses the PEG grammar** — replaced ~800 lines of
  hand-rolled pattern matching (`markup_patterns.rs`) with a dedicated `quotes_only_inlines`
  PEG rule. The new rule matches only formatting markup (bold, italic, monospace, highlight,
  superscript, subscript, curved quotes) without macros, xrefs, or autolinks, matching
  what "quotes" substitution actually means. Fixes edge cases the manual approach missed
  (nested markup, boundary detection) and makes the passthrough processing consistent with
  the main inline parser.
- **`Source::Url` now wraps `SourceUrl` instead of `url::Url`** — the new `SourceUrl` type
  preserves the original URL string for display, preventing the `url` crate from silently
  altering author URLs (e.g., stripping trailing slashes).
  `SourceUrl` implements `Deref<Target = url::Url>` so existing method calls continue to
  work. ([#335])

### Fixed

- **Section IDs now preserve underscores** — `id_from_title()` no longer strips underscores
  within words (e.g., `CHART_BOT` now generates `_chart_bot` instead of `_chartbot`). This
  also fixes broken cross-references and TOC links that depended on correct section IDs.
- **Warnings and errors now reference the correct file for included content** — warnings
  (e.g., trailing macro content, callout list validation) and errors (e.g., mismatched
  delimiters, section level mismatches) from `include::` directives previously reported
  the entry-point file with wrong line numbers. They now resolve the correct source file
  and line via `source_ranges`. ([#337])
- **Constrained markup boundary detection expanded** — `^`, `~`, `(`, `[`, `{`, and `|`
  are now recognized as valid boundary characters for constrained bold, italic, monospace,
  and highlight markup. Fixes cases where formatting wasn't properly terminated before
  these characters (e.g., `*bold*^super^`).
- **Passthrough content locations are now precise** — `RawText` nodes from non-quotes
  passthroughs now point to the content portion only, not the full macro span including
  delimiters.
- **Tables after definition list items now parse correctly** — when a definition list
  term with an empty description was immediately followed by block attributes (e.g.,
  `[cols="..."]`) and a table delimiter, the attributes line was consumed as principal
  text of the dlist item. The `principal_content` continuation loop now has a negative
  lookahead for `attributes_line()`. ([#332])
- **Bare autolinks no longer capture trailing punctuation** — URLs like
  `https://example.com.` now correctly exclude the trailing `.` from the link target.
  A new `bare_url()` rule with balanced parenthesis handling ensures sentence-level
  punctuation (`.`, `,`, `;`, `!`, `?`, `:`) and surrounding parens are not consumed.
- **URL macro display text no longer produces nested autolinks** — display text in
  `http://example.com[http://example.com]` is now parsed with autolinks suppressed,
  preventing the inner URL from being double-linked.
- **URLs are no longer altered during parsing** — trailing slashes and other author-written
  URL details are now preserved exactly as written. Previously, `http://example.com/` was
  incorrectly shortened to `http://example.com`. ([#335])

## [0.4.0] - 2026-02-07

### Fixed

- **Mixed list nesting at 3+ levels** - Mixed ordered/unordered nesting (e.g.,
  `*` → `.` → `**` → `..`) no longer breaks structure at depth 3+. The parser
  now threads ancestor marker context through cross-type nesting boundaries,
  preventing deeply nested items from consuming sibling markers that belong to a
  parent list context.
- Empty table cells (e.g., `|placeholder||`) were silently dropped, causing rows to be
  rejected with "incorrect column count". Empty cells are now preserved. Also fixed
  first-part handling for nested tables using `!` separator. ([#327])
- Markdown-style listing blocks with a language (`` ```ruby ``) now set `style: "source"`
  in metadata, matching the behavior of `[source,ruby]` blocks. This allows converters'
  `detect_language()` to work consistently regardless of syntax used.
- Inline passthroughs `+...+` and `++...++` no longer convert `...` to an ellipsis
  entity (`&#8230;&#8203;`). The root cause was that non-Quotes passthroughs were emitted
  as `PlainText` nodes, which got merged with adjacent text and lost their passthrough
  identity — the converter then applied the block's full substitutions (including
  Replacements). Passthroughs now carry their own substitution list on the `Raw` node
  (`subs: Vec<Substitution>`) instead of a boolean flag, so the converter applies exactly
  the right subs. ([#323])
- Parser warnings (counters, table column mismatches, invalid anchors, etc.) are no longer
  emitted multiple times due to PEG backtracking. Warnings are now collected during
  parsing with deduplication and emitted once after parsing completes. ([#319])

### Changed

- **BREAKING**: `Raw::escape_special_chars: bool` replaced with `Raw::subs: Vec<Substitution>`.
  The new field carries the passthrough's actual substitution list rather than a lossy
  boolean encoding. An empty vec means raw output (no subs), matching `+++` and `pass:[]`.

## [0.3.0] - 2026-02-01

### Added

- **Nested tables** - Tables can now be nested but if you're reading this, be warned, a
  lot of this bit was "vibe coded". I did check the code and it's _reasonable_ but I
  wouldn't be surprised if something was missed.
- **Support for 'numbered'** - If we find `:numbered:`, support it in the same way we
  support `:sectnums` (they behave in the same way). I think it's reasonable to support
  and it's a couple of lines of code, therefore I think the tradeoff is more than
  reasonable.

### Fixed

- Description list entries now capture implicit text continuations — non-blank lines
  immediately following a `term:: description` line are included in the same description
  paragraph, matching asciidoctor behavior. Previously these continuation lines were
  parsed as separate paragraph blocks, which also broke multi-entry description lists
  into multiple `<dl>` elements.
- Improved attribute substitutions:
  - authors
  - revision
  - xrefs
- Macro attributes (image, audio, video, icon) now parse `.`, `#`, `%` as literal
  characters instead of interpreting them as shorthand syntax. This matches asciidoctor
  behavior where `image::photo.jpg[Diablo 4 picture of Lilith.]` preserves the trailing
  period in alt text, and `image::photo.jpg[.role]` treats `.role` as literal text, not a
  CSS class. Shorthand syntax remains supported in block-level attribute lines (e.g.,
  `[.class]` before a block).
- Handle multi-line content correctly in table cells
- Handle cross-row continuation lines in tables - when a continuation line (no separator)
  appears after a blank line, append it to the previous row's last cell instead of
  dropping it.
- Detect row boundaries for cell specifier lines in tables. When parsing multi-line table
  rows, lines starting with only a cell specifier (like `a|`, `s|`, etc) now correctly
  start a new row instead of being collected in to the previous row. ([#277])

## [0.2.0] - 2026-01-24

### Added

- **`:leveloffset:` attribute support** - Tracks leveloffset ranges during preprocessing
  and applies them when parsing section headings. Included files can have their heading
  levels adjusted relative to the including document.
- **Bibliography anchor parsing** - `[[[anchor]]]` and `[[[anchor,label]]]` syntax in
  bibliography sections is now parsed as a distinct anchor type.
- **Validation for `:secnumlevels:` and `:toclevels:`** - Parser now warns when these
  attributes are set to values outside the valid range (0-5 for secnumlevels, 1-5 for
  toclevels).
- Complete cell specifier support for tables:
  - Colspan: `2+|` spans 2 columns
  - Rowspan: `.2+|` spans 2 rows
  - Combined: `2.3+|` spans 2 columns and 3 rows
  - Cell duplication: `3*|` duplicates cell content 3 times
  - Cell-level horizontal alignment: `<|` (left), `^|` (center), `>|` (right)
  - Cell-level vertical alignment: `.<|` (top), `.^|` (middle), `.>|` (bottom)
  - Cell-level style: `s|` (strong), `e|` (emphasis), `m|` (monospace), etc.
  - All specifiers can be combined (e.g., `2.3+^.^s|` for colspan=2, rowspan=3, centered, strong)
- Tag filtering for include directives ([#279])
  - `tag=name` - include a specific tagged region
  - `tags=a;b;c` - include multiple tags (semicolon or comma delimited)
  - `tags=*` - include all tagged regions
  - `tags=**` - include all content except tag directive lines
  - `tags=!name` - exclude a specific tag
  - `tags=*;!debug` - include all tags except debug
  - `tags=!*` - include only untagged content
  - Tag directives (`// tag::name[]` and `// end::name[]`) are automatically stripped
  - Nested tags supported; combining `tag=` with `lines=` applies both filters
- `substitute()` function for applying substitutions to text. Currently only
  `Attributes` substitution is implemented (expands `{attr}` references). ([#280])
- `parse_text_for_quotes()` function for parsing inline formatting (`*bold*`,
  `_italic_`, etc.) in arbitrary text. Used for quotes substitution in verbatim
  blocks. ([#280])

### Fixed

- DSV tables now correctly preserve the first cell. Previously, DSV format (`cell1:cell2`)
  was incorrectly treated like PSV (`| cell1 | cell2 |`), causing the first cell to be
  dropped. Escape handling (`\:` → literal `:`) also works correctly now.
- Table cell content now has correct source position tracking for multi-line cells
- CSV tables with quoted multiline values now have accurate source positions. Previously,
  positions were approximated without accounting for quote characters, so `"Hello\nWorld"`
  would report incorrect line/column. Now positions point to actual content start (inside
  the quotes), with proper handling for RFC 4180 escaped quotes (`""`).
- Description lists with terms starting with `#` (e.g., `#issue-123:: definition`)
  are no longer incorrectly parsed as section boundaries inside sections. The
  section boundary detection now requires a space after the level marker.
- (setext feature) Description list items are no longer matched as potential
  setext section titles, preventing parse errors when description lists appear
  before delimiter lines.
- Attribute references in attribute definitions are now resolved at definition time,
  matching asciidoctor behavior. Previously, `:foo: {bar}` followed by `:bar: value`
  would incorrectly expand `{foo}` to `value`; now `{foo}` correctly outputs `{bar}`
  (the literal value stored when foo was defined, before bar existed). ([#291])
- `pass:normal[]` and `pass:n[]` passthroughs now correctly expand attribute references.
  The `normal` substitution group includes `attributes`, but this was previously not
  being checked. ([#291])
- Context-aware backslash stripping for `\^` and `\~` escapes now matches asciidoctor
  behavior. Backslashes are only stripped when they prevent actual formatting (e.g.,
  `\^super^`), preserved as literal text when at word boundaries without closing marker
  (e.g., `\^caret`). ([#278])
- Discrete headings (`[discrete]`) at the end of a document are now parsed correctly
  instead of causing a parsing error. ([#289])
- Paragraphs no longer incorrectly split when a line starts with inline passthrough
  syntax like `+>+`. The list continuation lookahead now only matches actual
  continuation markers (standalone `+` followed by whitespace/EOL/EOF).
- Default table cells now treat list markers, delimited blocks, toc macros, and page
  breaks as literal text instead of parsing them as blocks. Only cells with the `a`
  (AsciiDoc) style get full block parsing, matching asciidoctor behavior.
- Nested include paths are now resolved relative to the parent file's directory instead
  of the root document's directory. ([#317])
- Anchors before section headers (e.g., `[#myid]\n= Section`) are now correctly associated
  with the section. Previously, list continuation and paragraph parsing consumed the
  anchor as content, causing explicit IDs to be lost when using `include::[]` with
  `leveloffset`. ([#321])

### Changed

- **BREAKING**: `TableColumn` struct now includes `colspan`, `rowspan`, `halign`, `valign`,
  and `style` fields.
- **BREAKING**: `BlockMetadata.substitutions` changed from `Option<Vec<Substitution>>`
  to `Option<SubstitutionSpec>`. New types `SubstitutionSpec` and `SubstitutionOp` are
  now public exports. Modifier syntax (`+quotes`, `-callouts`) is now stored as operations
  and resolved by converters with the appropriate baseline (NORMAL vs VERBATIM) rather
  than being eagerly resolved by the parser.
- **BREAKING**: `CrossReference.text` changed from `Option<String>` to `Vec<InlineNode>`,
  enabling rich inline markup in cross-reference text (e.g., `<<id,*bold* text>>`). The
  `with_text()` method now accepts `Vec<InlineNode>`. Serialization outputs `"inlines"`
  array instead of `"text"` string. ([#320])

### Removed

- **BREAKING**: Removed `Deserialize` implementation from all model types (`Document`,
  `Block`, `InlineNode`, and ~60 other AST types). Serialization to JSON remains
  supported. If you need to load previously serialized AST, parse the original
  AsciiDoc source instead.

## [0.1.4] - 2026-01-04

### Added

- Index term support with type-safe `IndexTermKind` enum ([#274])
  - Flow terms (visible): `((term))` or `indexterm2:[term]`
  - Concealed terms (hidden): `(((term,secondary,tertiary)))` or `indexterm:[...]`
  - `IndexTermKind::Flow` can only hold a single term (no hierarchy)
  - `IndexTermKind::Concealed` supports primary/secondary/tertiary terms
- Callout references (`<1>`, `<.>`) in source/listing blocks now have source locations in
  the AST
- New `CalloutRef` inline node type for programmatic access to callout markers

### Changed

- Auto-numbered callouts (`<.>`) are now resolved during parsing, not rendering
- JSON serialization for Callout references uses `"name": "callout_reference"`

### Fixed

- List continuations with blank line before `+` now correctly attach to ancestor list
  items instead of the last nested item, matching asciidoctor behaviour ([#265])

## [0.1.3] - 2026-01-02

### Fixed

- Description lists now correctly break when a `[[id]]` anchor is present between items
  ([#269])

## [0.1.2] - 2026-01-02

### Added

- Support for `{blank}`, `{cxx}`, and `{pp}` character replacement attributes
- `SafeMode` enum now exported from `acdc_parser` (migrated from removed `acdc-core`)
- `FromStr` and `Display` implementations for `SafeMode`

### Fixed

- `{empty}` attribute now works in description lists and inline contexts ([#262])
- `{lt}`, `{gt}`, `{amp}` character replacement attributes now produce raw HTML characters
  that bypass escaping, matching asciidoctor behaviour ([#266])

### Changed

- Removed dependency on `acdc-core`

## [0.1.1] - 2025-12-31

### Added

- README.md instead of README.adoc for `acdc-parser` to make sure it gets picked up by
  cargo publish.
- This Changelog 🥳

### Fixed

- Handle backslash-escaped patterns in URLs and text
  Fixes URLs like `{repo}/compare/v1.0\...v2.0` producing correct output instead of broken
  links with forward slashes or ellipsis entities.
- Documentation of a few models in the parser is now closer to what they actually are, and
  do, basically their intent.

## [0.1.0] - 2025-12-28

Initial release of acdc-parser, a PEG-based AsciiDoc parser with source location tracking.

[#262]: https://github.com/nlopes/acdc/issues/262
[#265]: https://github.com/nlopes/acdc/issues/265
[#266]: https://github.com/nlopes/acdc/issues/266
[#269]: https://github.com/nlopes/acdc/issues/269
[#274]: https://github.com/nlopes/acdc/issues/274
[#277]: https://github.com/nlopes/acdc/issues/277
[#278]: https://github.com/nlopes/acdc/issues/278
[#279]: https://github.com/nlopes/acdc/issues/279
[#280]: https://github.com/nlopes/acdc/issues/280
[#289]: https://github.com/nlopes/acdc/issues/289
[#291]: https://github.com/nlopes/acdc/issues/291
[#317]: https://github.com/nlopes/acdc/issues/317
[#319]: https://github.com/nlopes/acdc/issues/319
[#320]: https://github.com/nlopes/acdc/issues/320
[#321]: https://github.com/nlopes/acdc/issues/321
[#323]: https://github.com/nlopes/acdc/issues/323
[#327]: https://github.com/nlopes/acdc/issues/327
[#332]: https://github.com/nlopes/acdc/issues/332
[#312]: https://github.com/nlopes/acdc/issues/312
[#335]: https://github.com/nlopes/acdc/issues/335
[#337]: https://github.com/nlopes/acdc/issues/337
[#343]: https://github.com/nlopes/acdc/issues/343
[#349]: https://github.com/nlopes/acdc/issues/349
[#357]: https://github.com/nlopes/acdc/issues/357

[Unreleased]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.11.0...HEAD
[0.11.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.10.0...acdc-parser-v0.11.0
[0.10.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.9.0...acdc-parser-v0.10.0
[0.9.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.8.0...acdc-parser-v0.9.0
[0.8.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.7.0...acdc-parser-v0.8.0
[0.7.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.6.0...acdc-parser-v0.7.0
[0.6.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.5.0...acdc-parser-v0.6.0
[0.5.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.4.0...acdc-parser-v0.5.0
[0.4.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.3.0...acdc-parser-v0.4.0
[0.3.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.2.0...acdc-parser-v0.3.0
[0.2.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.1.4...acdc-parser-v0.2.0
[0.1.4]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.1.3...acdc-parser-v0.1.4
[0.1.3]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.1.2...acdc-parser-v0.1.3
[0.1.2]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.1.1...acdc-parser-v0.1.2
[0.1.1]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.1.0...acdc-parser-v0.1.1
[0.1.0]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.1.0
