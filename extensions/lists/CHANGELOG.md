# Changelog

## [Unreleased]

- Documents with repeated `list-of` requests allocate less memory during conversion.

- CLI builds with `lists` can select the extension with `--extension lists` for
  any enabled converter. The library's optional `network` feature enables remote
  includes under the parser's caller-permission and safe-mode rules.

- Register `list-of::<element>[]` to generate linked caption and title lines for
  document elements through any converter. Existing IDs are preserved; generated
  IDs are deterministic and reused by repeated requests. Unknown elements produce
  a located warning and readable call text. Empty lists disappear while their
  sections remain.
- Unknown-element recovery preserves the supplied arguments, quotes, and
  unexpanded attribute references in the call text.
- `hide_empty_section` removes the nearest section only when no matching blocks
  exist; untitled matching blocks keep it. The first positional slot selects the
  same policy. As in the pinned Ruby reference, string values such as `false`,
  `0`, and `""` enable hiding. Outside sections, only the empty output is removed.
  Nested calls and table cells follow section scope, and pending calls inside a
  removed section are canceled. Unlike the reference, extra nonempty positional
  arguments are rejected and root-level hiding calls keep the document.
- Titles retain their parsed formatting and original attribute context. Unlike
  the Ruby extension's default mode, rendered HTML tags are not copied into text
  and escaped again. Uncaptioned titles appear once, as the link label. The Ruby
  `enhanced_rendering` toggle is intentionally unsupported; supplying it reports
  that title formatting is automatic and asks the caller to remove the option.
