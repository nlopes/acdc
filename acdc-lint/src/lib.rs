//! `AsciiDoc` linting primitives.
//!
//! This crate defines the public lint registry, lint levels, and report
//! structures used by `acdc lint`.
//!
//! # Terminology
//!
//! A lint is a user-visible rule that can be configured by name and emitted as
//! a diagnostic. In code, a lint is identified by [`LintId`].
//!
//! [`LintInfo`] is the static, user-facing metadata for one lint: its stable
//! command-line name, default level, short summary, and long explanation. The
//! [`LINTS`] registry is the source of truth for metadata and is suitable for
//! future rule documentation or `--explain` output.
//!
//! A lint pass is an internal execution unit. A pass owns one traversal or
//! checker function and may emit diagnostics for one or more lint IDs. Passes
//! are grouped around efficient implementation, while lints remain the stable
//! user-facing rule boundary.

use std::{fmt, fs, path::Path, str::FromStr};

mod error;
mod registry;
mod rules;
mod runner;
pub use error::Error;

/// A lint severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LintLevel {
    /// Suppress a lint.
    Allow,
    /// Emit a warning but keep the lint run successful.
    Warn,
    /// Emit an error and make the lint run fail.
    Deny,
    /// Emit an error and reject later attempts to lower this lint's level.
    Forbid,
}

impl LintLevel {
    /// Returns whether this level fails the lint run when diagnostics are emitted.
    #[must_use]
    pub const fn is_error(self) -> bool {
        matches!(self, Self::Deny | Self::Forbid)
    }
}

impl fmt::Display for LintLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Allow => "allow",
            Self::Warn => "warn",
            Self::Deny => "deny",
            Self::Forbid => "forbid",
        })
    }
}

/// Stable identifier for one user-visible lint rule.
///
/// `LintId` is the value emitted on diagnostics and used by command-line
/// severity overrides. It intentionally contains no execution behavior and no
/// prose metadata; those live in `LintPass` and [`LintInfo`] respectively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LintId {
    /// Prefer `.adoc` and avoid `.asc`.
    DocumentExtension,
    /// Keep prose source readable by writing one sentence per source line.
    OneSentencePerLine,
    /// Prefer asymmetric ATX section titles over symmetric ATX titles.
    SectionTitleSymmetricMarker,
    /// Prefer ATX section titles over setext underline titles.
    SectionTitleSetextStyle,
    /// Include an author line immediately after the document title.
    DocumentTitleAuthor,
    /// Include a revision line after the author line.
    DocumentTitleRevision,
    /// Surface parser warnings for skipped section levels.
    SectionLevelSequence,
    /// Surface parser warnings for unterminated delimited blocks.
    UnterminatedDelimitedBlock,
    /// Surface parser warnings for unterminated tables.
    UnterminatedTable,
    /// Surface parser warnings for unsupported counter syntax.
    CounterSyntax,
    /// Detect more than one document title in a non-book document.
    MultipleDocumentTitle,
    /// Surface parser warnings for unknown table formats.
    TableUnknownFormat,
    /// Surface parser warnings for incomplete table rows.
    TableIncompleteRow,
    /// Surface parser warnings for incorrect table row column counts.
    TableColumnCount,
    /// Surface parser warnings for table cells that exceed the column count.
    TableCellOverflow,
    /// Use the minimum required delimiter length for delimited blocks.
    DelimitedBlockMinimalDelimiter,
    /// Put whitespace after heading markers.
    SectionTitleMarkerSpacing,
    /// Start headings with an uppercase letter.
    SectionTitleCapitalization,
    /// Start headings with a leading monospace span with an uppercase letter.
    SectionTitleCapitalizationMonospace,
    /// Put a blank line before delimited blocks.
    DelimitedBlockLeadingBlankLine,
    /// Put a blank line after delimited blocks.
    DelimitedBlockTrailingBlankLine,
    /// Avoid trailing whitespace.
    TrailingWhitespace,
    /// Avoid hard tab characters.
    HardTab,
    /// Avoid repeated blank lines.
    ExcessiveBlankLines,
    /// Put whitespace after list markers.
    ListMarkerSpacing,
    /// Use `url-` or `uri-` prefixes for URL-valued attributes.
    AttributeUrlPrefix,
    /// Use `imagesdir` instead of repeating the image directory in each target.
    Imagesdir,
    /// Provide image alt text.
    ImageAltText,
    /// Reference image files that exist on disk.
    ImageTargetExists,
    /// Use asterisk markers for nested unordered lists.
    NestedUnorderedListMarker,
    /// Separate adjacent lists with an empty line comment.
    AdjacentListSeparator,
    /// Prefer `AsciiDoc` dot ordered-list markers over explicit numbers.
    OrderedListExplicitNumber,
    /// Prefer real description lists over bold-term paragraphs.
    DescriptionListBoldTerm,
    /// Avoid Markdown heading syntax in `AsciiDoc`.
    MarkdownHeading,
    /// Avoid Markdown link syntax in `AsciiDoc`.
    MarkdownLink,
    /// Avoid Markdown image syntax in `AsciiDoc`.
    MarkdownImage,
    /// Avoid Markdown code fences in `AsciiDoc`.
    MarkdownCodeFence,
    /// Avoid Markdown table syntax in `AsciiDoc`.
    MarkdownTable,
}

