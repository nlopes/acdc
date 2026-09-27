// In Rust 1.92 without this, miette generates warnings about unused assignments.
//
// See issue https://github.com/zkat/miette/issues/458 and PR
// https://github.com/zkat/miette/pull/459 for more details.
#![allow(unused_assignments)]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

#[cfg(any(
    feature = "html",
    feature = "manpage",
    feature = "markdown",
    feature = "pdf",
    feature = "terminal"
))]
use acdc_converters_core::Warning as ConverterWarning;
#[cfg(feature = "lint")]
use acdc_lint::{LintDiagnostic, LintLevel};
use acdc_parser::{SourceLocation, Warning as ParserWarning};
use miette::{Diagnostic, NamedSource, Report, SourceSpan};
#[cfg(feature = "lint")]
use miette::{LabeledSpan, Severity};

/// An error with its source snippet.
#[cfg(any(
    feature = "html",
    feature = "manpage",
    feature = "markdown",
    feature = "pdf",
    feature = "terminal"
))]
#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("{message}")]
#[diagnostic()]
pub(crate) struct RichError {
    message: String,

    #[help]
    advice: Option<&'static str>,

    #[source_code]
    src: NamedSource<String>,

    #[label("{position_advice}")]
    span: SourceSpan,
    position_advice: String,
}

#[cfg(feature = "execute")]
#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("{error}")]
pub(crate) struct LocatedError<E: std::error::Error + Send + Sync + 'static> {
    #[source]
    error: E,
    #[related]
    locations: Vec<SourceContext>,
}

#[cfg(feature = "execute")]
impl<E: std::error::Error + Send + Sync + 'static> LocatedError<E> {
    pub(crate) fn new(error: E, locations: impl IntoIterator<Item = SourceLocation>) -> Self {
        Self {
            error,
            locations: locations
                .into_iter()
                .map(|location| SourceContext::new(&location))
                .collect(),
        }
    }

    pub(crate) fn with_cached_source(
        error: E,
        location: &SourceLocation,
        sources: &mut SourceCache,
    ) -> Self {
        let content = location.file.as_deref().and_then(|path| sources.load(path));
        Self {
            error,
            locations: vec![SourceContext::with_content(location, content)],
        }
    }

    pub(crate) fn without_source(error: E, location: &SourceLocation) -> Self {
        Self {
            error,
            locations: vec![SourceContext::with_content(location, None)],
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct SourceCache {
    sources: HashMap<PathBuf, Option<Arc<String>>>,
}

impl SourceCache {
    fn load(&mut self, path: &Path) -> Option<Arc<String>> {
        if let Some(source) = self.sources.get(path) {
            return source.clone();
        }
        let source = std::fs::read_to_string(path).ok().map(Arc::new);
        self.sources.insert(path.to_owned(), source.clone());
        source
    }
}

#[cfg(feature = "execute")]
#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("{description}")]
struct SourceContext {
    description: String,
    #[source_code]
    src: Option<NamedSource<Arc<String>>>,
    #[label("command source")]
    span: Option<SourceSpan>,
}

#[cfg(feature = "execute")]
impl SourceContext {
    fn new(location: &SourceLocation) -> Self {
        let content = location
            .file
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(Arc::new);
        Self::with_content(location, content)
    }

    fn with_content(location: &SourceLocation, content: Option<Arc<String>>) -> Self {
        let (line, column) = source_location_line_column(location);
        let mut description = match &location.file {
            Some(path) => format!("{}:{line}:{column}", path.display()),
            None => format!("line {line}, column {column}"),
        };
        if let Some(chain) = location
            .location
            .start
            .file
            .as_deref()
            .filter(|chain| !chain.is_empty())
        {
            description.push_str(" (include chain: ");
            description.push_str(&chain.join(" -> "));
            description.push(')');
        }
        let span = content
            .as_deref()
            .map(|source| source_span_from_source_location(location, source));
        let src = content.map(|source| NamedSource::new(description.clone(), source));
        Self {
            description,
            src,
            span,
        }
    }
}

/// Rich warning wrapper: same shape as `RichError` but with `severity = Warning` so
/// miette renders it in the warning palette (yellow, `⚠`) rather than the error palette
/// (red, `✗`).
#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("{message}")]
#[diagnostic(severity(Warning))]
pub(crate) struct RichWarning {
    message: String,

    #[help]
    advice: Option<String>,

    #[source_code]
    src: NamedSource<Arc<String>>,

    #[label("{position_advice}")]
    span: SourceSpan,
    position_advice: String,
}

/// Minimal warning with no attached source span. Used when the warning has no location,
/// or when we can't read the source file to build a rich span.
#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("{message}")]
#[diagnostic(severity(Warning))]
pub(crate) struct PlainWarning {
    message: String,

    #[help]
    advice: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct WarningReportContext<'a> {
    file: Option<&'a Path>,
    sources: SourceCache,
}

impl<'a> WarningReportContext<'a> {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[cfg(any(
        feature = "html",
        feature = "manpage",
        feature = "markdown",
        feature = "pdf",
        feature = "terminal"
    ))]
    pub(crate) const fn with_optional_file(mut self, file: Option<&'a Path>) -> Self {
        self.file = file;
        self
    }
}

pub(crate) trait WarningReport {
    fn to_report(&self, context: &mut WarningReportContext<'_>) -> Report;
}

#[cfg(feature = "lint")]
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub(crate) struct FullLintDiagnostic {
    message: String,
    code: String,
    severity: Severity,
    advice: Option<String>,
    src: NamedSource<String>,
    span: SourceSpan,
    position_advice: String,
}

#[cfg(feature = "lint")]
impl Diagnostic for FullLintDiagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(&self.code))
    }

    fn severity(&self) -> Option<Severity> {
        Some(self.severity)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.advice
            .as_ref()
            .map(|help| Box::new(help) as Box<dyn std::fmt::Display + 'a>)
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.src)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(std::iter::once(
            LabeledSpan::new_primary_with_span(Some(self.position_advice.clone()), self.span),
        )))
    }
}

