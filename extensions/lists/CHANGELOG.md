# Changelog

## [Unreleased]

- Register `list-of::<element>[]` to generate linked caption and title lines for
  document elements through any converter. Existing IDs are preserved; generated
  IDs are deterministic and reused by repeated requests. Unknown elements produce
  a located warning and readable call text. Empty lists disappear while their
  sections remain.
- Unknown-element recovery preserves the supplied arguments, quotes, and
  unexpanded attribute references in the call text.
- Titles retain their parsed formatting and original attribute context. Unlike
  the Ruby extension's default mode, rendered HTML tags are not copied into text
  and escaped again. Uncaptioned titles appear once, as the link label.