impl LintId {
    /// Returns this lint's command-line name.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.info().name
    }

    /// Returns this lint's default level.
    #[must_use]
    pub fn default_level(self) -> LintLevel {
        self.info().default_level
    }

    /// Returns this lint's short user-facing description.
    #[must_use]
    pub fn summary(self) -> &'static str {
        self.info().summary
    }

    /// Returns this lint's long-form explanation.
    #[must_use]
    pub fn explanation(self) -> &'static str {
        self.info().explanation
    }

    /// Returns this lint's default remediation help, when it has one.
    #[must_use]
    pub fn help(self) -> Option<&'static str> {
        self.info().help
    }

    #[allow(clippy::unreachable)]
    fn info(self) -> &'static LintInfo {
        match LINTS.get(self as usize) {
            Some(info) => info,
            None => unreachable!("registered lint metadata must contain every LintId"),
        }
    }
}

impl fmt::Display for LintId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for LintId {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        LINTS
            .iter()
            .find(|info| info.name == value)
            .map(|info| info.id)
            .ok_or_else(|| Error::UnknownLintName {
                name: value.to_owned(),
            })
    }
}

/// User-facing metadata for one lint.
///
/// This is documentation/configuration data, not executable lint logic. The
/// implementation that emits a lint lives in one of the internal lint passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LintInfo {
    /// Stable command-line name.
    pub name: &'static str,
    /// Stable lint identifier.
    pub id: LintId,
    /// Default level before command-line overrides are applied.
    pub default_level: LintLevel,
    /// Short user-facing description.
    pub summary: &'static str,
    /// Long-form explanation for documentation and future explain output.
    pub explanation: &'static str,
    /// Default remediation help for emitted diagnostics.
    ///
    /// Rule implementations can override this with occurrence-specific help.
    pub help: Option<&'static str>,
}