#[cfg(feature = "lint")]
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub(crate) struct CompactLintDiagnostic {
    message: String,
    code: String,
    severity: Severity,
    advice: Option<String>,
}

#[cfg(feature = "lint")]
#[derive(Debug, Clone, Copy)]
pub(crate) struct LintDiagnosticReportContext<'a> {
    file: Option<&'a Path>,
    source_name: Option<&'a str>,
    source: Option<&'a str>,
}

#[cfg(feature = "lint")]
impl<'a> LintDiagnosticReportContext<'a> {
    pub(crate) const fn new() -> Self {
        Self {
            file: None,
            source_name: None,
            source: None,
        }
    }

    pub(crate) const fn with_optional_file(mut self, file: Option<&'a Path>) -> Self {
        self.file = file;
        self
    }

    pub(crate) const fn with_optional_source_name(mut self, source_name: Option<&'a str>) -> Self {
        self.source_name = source_name;
        self
    }

    pub(crate) const fn with_optional_source(mut self, source: Option<&'a str>) -> Self {
        self.source = source;
        self
    }
}

#[cfg(feature = "lint")]
pub(crate) trait LintDiagnosticReport {
    fn to_report(&self, context: LintDiagnosticReportContext<'_>) -> Report;
}

#[cfg(feature = "lint")]
impl Diagnostic for CompactLintDiagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(&self.code))
    }

    fn severity(&self) -> Option<Severity> {
        Some(self.severity)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.advice
            .as_ref()
            .map(|help| Box::new(help) as Box<dyn std::fmt::Display + 'a>)
    }
}

