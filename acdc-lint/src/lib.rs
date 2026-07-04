//! `AsciiDoc` linting primitives.
//!
//! This crate defines the public lint registry, lint levels, and report
//! structures used by `acdc lint`.

use std::{fmt, fs, path::Path, str::FromStr};

mod error;
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

/// A single known lint.
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
    pub const fn name(self) -> &'static str {
        self.info().name
    }

    /// Returns this lint's default level.
    #[must_use]
    pub const fn default_level(self) -> LintLevel {
        self.info().default_level
    }

    /// Returns this lint's short user-facing description.
    #[must_use]
    pub const fn summary(self) -> &'static str {
        self.info().summary
    }

    #[allow(clippy::too_many_lines)]
    const fn info(self) -> LintInfo {
        match self {
            Self::DocumentExtension => lint_info(
                self,
                "document-extension",
                LintLevel::Warn,
                "prefer .adoc and avoid .asc",
            ),
            Self::OneSentencePerLine => lint_info(
                self,
                "one-sentence-per-line",
                LintLevel::Warn,
                "write one sentence per source line",
            ),
            Self::SectionTitleSymmetricMarker => lint_info(
                self,
                "section-title-symmetric-marker",
                LintLevel::Warn,
                "prefer asymmetric ATX section titles",
            ),
            Self::SectionTitleSetextStyle => lint_info(
                self,
                "section-title-setext-style",
                LintLevel::Warn,
                "prefer ATX section titles over setext titles",
            ),
            Self::DocumentTitleAuthor => lint_info(
                self,
                "document-title-author",
                LintLevel::Allow,
                "include an author line after the document title",
            ),
            Self::DocumentTitleRevision => lint_info(
                self,
                "document-title-revision",
                LintLevel::Allow,
                "include a revision line after the author line",
            ),
            Self::SectionLevelSequence => lint_info(
                self,
                "section-level-sequence",
                LintLevel::Warn,
                "do not skip section levels",
            ),
            Self::UnterminatedDelimitedBlock => lint_info(
                self,
                "unterminated-delimited-block",
                LintLevel::Warn,
                "close delimited blocks",
            ),
            Self::UnterminatedTable => lint_info(
                self,
                "unterminated-table",
                LintLevel::Warn,
                "close table blocks",
            ),
            Self::CounterSyntax => lint_info(
                self,
                "counter-syntax",
                LintLevel::Warn,
                "avoid unsupported counter syntax",
            ),
            Self::MultipleDocumentTitle => lint_info(
                self,
                "multiple-document-title",
                LintLevel::Warn,
                "use only one document title",
            ),
            Self::TableUnknownFormat => lint_info(
                self,
                "table-unknown-format",
                LintLevel::Warn,
                "use a supported table format",
            ),
            Self::TableIncompleteRow => lint_info(
                self,
                "table-incomplete-row",
                LintLevel::Warn,
                "complete table rows",
            ),
            Self::TableColumnCount => lint_info(
                self,
                "table-column-count",
                LintLevel::Warn,
                "match table rows to the configured column count",
            ),
            Self::TableCellOverflow => lint_info(
                self,
                "table-cell-overflow",
                LintLevel::Warn,
                "keep table cells within the configured column count",
            ),
            Self::DelimitedBlockMinimalDelimiter => lint_info(
                self,
                "delimited-block-minimal-delimiter",
                LintLevel::Warn,
                "use minimum required delimited block fences",
            ),
            Self::SectionTitleMarkerSpacing => lint_info(
                self,
                "section-title-marker-spacing",
                LintLevel::Warn,
                "put whitespace after section title markers",
            ),
            Self::SectionTitleCapitalization => lint_info(
                self,
                "section-title-capitalization",
                LintLevel::Warn,
                "start section titles with an uppercase letter",
            ),
            Self::DelimitedBlockLeadingBlankLine => lint_info(
                self,
                "delimited-block-leading-blank-line",
                LintLevel::Warn,
                "put a blank line before delimited blocks",
            ),
            Self::DelimitedBlockTrailingBlankLine => lint_info(
                self,
                "delimited-block-trailing-blank-line",
                LintLevel::Warn,
                "put a blank line after delimited blocks",
            ),
            Self::TrailingWhitespace => lint_info(
                self,
                "trailing-whitespace",
                LintLevel::Warn,
                "avoid trailing whitespace",
            ),
            Self::HardTab => lint_info(
                self,
                "hard-tab",
                LintLevel::Warn,
                "avoid hard tab characters",
            ),
            Self::ExcessiveBlankLines => lint_info(
                self,
                "excessive-blank-lines",
                LintLevel::Warn,
                "avoid repeated blank lines",
            ),
            Self::ListMarkerSpacing => lint_info(
                self,
                "list-marker-spacing",
                LintLevel::Warn,
                "put whitespace after list markers",
            ),
            Self::AttributeUrlPrefix => lint_info(
                self,
                "attribute-url-prefix",
                LintLevel::Warn,
                "prefix URL-valued attributes with url- or uri-",
            ),
            Self::Imagesdir => lint_info(
                self,
                "imagesdir",
                LintLevel::Warn,
                "use imagesdir instead of repeating image directories",
            ),
            Self::ImageAltText => lint_info(
                self,
                "image-alt-text",
                LintLevel::Warn,
                "provide image alt text",
            ),
            Self::ImageTargetExists => lint_info(
                self,
                "image-target-exists",
                LintLevel::Warn,
                "reference image files that exist",
            ),
            Self::NestedUnorderedListMarker => lint_info(
                self,
                "nested-unordered-list-marker",
                LintLevel::Warn,
                "use asterisk markers for nested unordered lists",
            ),
            Self::AdjacentListSeparator => lint_info(
                self,
                "adjacent-list-separator",
                LintLevel::Warn,
                "separate adjacent lists with an empty line comment",
            ),
            Self::OrderedListExplicitNumber => lint_info(
                self,
                "ordered-list-explicit-number",
                LintLevel::Warn,
                "prefer dot ordered-list markers over explicit numbers",
            ),
            Self::DescriptionListBoldTerm => lint_info(
                self,
                "description-list-bold-term",
                LintLevel::Warn,
                "prefer description-list syntax over bold-term paragraphs",
            ),
            Self::MarkdownHeading => lint_info(
                self,
                "markdown-heading",
                LintLevel::Warn,
                "avoid Markdown heading syntax",
            ),
            Self::MarkdownLink => lint_info(
                self,
                "markdown-link",
                LintLevel::Warn,
                "avoid Markdown link syntax",
            ),
            Self::MarkdownImage => lint_info(
                self,
                "markdown-image",
                LintLevel::Warn,
                "avoid Markdown image syntax",
            ),
            Self::MarkdownCodeFence => lint_info(
                self,
                "markdown-code-fence",
                LintLevel::Warn,
                "avoid Markdown code fences",
            ),
            Self::MarkdownTable => lint_info(
                self,
                "markdown-table",
                LintLevel::Warn,
                "avoid Markdown table syntax",
            ),
        }
    }
}