/// Static metadata registry for every known lint.
///
/// This registry defines the stable lint names and documentation text. Execution
/// is registered separately by internal lint passes.
pub const LINTS: &[LintInfo] = &[
    LintInfo {
        name: "document-extension",
        id: LintId::DocumentExtension,
        default_level: LintLevel::Warn,
        summary: "prefer .adoc and avoid .asc",
        explanation: "Checks file paths passed to the linter. AsciiDoc sources should use the \
                      .adoc extension so editor, build, and publishing tools can identify them \
                      consistently.",
        help: Some("rename the file to use the .adoc extension"),
    },
    LintInfo {
        name: "one-sentence-per-line",
        id: LintId::OneSentencePerLine,
        default_level: LintLevel::Warn,
        summary: "write one sentence per source line",
        explanation: "Checks prose paragraphs for multiple sentences on one source line. Keeping \
                      one sentence per line makes reviews and diffs easier without changing \
                      rendered output.",
        help: Some("write one complete sentence per source line"),
    },
    LintInfo {
        name: "section-title-symmetric-marker",
        id: LintId::SectionTitleSymmetricMarker,
        default_level: LintLevel::Warn,
        summary: "prefer asymmetric ATX section titles",
        explanation: "Checks section titles written with a closing marker such as `== Title ==`. \
                      AsciiDoc does not require the closing marker, and asymmetric titles are \
                      simpler to edit.",
        help: Some("remove the closing title marker"),
    },
    LintInfo {
        name: "section-title-setext-style",
        id: LintId::SectionTitleSetextStyle,
        default_level: LintLevel::Warn,
        summary: "prefer ATX section titles over setext titles",
        explanation: "Checks two-line underline section titles. ATX-style section titles such as \
                      `== Title` make the section level explicit on the title line.",
        help: Some("use an asymmetric ATX title such as `== Section`"),
    },
    LintInfo {
        name: "document-title-author",
        id: LintId::DocumentTitleAuthor,
        default_level: LintLevel::Allow,
        summary: "include an author line after the document title",
        explanation: "Checks whether a document header with a title also has an author line. This \
                      lint is opt-in because not every document type needs author metadata.",
        help: Some("add an author line immediately after the document title"),
    },
    LintInfo {
        name: "document-title-revision",
        id: LintId::DocumentTitleRevision,
        default_level: LintLevel::Allow,
        summary: "include a revision line after the author line",
        explanation: "Checks whether a document header with an author also has revision metadata. \
                      This lint is opt-in because revision lines are a project convention, not a \
                      general AsciiDoc requirement.",
        help: Some("add a revision line after the author line"),
    },
    LintInfo {
        name: "section-level-sequence",
        id: LintId::SectionLevelSequence,
        default_level: LintLevel::Warn,
        summary: "do not skip section levels",
        explanation: "Surfaces parser recovery warnings for skipped section levels, such as \
                      jumping from level 1 to level 3. Sequential section levels preserve the \
                      intended document outline.",
        help: Some("adjust section levels so they increase one level at a time"),
    },
    LintInfo {
        name: "unterminated-delimited-block",
        id: LintId::UnterminatedDelimitedBlock,
        default_level: LintLevel::Warn,
        summary: "close delimited blocks",
        explanation: "Surfaces parser recovery warnings for delimited blocks without a matching \
                      closing delimiter. Unterminated blocks can consume more source than \
                      intended.",
        help: Some("add the matching closing block delimiter"),
    },
    LintInfo {
        name: "unterminated-table",
        id: LintId::UnterminatedTable,
        default_level: LintLevel::Warn,
        summary: "close table blocks",
        explanation: "Surfaces parser recovery warnings for table blocks without a closing \
                      delimiter. Unterminated tables can change how following content is parsed.",
        help: Some("add the matching table closing delimiter"),
    },
    LintInfo {
        name: "counter-syntax",
        id: LintId::CounterSyntax,
        default_level: LintLevel::Warn,
        summary: "avoid unsupported counter syntax",
        explanation: "Surfaces parser warnings for counter syntax that acdc does not support. \
                      These counters are removed from output, so authors should avoid relying on \
                      them.",
        help: Some("remove the unsupported counter or replace it with supported content"),
    },
    LintInfo {
        name: "multiple-document-title",
        id: LintId::MultipleDocumentTitle,
        default_level: LintLevel::Warn,
        summary: "use only one document title",
        explanation: "Checks non-book documents for more than one top-level document title. After \
                      the document title, additional headings should normally be section titles.",
        help: Some("use section titles (`==`) after the document title"),
    },
    LintInfo {
        name: "table-unknown-format",
        id: LintId::TableUnknownFormat,
        default_level: LintLevel::Warn,
        summary: "use a supported table format",
        explanation: "Surfaces parser warnings for table formats that acdc does not support. \
                      Unknown formats may not be parsed with the structure authors expect.",
        help: Some("use a supported table format or remove the format attribute"),
    },
    LintInfo {
        name: "table-incomplete-row",
        id: LintId::TableIncompleteRow,
        default_level: LintLevel::Warn,
        summary: "complete table rows",
        explanation: "Surfaces parser warnings for table rows that end before all expected cells \
                      are present. Incomplete rows can shift table content in rendered output.",
        help: Some("add the missing table cells or adjust the column count"),
    },
    LintInfo {
        name: "table-column-count",
        id: LintId::TableColumnCount,
        default_level: LintLevel::Warn,
        summary: "match table rows to the configured column count",
        explanation: "Surfaces parser warnings for rows whose cell count does not match the \
                      configured table column count. Matching the column count keeps table \
                      structure predictable.",
        help: Some("match each table row to the configured column count"),
    },
    LintInfo {
        name: "table-cell-overflow",
        id: LintId::TableCellOverflow,
        default_level: LintLevel::Warn,
        summary: "keep table cells within the configured column count",
        explanation: "Surfaces parser warnings for cells that overflow the configured table column \
                      count. Overflowing cells indicate that the table structure is ambiguous.",
        help: Some("remove extra cells or increase the configured column count"),
    },
    LintInfo {
        name: "delimited-block-minimal-delimiter",
        id: LintId::DelimitedBlockMinimalDelimiter,
        default_level: LintLevel::Warn,
        summary: "use minimum required delimited block fences",
        explanation: "Checks delimited blocks for fences longer than the minimum required by \
                      AsciiDoc. Minimal delimiters reduce visual noise while preserving block \
                      semantics.",
        help: Some("shorten the opening and closing block delimiters"),
    },
    LintInfo {
        name: "section-title-marker-spacing",
        id: LintId::SectionTitleMarkerSpacing,
        default_level: LintLevel::Warn,
        summary: "put whitespace after section title markers",
        explanation: "Checks ATX-style section titles for missing whitespace after the marker. The \
                      space separates the marker from the title text and avoids ambiguous source.",
        help: Some("insert a space after the opening title marker"),
    },
    LintInfo {
        name: "section-title-capitalization",
        id: LintId::SectionTitleCapitalization,
        default_level: LintLevel::Warn,
        summary: "start section titles with an uppercase letter",
        explanation: "Checks document, section, and discrete titles whose first alphabetic \
                      character is lowercase. This is a style lint for projects that expect \
                      title-style starts. Leading monospace spans are ignored so tool and command \
                      names can keep their exact casing.",
        help: Some("capitalize the first word of the title"),
    },
    LintInfo {
        name: "section-title-capitalization-monospace",
        id: LintId::SectionTitleCapitalizationMonospace,
        default_level: LintLevel::Allow,
        summary: "start leading monospace title text with an uppercase letter",
        explanation: "Checks document, section, and discrete titles whose first alphabetic \
                      character is lowercase inside a leading monospace span. This lint is opt-in \
                      because titles often start with case-sensitive tool, package, or command \
                      names.",
        help: Some("capitalize the leading monospace title text"),
    },
    LintInfo {
        name: "delimited-block-leading-blank-line",
        id: LintId::DelimitedBlockLeadingBlankLine,
        default_level: LintLevel::Warn,
        summary: "put a blank line before delimited blocks",
        explanation: "Checks delimited block openings that directly follow another source line. A \
                      blank line before the block makes the block boundary clear.",
        help: Some("insert a blank line before the opening delimiter"),
    },
    LintInfo {
        name: "delimited-block-trailing-blank-line",
        id: LintId::DelimitedBlockTrailingBlankLine,
        default_level: LintLevel::Warn,
        summary: "put a blank line after delimited blocks",
        explanation: "Checks delimited block closings that are immediately followed by more \
                      content. A blank line after the block makes the following block boundary \
                      clear.",
        help: Some("insert a blank line after the closing delimiter"),
    },
    LintInfo {
        name: "trailing-whitespace",
        id: LintId::TrailingWhitespace,
        default_level: LintLevel::Warn,
        summary: "avoid trailing whitespace",
        explanation: "Checks source lines that end with whitespace. Trailing whitespace is \
                      invisible in editors and creates noisy diffs.",
        help: Some("remove the trailing whitespace"),
    },
    LintInfo {
        name: "hard-tab",
        id: LintId::HardTab,
        default_level: LintLevel::Warn,
        summary: "avoid hard tab characters",
        explanation: "Checks source lines containing tab characters. Spaces keep indentation and \
                      alignment stable across editors and renderers.",
        help: Some("replace the tab with spaces"),
    },
    LintInfo {
        name: "excessive-blank-lines",
        id: LintId::ExcessiveBlankLines,
        default_level: LintLevel::Warn,
        summary: "avoid repeated blank lines",
        explanation: "Checks for repeated blank lines. A single blank line is enough to separate \
                      adjacent blocks in normal AsciiDoc source.",
        help: Some("keep a single blank line between adjacent blocks"),
    },
    LintInfo {
        name: "list-marker-spacing",
        id: LintId::ListMarkerSpacing,
        default_level: LintLevel::Warn,
        summary: "put whitespace after list markers",
        explanation: "Checks list markers without whitespace before the item text. The spacing \
                      makes list syntax unambiguous and easier to scan.",
        help: Some("insert a space after the list marker"),
    },
    LintInfo {
        name: "attribute-url-prefix",
        id: LintId::AttributeUrlPrefix,
        default_level: LintLevel::Warn,
        summary: "prefix URL-valued attributes with url- or uri-",
        explanation: "Checks URL-valued attributes that are not named with a `url-` or `uri-` \
                      prefix. Prefixing documents the attribute's expected value type.",
        help: Some("rename the attribute with a url- or uri- prefix"),
    },
    LintInfo {
        name: "imagesdir",
        id: LintId::Imagesdir,
        default_level: LintLevel::Warn,
        summary: "use imagesdir instead of repeating image directories",
        explanation: "Checks image targets that repeat a directory path. Prefer setting \
                      `:imagesdir:` once and using filename-only image targets.",
        help: Some("set :imagesdir: and use filename-only image targets"),
    },
    LintInfo {
        name: "image-alt-text",
        id: LintId::ImageAltText,
        default_level: LintLevel::Warn,
        summary: "provide image alt text",
        explanation: "Checks image macros with missing or empty alt text. Alt text improves \
                      accessible output and gives non-visual renderers meaningful fallback text.",
        help: Some("add positional alt text or an `alt=` attribute"),
    },
    LintInfo {
        name: "image-target-exists",
        id: LintId::ImageTargetExists,
        default_level: LintLevel::Warn,
        summary: "reference image files that exist",
        explanation: "Checks local image targets resolved from the source path and `imagesdir`. \
                      Missing image files usually indicate broken output.",
        help: Some("fix the image target or create the referenced file"),
    },
    LintInfo {
        name: "nested-unordered-list-marker",
        id: LintId::NestedUnorderedListMarker,
        default_level: LintLevel::Warn,
        summary: "use asterisk markers for nested unordered lists",
        explanation: "Checks nested unordered list markers that do not use asterisk depth markers. \
                      Asterisk markers make nesting depth explicit in AsciiDoc source.",
        help: Some("use asterisk markers for nested unordered lists"),
    },
    LintInfo {
        name: "adjacent-list-separator",
        id: LintId::AdjacentListSeparator,
        default_level: LintLevel::Warn,
        summary: "separate adjacent lists with an empty line comment",
        explanation: "Checks adjacent list blocks that are not separated by an empty line comment. \
                      The separator makes it explicit that the lists are distinct blocks.",
        help: Some("insert a line comment such as `//-` between the lists"),
    },
    LintInfo {
        name: "ordered-list-explicit-number",
        id: LintId::OrderedListExplicitNumber,
        default_level: LintLevel::Warn,
        summary: "prefer dot ordered-list markers over explicit numbers",
        explanation: "Checks ordered list items written with explicit numeric markers. Dot markers \
                      let AsciiDoc number the list and avoid stale numbering after edits.",
        help: Some("use AsciiDoc dot syntax such as `. item`"),
    },
    LintInfo {
        name: "description-list-bold-term",
        id: LintId::DescriptionListBoldTerm,
        default_level: LintLevel::Warn,
        summary: "prefer description-list syntax over bold-term paragraphs",
        explanation: "Checks paragraphs that look like bold terms followed by text. \
                      Description-list syntax captures that structure directly.",
        help: Some("use description-list syntax such as `Term:: description`"),
    },
    LintInfo {
        name: "markdown-heading",
        id: LintId::MarkdownHeading,
        default_level: LintLevel::Warn,
        summary: "avoid Markdown heading syntax",
        explanation: "Checks Markdown `#` heading markers in AsciiDoc source. Use AsciiDoc \
                      section markers so the document is parsed consistently as AsciiDoc.",
        help: Some("use AsciiDoc section markers such as `== Section`"),
    },
    LintInfo {
        name: "markdown-link",
        id: LintId::MarkdownLink,
        default_level: LintLevel::Warn,
        summary: "avoid Markdown link syntax",
        explanation: "Checks Markdown inline link syntax in AsciiDoc source. Use AsciiDoc link \
                      macros or bare URLs instead.",
        help: Some("use `link:target[text]` or an AsciiDoc URL macro"),
    },
    LintInfo {
        name: "markdown-image",
        id: LintId::MarkdownImage,
        default_level: LintLevel::Warn,
        summary: "avoid Markdown image syntax",
        explanation: "Checks Markdown image syntax in AsciiDoc source. Use `image::` or `image:` \
                      macros so image attributes and paths follow AsciiDoc rules.",
        help: Some("use `image::target[alt]` or `image:target[alt]`"),
    },
    LintInfo {
        name: "markdown-code-fence",
        id: LintId::MarkdownCodeFence,
        default_level: LintLevel::Warn,
        summary: "avoid Markdown code fences",
        explanation: "Checks Markdown backtick code fences in AsciiDoc source. Use AsciiDoc \
                      listing blocks, commonly delimited with `----`.",
        help: Some("use an AsciiDoc listing block delimiter such as `----`"),
    },
    LintInfo {
        name: "markdown-table",
        id: LintId::MarkdownTable,
        default_level: LintLevel::Warn,
        summary: "avoid Markdown table syntax",
        explanation: "Checks Markdown pipe-table separator rows in AsciiDoc source. Use AsciiDoc \
                      table blocks such as `|===` for table content.",
        help: Some("use an AsciiDoc table block such as `|===`"),
    },
];