fn source_span_from_source_location(loc: &SourceLocation, source: &str) -> SourceSpan {
    // Parser locations are original-source-relative. Resolve their Unicode-scalar
    // line/column positions back to byte offsets in the source miette will render.
    let location = &loc.location;
    let start_offset = offset_from_position(source, location.start.line, location.start.column);
    let end_offset = if location.start.file == location.end.file {
        offset_from_position(source, location.end.line, location.end.column).max(start_offset)
    } else {
        start_offset
    };
    let end_exclusive = source
        .get(end_offset..)
        .and_then(|remainder| remainder.chars().next())
        .map_or(end_offset, |character| end_offset + character.len_utf8());
    let length = end_exclusive.saturating_sub(start_offset);
    SourceSpan::new(start_offset.into(), length)
}

fn source_location_line_column(loc: &SourceLocation) -> (u32, u32) {
    (loc.location.start.line, loc.location.start.column)
}

/// Resolve 1-indexed line and Unicode-scalar column numbers to a byte offset.
fn offset_from_position(source: &str, line: u32, column: u32) -> usize {
    let mut current_line = 1_u32;
    let mut line_start = 0;
    for source_line in source.split_inclusive('\n') {
        if current_line == line {
            let column_index = column.saturating_sub(1);
            let mut current_column = 0_u32;
            for (byte_offset, character) in source_line.char_indices() {
                if current_column == column_index {
                    return line_start + byte_offset;
                }
                if character == '\n' {
                    return line_start + byte_offset;
                }
                current_column = current_column.saturating_add(1);
            }
            return line_start + source_line.len();
        }
        line_start += source_line.len();
        current_line = current_line.saturating_add(1);
    }

    source.len()
}

/// Attach a source snippet when the warning's file or the context's fallback is readable.
/// Warnings in one context share source text; missing files remain source-less.
fn build_warning_report(
    message: String,
    advice: Option<String>,
    location: Option<&SourceLocation>,
    context: &mut WarningReportContext<'_>,
) -> Report {
    let rich = location.and_then(|loc| {
        let path = loc.file.as_deref().or(context.file)?;
        let source_str = context.sources.load(path)?;
        let span = source_span_from_source_location(loc, &source_str);
        let (line, column) = source_location_line_column(loc);
        Some(RichWarning {
            message: message.clone(),
            advice: advice.clone(),
            src: NamedSource::new(path.display().to_string(), source_str),
            span,
            position_advice: format!("warning occurred here (line {line}, column {column})"),
        })
    });

    match rich {
        Some(rich) => Report::new(rich),
        None => Report::new(PlainWarning { message, advice }),
    }
}

impl WarningReport for ParserWarning {
    fn to_report(&self, context: &mut WarningReportContext<'_>) -> Report {
        build_warning_report(
            self.kind.to_string(),
            self.advice().map(str::to_string),
            self.source_location(),
            context,
        )
    }
}

#[cfg(any(
    feature = "html",
    feature = "manpage",
    feature = "markdown",
    feature = "pdf",
    feature = "terminal"
))]
impl WarningReport for ConverterWarning {
    fn to_report(&self, context: &mut WarningReportContext<'_>) -> Report {
        build_warning_report(
            self.to_string(),
            self.advice().map(str::to_string),
            self.source_location(),
            context,
        )
    }
}

#[cfg(feature = "lint")]
fn severity_for_lint_level(level: LintLevel) -> Severity {
    match level {
        LintLevel::Allow | LintLevel::Warn => Severity::Warning,
        LintLevel::Deny | LintLevel::Forbid => Severity::Error,
    }
}

