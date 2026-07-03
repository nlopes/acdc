//! `AsciiDoc` linting primitives.
//!
//! This crate defines the public lint registry, lint levels, and report
//! structures used by `acdc lint`.

use std::{fmt, path::Path, str::FromStr};

mod error;
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
    /// Prefer asymmetric ATX section titles over symmetric ATX or setext titles.
    SectionTitleStyle,
    /// Include an author line immediately after the document title.
    DocumentTitleAuthor,
    /// Include a revision line after the author line.
    DocumentTitleRevision,
    /// Use the minimum required delimiter length for delimited blocks.
    DelimitedBlockMinimalDelimiter,
    /// Use `url-` or `uri-` prefixes for URL-valued attributes.
    AttributeUrlPrefix,
    /// Use `imagesdir` instead of repeating the image directory in each target.
    Imagesdir,
    /// Use asterisk markers for nested unordered lists.
    NestedUnorderedListMarker,
    /// Separate adjacent lists with an empty line comment.
    AdjacentListSeparator,
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

    const fn info(self) -> &'static LintInfo {
        match self {
            Self::DocumentExtension => &LINTS[0],
            Self::OneSentencePerLine => &LINTS[1],
            Self::SectionTitleStyle => &LINTS[2],
            Self::DocumentTitleAuthor => &LINTS[3],
            Self::DocumentTitleRevision => &LINTS[4],
            Self::DelimitedBlockMinimalDelimiter => &LINTS[5],
            Self::AttributeUrlPrefix => &LINTS[6],
            Self::Imagesdir => &LINTS[7],
            Self::NestedUnorderedListMarker => &LINTS[8],
            Self::AdjacentListSeparator => &LINTS[9],
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
        match value {
            "document-extension" => Ok(Self::DocumentExtension),
            "one-sentence-per-line" => Ok(Self::OneSentencePerLine),
            "section-title-style" => Ok(Self::SectionTitleStyle),
            "document-title-author" => Ok(Self::DocumentTitleAuthor),
            "document-title-revision" => Ok(Self::DocumentTitleRevision),
            "delimited-block-minimal-delimiter" => Ok(Self::DelimitedBlockMinimalDelimiter),
            "attribute-url-prefix" => Ok(Self::AttributeUrlPrefix),
            "imagesdir" => Ok(Self::Imagesdir),
            "nested-unordered-list-marker" => Ok(Self::NestedUnorderedListMarker),
            "adjacent-list-separator" => Ok(Self::AdjacentListSeparator),
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

/// The initial lint registry, based on objective rules from Asciidoctor's
/// recommended-practices draft. Draft TODO sections are intentionally omitted.
pub const LINTS: [LintInfo; 10] = [
    LintInfo {
        name: "document-extension",
        id: LintId::DocumentExtension,
        default_level: LintLevel::Warn,
        summary: "prefer .adoc and avoid .asc",
    },
    LintInfo {
        name: "one-sentence-per-line",
        id: LintId::OneSentencePerLine,
        default_level: LintLevel::Warn,
        summary: "write one sentence per source line",
    },
    LintInfo {
        name: "section-title-style",
        id: LintId::SectionTitleStyle,
        default_level: LintLevel::Warn,
        summary: "prefer asymmetric ATX section titles",
    },
    LintInfo {
        name: "document-title-author",
        id: LintId::DocumentTitleAuthor,
        default_level: LintLevel::Warn,
        summary: "include an author line after the document title",
    },
    LintInfo {
        name: "document-title-revision",
        id: LintId::DocumentTitleRevision,
        default_level: LintLevel::Warn,
        summary: "include a revision line after the author line",
    },
    LintInfo {
        name: "delimited-block-minimal-delimiter",
        id: LintId::DelimitedBlockMinimalDelimiter,
        default_level: LintLevel::Warn,
        summary: "use minimum required delimited block fences",
    },
    LintInfo {
        name: "attribute-url-prefix",
        id: LintId::AttributeUrlPrefix,
        default_level: LintLevel::Warn,
        summary: "prefix URL-valued attributes with url- or uri-",
    },
    LintInfo {
        name: "imagesdir",
        id: LintId::Imagesdir,
        default_level: LintLevel::Warn,
        summary: "use imagesdir instead of repeating image directories",
    },
    LintInfo {
        name: "nested-unordered-list-marker",
        id: LintId::NestedUnorderedListMarker,
        default_level: LintLevel::Warn,
        summary: "use asterisk markers for nested unordered lists",
    },
    LintInfo {
        name: "adjacent-list-separator",
        id: LintId::AdjacentListSeparator,
        default_level: LintLevel::Warn,
        summary: "separate adjacent lists with an empty line comment",
    },
];

/// A named group of lints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LintGroup {
    /// Every registered lint.
    All,
    /// The recommended-practices lint set.
    RecommendedPractices,
}

impl LintGroup {
    /// Returns this group's command-line name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::RecommendedPractices => "recommended-practices",
        }
    }

    /// Returns whether this group includes `lint`.
    #[must_use]
    pub fn contains(self, lint: LintId) -> bool {
        match self {
            Self::All => true,
            Self::RecommendedPractices => is_recommended_practice(lint),
        }
    }
}

fn is_recommended_practice(lint: LintId) -> bool {
    matches!(
        lint,
        LintId::DocumentExtension
            | LintId::OneSentencePerLine
            | LintId::SectionTitleStyle
            | LintId::DelimitedBlockMinimalDelimiter
            | LintId::AttributeUrlPrefix
            | LintId::Imagesdir
            | LintId::NestedUnorderedListMarker
            | LintId::AdjacentListSeparator
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
        if let Ok(lint) = LintId::from_str(value) {
            return Ok(Self::Lint(lint));
        }
        LintGroup::from_str(value).map(Self::Group)
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
        write!(f, "{}[{}]: {}", self.level, self.lint, self.message)?;
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

/// Lints a file.
///
/// # Errors
///
/// Will return parser or I/O errors once lint execution is implemented.
pub fn lint_path(path: &Path, options: &LintOptions) -> Result<LintReport, Error> {
    let _ = (path, options);
    todo!("implement AsciiDoc lint execution for files")
}

/// Lints an in-memory `AsciiDoc` source.
///
/// # Errors
///
/// Will return parser errors once lint execution is implemented.
pub fn lint_source(
    name: Option<&str>,
    source: &str,
    options: &LintOptions,
) -> Result<LintReport, Error> {
    let _ = (name, source, options);
    todo!("implement AsciiDoc lint execution for in-memory sources")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE_SENTENCE: LintSelector = LintSelector::Lint(LintId::OneSentencePerLine);
    const SECTIONS: LintSelector = LintSelector::Lint(LintId::SectionTitleStyle);

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
            options.level_for(LintId::SectionTitleStyle),
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
            LintOverride::new(LintLevel::Allow, SECTIONS),
        ]);

        assert_eq!(
            options.level_for(LintId::OneSentencePerLine),
            LintLevel::Deny
        );
        assert_eq!(
            options.level_for(LintId::SectionTitleStyle),
            LintLevel::Allow
        );
    }

    #[test]
    fn recommended_practices_excludes_opt_in_document_header_lints() {
        let selector = LintSelector::Group(LintGroup::RecommendedPractices);

        assert!(selector.contains(LintId::OneSentencePerLine));
        assert!(selector.contains(LintId::SectionTitleStyle));
        assert!(!selector.contains(LintId::DocumentTitleAuthor));
        assert!(!selector.contains(LintId::DocumentTitleRevision));
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