/// A named group of lints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LintGroup {
    /// Every registered lint.
    All,
    /// The recommended-practices lint set.
    RecommendedPractices,
    /// Document-shape and parser-recovery lints.
    DocumentStructure,
    /// Physical source-formatting lints.
    SourceFormat,
    /// Semantic `AsciiDoc` authoring lints.
    SemanticAsciiDoc,
    /// Resource-reference lints.
    Resources,
}

impl LintGroup {
    /// Returns this group's command-line name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::RecommendedPractices => "recommended-practices",
            Self::DocumentStructure => "document-structure",
            Self::SourceFormat => "source-format",
            Self::SemanticAsciiDoc => "semantic-asciidoc",
            Self::Resources => "resources",
        }
    }

    /// Returns whether this group includes `lint`.
    #[must_use]
    pub fn contains(self, lint: LintId) -> bool {
        match self {
            Self::All => true,
            Self::RecommendedPractices => is_recommended_practice(lint),
            Self::DocumentStructure => is_document_structure(lint),
            Self::SourceFormat => is_source_format(lint),
            Self::SemanticAsciiDoc => is_semantic_asciidoc(lint),
            Self::Resources => is_resource_lint(lint),
        }
    }
}

fn is_recommended_practice(lint: LintId) -> bool {
    matches!(
        lint,
        LintId::DocumentExtension
            | LintId::OneSentencePerLine
            | LintId::SectionTitleSymmetricMarker
            | LintId::SectionTitleSetextStyle
            | LintId::DelimitedBlockMinimalDelimiter
            | LintId::AttributeUrlPrefix
            | LintId::Imagesdir
            | LintId::NestedUnorderedListMarker
            | LintId::AdjacentListSeparator
    )
}