#[cfg(feature = "lint")]
impl LintDiagnosticReport for LintDiagnostic {
    fn to_report(&self, context: LintDiagnosticReportContext<'_>) -> Report {
        let message = self.message().to_owned();
        let code = self.lint().name().to_owned();
        let severity = severity_for_lint_level(self.level());
        let advice = self.help().map(str::to_string);

        let full = self.location().and_then(|loc| {
            let location_file = loc.file.as_deref().or(context.file);
            let context_source_matches = context.source.filter(|_| match location_file {
                Some(path) => context.file == Some(path),
                None => true,
            });
            let (name, source_str) = match (location_file, context_source_matches) {
                (Some(path), Some(source)) => (path.display().to_string(), source.to_owned()),
                (Some(path), None) => (
                    path.display().to_string(),
                    std::fs::read_to_string(path).ok()?,
                ),
                (None, Some(source)) => (
                    context.source_name.unwrap_or("<stdin>").to_owned(),
                    source.to_owned(),
                ),
                (None, None) => return None,
            };
            let span = source_span_from_source_location(loc, &source_str);
            let (line, column) = source_location_line_column(loc);
            Some(FullLintDiagnostic {
                message: message.clone(),
                code: code.clone(),
                severity,
                advice: advice.clone(),
                src: NamedSource::new(name, source_str),
                span,
                position_advice: format!(
                    "{} triggered here (line {line}, column {column})",
                    self.lint()
                ),
            })
        });

        match full {
            Some(full) => Report::new(full),
            None => Report::new(CompactLintDiagnostic {
                message,
                code,
                severity,
                advice,
            }),
        }
    }
}