const fn lint_info(
    id: LintId,
    name: &'static str,
    default_level: LintLevel,
    summary: &'static str,
) -> LintInfo {
    LintInfo {
        name,
        id,
        default_level,
        summary,
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
        match value {
            "document-extension" => Ok(Self::DocumentExtension),
            "one-sentence-per-line" => Ok(Self::OneSentencePerLine),
            "section-title-symmetric-marker" => Ok(Self::SectionTitleSymmetricMarker),
            "section-title-setext-style" => Ok(Self::SectionTitleSetextStyle),
            "document-title-author" => Ok(Self::DocumentTitleAuthor),
            "document-title-revision" => Ok(Self::DocumentTitleRevision),
            "section-level-sequence" => Ok(Self::SectionLevelSequence),
            "unterminated-delimited-block" => Ok(Self::UnterminatedDelimitedBlock),
            "unterminated-table" => Ok(Self::UnterminatedTable),
            "counter-syntax" => Ok(Self::CounterSyntax),
            "multiple-document-title" => Ok(Self::MultipleDocumentTitle),
            "table-unknown-format" => Ok(Self::TableUnknownFormat),
            "table-incomplete-row" => Ok(Self::TableIncompleteRow),
            "table-column-count" => Ok(Self::TableColumnCount),
            "table-cell-overflow" => Ok(Self::TableCellOverflow),
            "delimited-block-minimal-delimiter" => Ok(Self::DelimitedBlockMinimalDelimiter),
            "section-title-marker-spacing" => Ok(Self::SectionTitleMarkerSpacing),
            "section-title-capitalization" => Ok(Self::SectionTitleCapitalization),
            "delimited-block-leading-blank-line" => Ok(Self::DelimitedBlockLeadingBlankLine),
            "delimited-block-trailing-blank-line" => Ok(Self::DelimitedBlockTrailingBlankLine),
            "trailing-whitespace" => Ok(Self::TrailingWhitespace),
            "hard-tab" => Ok(Self::HardTab),
            "excessive-blank-lines" => Ok(Self::ExcessiveBlankLines),
            "list-marker-spacing" => Ok(Self::ListMarkerSpacing),
            "attribute-url-prefix" => Ok(Self::AttributeUrlPrefix),
            "imagesdir" => Ok(Self::Imagesdir),
            "image-alt-text" => Ok(Self::ImageAltText),
            "image-target-exists" => Ok(Self::ImageTargetExists),
            "nested-unordered-list-marker" => Ok(Self::NestedUnorderedListMarker),
            "adjacent-list-separator" => Ok(Self::AdjacentListSeparator),
            "ordered-list-explicit-number" => Ok(Self::OrderedListExplicitNumber),
            "description-list-bold-term" => Ok(Self::DescriptionListBoldTerm),
            "markdown-heading" => Ok(Self::MarkdownHeading),
            "markdown-link" => Ok(Self::MarkdownLink),
            "markdown-image" => Ok(Self::MarkdownImage),
            "markdown-code-fence" => Ok(Self::MarkdownCodeFence),
            "markdown-table" => Ok(Self::MarkdownTable),
            _ => Err(Error::UnknownLintName {
                name: value.to_owned(),
            }),
        }
    }
}