fn is_document_structure(lint: LintId) -> bool {
    matches!(
        lint,
        LintId::DocumentExtension
            | LintId::DocumentTitleAuthor
            | LintId::DocumentTitleRevision
            | LintId::SectionLevelSequence
            | LintId::UnterminatedDelimitedBlock
            | LintId::UnterminatedTable
            | LintId::CounterSyntax
            | LintId::MultipleDocumentTitle
            | LintId::TableUnknownFormat
            | LintId::TableIncompleteRow
            | LintId::TableColumnCount
            | LintId::TableCellOverflow
    )
}

fn is_source_format(lint: LintId) -> bool {
    matches!(
        lint,
        LintId::OneSentencePerLine
            | LintId::SectionTitleSymmetricMarker
            | LintId::SectionTitleSetextStyle
            | LintId::DelimitedBlockMinimalDelimiter
            | LintId::SectionTitleMarkerSpacing
            | LintId::SectionTitleCapitalization
            | LintId::SectionTitleCapitalizationMonospace
            | LintId::DelimitedBlockLeadingBlankLine
            | LintId::DelimitedBlockTrailingBlankLine
            | LintId::TrailingWhitespace
            | LintId::HardTab
            | LintId::ExcessiveBlankLines
            | LintId::ListMarkerSpacing
            | LintId::NestedUnorderedListMarker
            | LintId::AdjacentListSeparator
    )
}

