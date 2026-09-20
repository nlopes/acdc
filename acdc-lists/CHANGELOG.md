# Changelog

All notable changes to `acdc-lists` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `list-of::<element>[]` builds a list of every block of that kind that carries
  a title or a caption, so a document can offer a list of figures, of tables,
  or of anything else it contains:

  ```asciidoc
  == List of figures
  list-of::image[]

  == List of tables
  list-of::table[]
  ```

  Each entry links to the element it names. An element whose caption supplies a
  prefix contributes that prefix as the link and its title as the text after it
  — `Figure 1 The wonderful linux logo`, with `Figure 1` linked — so the list
  follows `figure-caption`, `listing-caption` and the rest. An element with
  only a title puts the whole title in the link.

- The element is named with Asciidoctor's block context: `image`, `table`,
  `listing`, `literal`, `example`, `quote`, `verse`, `sidebar`, `open`, `pass`,
  `stem`, `audio`, `video`, `admonition`, `paragraph`, `section`, `olist`,
  `ulist`, `dlist`, or `colist`. A `[source]` block is listed as a `listing`,
  as it is in `asciidoctor`. Any other name is reported and the call is left in
  the document unchanged, so the mistake is visible rather than silent.

- `hide_empty_section=true` removes the section holding the call when the list
  comes out empty. Without it an empty list simply leaves nothing behind.

- Elements nested anywhere are found, including inside an `AsciiDoc` table
  cell, and are listed in document order.

- An element that has no id is given one, so a list can link to a figure the
  document never anchored. The id is the element name and the entry's position
  — `image-1`, `table-2` — which reads sensibly in a URL and stays the same
  between runs, so `:reproducible:` output remains reproducible. An id the
  document already uses is never taken over or duplicated.

### Divergences from `asciidoctor-lists`

- Generated ids are readable rather than UUIDs, as above.
- A caption in a list entry carries no trailing period: acdc renders a
  caption-only cross-reference as `Figure 1`, the same as `xrefstyle=short`,
  instead of repeating the separator that follows a block title.
- The last entry is not followed by a trailing line break.
- `enhanced_rendering` is accepted and ignored. It exists in the original to
  make a title's inline markup render inside the list; acdc does that
  unconditionally, because a title is inline content in the document model
  rather than pre-rendered text.
- The pre-1.0.6 spelling `list-of::[element=image]` is still understood.