/// Metadata for one lint.
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
}

/// The known lint registry.
pub const LINTS: [LintInfo; 37] = [
    LintId::DocumentExtension.info(),
    LintId::OneSentencePerLine.info(),
    LintId::SectionTitleSymmetricMarker.info(),
    LintId::SectionTitleSetextStyle.info(),
    LintId::DocumentTitleAuthor.info(),
    LintId::DocumentTitleRevision.info(),
    LintId::SectionLevelSequence.info(),
    LintId::UnterminatedDelimitedBlock.info(),
    LintId::UnterminatedTable.info(),
    LintId::CounterSyntax.info(),
    LintId::MultipleDocumentTitle.info(),
    LintId::TableUnknownFormat.info(),
    LintId::TableIncompleteRow.info(),
    LintId::TableColumnCount.info(),
    LintId::TableCellOverflow.info(),
    LintId::DelimitedBlockMinimalDelimiter.info(),
    LintId::SectionTitleMarkerSpacing.info(),
    LintId::SectionTitleCapitalization.info(),
    LintId::DelimitedBlockLeadingBlankLine.info(),
    LintId::DelimitedBlockTrailingBlankLine.info(),
    LintId::TrailingWhitespace.info(),
    LintId::HardTab.info(),
    LintId::ExcessiveBlankLines.info(),
    LintId::ListMarkerSpacing.info(),
    LintId::AttributeUrlPrefix.info(),
    LintId::Imagesdir.info(),
    LintId::ImageAltText.info(),
    LintId::ImageTargetExists.info(),
    LintId::NestedUnorderedListMarker.info(),
    LintId::AdjacentListSeparator.info(),
    LintId::OrderedListExplicitNumber.info(),
    LintId::DescriptionListBoldTerm.info(),
    LintId::MarkdownHeading.info(),
    LintId::MarkdownLink.info(),
    LintId::MarkdownImage.info(),
    LintId::MarkdownCodeFence.info(),
    LintId::MarkdownTable.info(),
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