fn is_semantic_asciidoc(lint: LintId) -> bool {
    matches!(
        lint,
        LintId::AttributeUrlPrefix
            | LintId::OrderedListExplicitNumber
            | LintId::DescriptionListBoldTerm
            | LintId::MarkdownHeading
            | LintId::MarkdownLink
            | LintId::MarkdownImage
            | LintId::MarkdownCodeFence
            | LintId::MarkdownTable
    )
}

fn is_resource_lint(lint: LintId) -> bool {
    matches!(
        lint,
        LintId::Imagesdir | LintId::ImageAltText | LintId::ImageTargetExists
    )
}

impl fmt::Display for LintGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for LintGroup {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "all" => Ok(Self::All),
            "recommended-practices" => Ok(Self::RecommendedPractices),
            "document-structure" => Ok(Self::DocumentStructure),
            "source-format" => Ok(Self::SourceFormat),
            "semantic-asciidoc" => Ok(Self::SemanticAsciiDoc),
            "resources" => Ok(Self::Resources),
            _ => Err(Error::UnknownLintName {
                name: value.to_owned(),
            }),
        }
    }
}

/// A lint command-line selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LintSelector {
    /// Select one lint.
    Lint(LintId),
    /// Select a lint group.
    Group(LintGroup),
}

impl LintSelector {
    /// Returns whether this selector includes `lint`.
    #[must_use]
    pub fn contains(self, lint: LintId) -> bool {
        match self {
            Self::Lint(selected) => selected == lint,
            Self::Group(group) => group.contains(lint),
        }
    }
}