#[cfg(any(
    feature = "html",
    feature = "manpage",
    feature = "markdown",
    feature = "pdf",
    feature = "terminal"
))]
pub(crate) fn display<E: std::error::Error + 'static>(e: &E) -> Report {
    if let Some(parser_error) = acdc_converters_core::find_parser_error(e)
        && let Some(source_location) = parser_error.source_location()
        && let Some(path) = &source_location.file
        /* Lazy-load file content only if we have a file path */
        /*
           NOTE: this might be an issue for very large files, but in practice we expect the source
           files to be reasonably sized, and this allows us to show rich snippets for errors that
           have file paths but no source content attached. I might regret leaving this as is.
        */
        && let Ok(source_str) = std::fs::read_to_string(path)
    {
        let advice = parser_error.advice();
        let span = source_span_from_source_location(source_location, &source_str);
        let named_source = NamedSource::new(path.display().to_string(), source_str);
        let (line, column) = source_location_line_column(source_location);
        let position_advice = format!("error occurred here (line {line}, column {column})");
        let rich_error = RichError {
            message: parser_error.to_string(),
            advice,
            src: named_source,
            span,
            position_advice,
        };
        return Report::new(rich_error);
    }

    // Fallback: no parser error with a readable source location was found.
    Report::msg(e.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use acdc_parser::{Location, Position, SourceLocation};

    use super::*;

    #[test]
    fn warning_reports_reuse_the_first_source_snapshot() -> Result<(), Box<dyn std::error::Error>> {
        let file = tempfile::NamedTempFile::new()?;
        std::fs::write(file.path(), "original source\n")?;
        let location =
            SourceLocation::at_position(Some(file.path().to_owned()), Position::new(1, 1));
        let mut context = WarningReportContext::new();
        let first = build_warning_report("first".into(), None, Some(&location), &mut context);
        std::fs::write(file.path(), "changed source\n")?;
        let second = build_warning_report("second".into(), None, Some(&location), &mut context);
        let first = first
            .downcast_ref::<RichWarning>()
            .ok_or("missing first source")?;
        let second = second
            .downcast_ref::<RichWarning>()
            .ok_or("missing second source")?;
        assert_eq!(second.src.inner().as_str(), "original source\n");
        assert!(Arc::ptr_eq(first.src.inner(), second.src.inner()));
        Ok(())
    }

    #[test]
    fn warning_reports_cache_unreadable_sources() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("missing.adoc");
        let location = SourceLocation::at_position(Some(path.clone()), Position::new(1, 1));
        let mut context = WarningReportContext::new();
        let first = build_warning_report("first".into(), None, Some(&location), &mut context);
        std::fs::write(path, "created later\n")?;
        let second = build_warning_report("second".into(), None, Some(&location), &mut context);
        assert!(first.downcast_ref::<PlainWarning>().is_some());
        assert!(second.downcast_ref::<PlainWarning>().is_some());
        Ok(())
    }

    #[test]
    fn resolves_unicode_columns_to_byte_offsets() {
        assert_eq!(offset_from_position("éx\nnext", 1, 1), 0);
        assert_eq!(offset_from_position("éx\nnext", 1, 2), 2);
        assert_eq!(offset_from_position("éx\nnext", 2, 1), 4);
    }

    #[test]
    fn clamps_missing_positions_to_source_end() {
        let source = "é";
        let offset = offset_from_position(source, 99, 99);

        assert_eq!(offset, source.len());
        assert!(source.is_char_boundary(offset));
    }

    #[test]
    fn creates_empty_span_for_empty_source() {
        let location = SourceLocation::at_position(None, Position::new(1, 1));
        let span = source_span_from_source_location(&location, "");

        assert_eq!(span.offset(), 0);
        assert!(span.is_empty());
    }

    #[test]
    fn spans_a_complete_unicode_scalar() {
        let location = SourceLocation::at_position(None, Position::new(1, 1));
        let span = source_span_from_source_location(&location, "éx");

        assert_eq!(span.offset(), 0);
        assert_eq!(span.len(), "é".len());
    }

    #[test]
    fn included_file_positions_use_their_source_coordinates() {
        let chain = Arc::new(vec!["included.adoc".to_owned()]);
        let mut start = Position::new(2, 1);
        start.file = Some(Arc::clone(&chain));
        let mut end = Position::new(2, 2);
        end.file = Some(chain);
        let mut location = Location::point(start);
        location.end = end;
        let location = SourceLocation::at_location(None, location);
        let span = source_span_from_source_location(&location, "first\néx\n");

        assert_eq!(span.offset(), 6);
        assert_eq!(span.len(), 3);
    }

    #[cfg(feature = "execute")]
    #[test]
    fn unresolved_command_sources_keep_the_include_chain() {
        let mut position = Position::new(7, 3);
        position.file = Some(Arc::new(vec![
            "outer.adoc".into(),
            "nested/inner.adoc".into(),
        ]));
        let context = SourceContext::new(&SourceLocation::at_position(None, position));

        assert_eq!(
            context.description,
            "line 7, column 3 (include chain: outer.adoc -> nested/inner.adoc)"
        );
        assert!(context.src.is_none());
        assert!(context.span.is_none());
    }

    #[cfg(feature = "execute")]
    #[test]
    fn skipped_command_diagnostics_do_not_load_source_text()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = tempfile::NamedTempFile::new()?;
        std::fs::write(file.path(), "source must not be retained")?;
        let location =
            SourceLocation::at_position(Some(file.path().to_owned()), Position::new(1, 1));
        let report = LocatedError::without_source(std::io::Error::other("skipped"), &location);
        let context = report
            .locations
            .first()
            .ok_or("missing source coordinates")?;

        assert!(
            context
                .description
                .contains(file.path().to_string_lossy().as_ref())
        );
        assert!(context.src.is_none());
        assert!(context.span.is_none());
        Ok(())
    }

    #[cfg(feature = "execute")]
    #[test]
    fn execution_failures_share_one_source_allocation_per_file()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = tempfile::NamedTempFile::new()?;
        std::fs::write(file.path(), "first\nsecond\n")?;
        let location =
            SourceLocation::at_position(Some(file.path().to_owned()), Position::new(1, 1));
        let mut cache = SourceCache::default();
        let first =
            LocatedError::with_cached_source(std::io::Error::other("first"), &location, &mut cache);
        let second = LocatedError::with_cached_source(
            std::io::Error::other("second"),
            &location,
            &mut cache,
        );
        let first = first
            .locations
            .first()
            .and_then(|context| context.src.as_ref())
            .ok_or("missing first source")?;
        let second = second
            .locations
            .first()
            .and_then(|context| context.src.as_ref())
            .ok_or("missing second source")?;

        assert!(Arc::ptr_eq(first.inner(), second.inner()));
        assert_eq!(cache.sources.len(), 1);
        Ok(())
    }
}