impl fmt::Display for LintSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lint(lint) => lint.fmt(f),
            Self::Group(group) => group.fmt(f),
        }
    }
}

impl FromStr for LintSelector {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Ok(group) = LintGroup::from_str(value) {
            return Ok(Self::Group(group));
        }
        if let Ok(lint) = LintId::from_str(value) {
            return Ok(Self::Lint(lint));
        }
        Err(Error::UnknownLintName {
            name: value.to_owned(),
        })
    }
}

/// A command-line lint level override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LintOverride {
    /// Level to apply.
    pub level: LintLevel,
    /// Lint or group to apply it to.
    pub selector: LintSelector,
}

impl LintOverride {
    /// Creates a new lint override.
    #[must_use]
    pub const fn new(level: LintLevel, selector: LintSelector) -> Self {
        Self { level, selector }
    }
}

/// Options for a lint run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LintOptions {
    overrides: Vec<LintOverride>,
}

impl LintOptions {
    /// Creates lint options from command-line overrides in command-line order.
    #[must_use]
    pub fn new(overrides: Vec<LintOverride>) -> Self {
        Self { overrides }
    }

    /// Returns the configured command-line overrides.
    #[must_use]
    pub fn overrides(&self) -> &[LintOverride] {
        &self.overrides
    }

    /// Resolves the effective level for `lint`.
    #[must_use]
    pub fn level_for(&self, lint: LintId) -> LintLevel {
        let mut level = lint.default_level();

        for lint_override in &self.overrides {
            if !lint_override.selector.contains(lint) {
                continue;
            }
            if level == LintLevel::Forbid {
                continue;
            }
            level = lint_override.level;
        }

        level
    }
}

/// One lint diagnostic.
#[derive(Debug, PartialEq)]
pub struct LintDiagnostic {
    lint: LintId,
    level: LintLevel,
    message: String,
    help: Option<String>,
    location: Option<acdc_parser::SourceLocation>,
}

impl LintDiagnostic {
    /// Creates a lint diagnostic.
    #[must_use]
    pub fn new(lint: LintId, level: LintLevel, message: impl Into<String>) -> Self {
        Self {
            lint,
            level,
            message: message.into(),
            help: None,
            location: None,
        }
    }

    /// Attaches a help message.
    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Attaches a source location.
    #[must_use]
    pub fn at(mut self, location: acdc_parser::SourceLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Returns the lint that emitted this diagnostic.
    #[must_use]
    pub const fn lint(&self) -> LintId {
        self.lint
    }

    /// Returns this diagnostic's effective level.
    #[must_use]
    pub const fn level(&self) -> LintLevel {
        self.level
    }

    /// Returns the diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns optional help text.
    #[must_use]
    pub fn help(&self) -> Option<&str> {
        self.help.as_deref()
    }

    /// Returns this diagnostic's source location, when known.
    #[must_use]
    pub const fn location(&self) -> Option<&acdc_parser::SourceLocation> {
        self.location.as_ref()
    }
}

impl fmt::Display for LintDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}[{}]", self.level, self.lint)?;
        if let Some(location) = &self.location {
            write!(f, " at {location}")?;
        }
        write!(f, ": {}", self.message)?;
        if let Some(help) = &self.help {
            write!(f, "\nhelp: {help}")?;
        }
        Ok(())
    }
}

/// Diagnostics emitted during one lint run.
#[derive(Debug, PartialEq, Default)]
pub struct LintReport {
    diagnostics: Vec<LintDiagnostic>,
}

impl LintReport {
    /// Creates a lint report from diagnostics.
    #[must_use]
    pub fn new(diagnostics: Vec<LintDiagnostic>) -> Self {
        Self { diagnostics }
    }

    /// Returns diagnostics in emission order.
    #[must_use]
    pub fn diagnostics(&self) -> &[LintDiagnostic] {
        &self.diagnostics
    }

    /// Returns whether the lint run emitted no diagnostics.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Returns whether any emitted diagnostic is an error.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level().is_error())
    }
}

/// An input that can be linted as `AsciiDoc`.
///
/// Implemented for filesystem paths and in-memory strings.
pub trait Lintable {
    /// Lints this source with `options`.
    ///
    /// # Errors
    ///
    /// Returns parser errors for invalid `AsciiDoc` input, or I/O errors when
    /// the source must be read from disk.
    fn lint(&self, options: &LintOptions) -> Result<LintReport, Error>;
}

impl Lintable for Path {
    fn lint(&self, options: &LintOptions) -> Result<LintReport, Error> {
        let source = fs::read_to_string(self)?;
        let parsed = acdc_parser::parse_file(self, &acdc_parser::Options::default())?;
        Ok(runner::lint_parsed(
            Some(self.to_path_buf()),
            Some(self),
            &source,
            &parsed,
            options,
        ))
    }
}

impl Lintable for str {
    fn lint(&self, options: &LintOptions) -> Result<LintReport, Error> {
        let parsed = acdc_parser::parse(self, &acdc_parser::Options::default())?;
        Ok(runner::lint_parsed(None, None, self, &parsed, options))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE_SENTENCE: LintSelector = LintSelector::Lint(LintId::OneSentencePerLine);
    const SYMMETRIC_TITLE: LintSelector = LintSelector::Lint(LintId::SectionTitleSymmetricMarker);

    #[test]
    fn parses_lint_names() {
        assert!(matches!(
            "one-sentence-per-line".parse::<LintId>(),
            Ok(LintId::OneSentencePerLine)
        ));
        assert!(matches!(
            "recommended-practices".parse::<LintSelector>(),
            Ok(LintSelector::Group(LintGroup::RecommendedPractices))
        ));
        assert!(matches!(
            "not-real".parse::<LintSelector>(),
            Err(Error::UnknownLintName { name }) if name == "not-real"
        ));
        assert!(matches!(
            "counter-prefix".parse::<LintSelector>(),
            Err(Error::UnknownLintName { name }) if name == "counter-prefix"
        ));
    }

    #[test]
    fn later_overrides_win_for_non_forbid_levels() {
        let options = LintOptions::new(vec![
            LintOverride::new(LintLevel::Deny, ONE_SENTENCE),
            LintOverride::new(LintLevel::Allow, ONE_SENTENCE),
        ]);

        assert_eq!(
            options.level_for(LintId::OneSentencePerLine),
            LintLevel::Allow
        );
        assert_eq!(
            options.level_for(LintId::SectionTitleSymmetricMarker),
            LintLevel::Warn
        );
    }

    #[test]
    fn forbid_cannot_be_lowered() {
        let options = LintOptions::new(vec![
            LintOverride::new(LintLevel::Forbid, ONE_SENTENCE),
            LintOverride::new(LintLevel::Allow, ONE_SENTENCE),
            LintOverride::new(LintLevel::Deny, ONE_SENTENCE),
        ]);

        assert_eq!(
            options.level_for(LintId::OneSentencePerLine),
            LintLevel::Forbid
        );
    }

    #[test]
    fn group_overrides_apply_to_all_registered_lints() {
        let options = LintOptions::new(vec![
            LintOverride::new(LintLevel::Deny, LintSelector::Group(LintGroup::All)),
            LintOverride::new(LintLevel::Allow, SYMMETRIC_TITLE),
        ]);

        assert_eq!(
            options.level_for(LintId::OneSentencePerLine),
            LintLevel::Deny
        );
        assert_eq!(
            options.level_for(LintId::SectionTitleSymmetricMarker),
            LintLevel::Allow
        );
        assert_eq!(
            options.level_for(LintId::SectionTitleSetextStyle),
            LintLevel::Deny
        );
    }

    #[test]
    fn recommended_practices_excludes_opt_in_document_header_lints() {
        let selector = LintSelector::Group(LintGroup::RecommendedPractices);

        assert!(selector.contains(LintId::OneSentencePerLine));
        assert!(selector.contains(LintId::SectionTitleSymmetricMarker));
        assert!(selector.contains(LintId::SectionTitleSetextStyle));
        assert!(!selector.contains(LintId::DocumentTitleAuthor));
        assert!(!selector.contains(LintId::DocumentTitleRevision));
        assert_eq!(
            LintId::DocumentTitleAuthor.default_level(),
            LintLevel::Allow
        );
        assert_eq!(
            LintId::DocumentTitleRevision.default_level(),
            LintLevel::Allow
        );
    }

    #[test]
    fn group_forbid_blocks_later_specific_override() {
        let options = LintOptions::new(vec![
            LintOverride::new(LintLevel::Forbid, LintSelector::Group(LintGroup::All)),
            LintOverride::new(LintLevel::Allow, ONE_SENTENCE),
        ]);

        assert_eq!(
            options.level_for(LintId::OneSentencePerLine),
            LintLevel::Forbid
        );
    }
}
