use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
};

use acdc_lint::{
    LintId, LintLevel, LintOptions, LintOverride, LintOverrideSelector, LintReport, Lintable,
};
use clap::{ArgAction, ArgMatches, Args as ClapArgs, ValueEnum};

use crate::error::{LintDiagnosticReport, LintDiagnosticReportContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputStyle {
    /// Full diagnostics with color, labels, source snippets, and help text
    Full,
    /// Compact diagnostics without colors, symbols, or source snippets
    Compact,
}

impl OutputStyle {
    pub const fn is_full(self) -> bool {
        matches!(self, Self::Full)
    }
}

/// Lint `AsciiDoc` documents
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Input from stdin
    #[arg(long, conflicts_with = "files")]
    pub stdin: bool,

    /// List of files to lint
    #[arg(conflicts_with = "stdin", required_unless_present = "stdin")]
    pub files: Vec<PathBuf>,

    /// Suppress a lint or lint group, optionally scoped as LINT@LOCATION[,LOCATION]...
    #[arg(
        short = 'A',
        long = "allow",
        value_name = "LINT[@LOCATION[,LOCATION]...]",
        value_parser = clap::value_parser!(LintOverrideSelector),
        action = ArgAction::Append
    )]
    pub allow: Vec<LintOverrideSelector>,

    /// Warn on a lint or lint group, optionally scoped as LINT@LOCATION[,LOCATION]...
    #[arg(
        short = 'W',
        long = "warn",
        value_name = "LINT[@LOCATION[,LOCATION]...]",
        value_parser = clap::value_parser!(LintOverrideSelector),
        action = ArgAction::Append
    )]
    pub warn: Vec<LintOverrideSelector>,

    /// Deny a lint or lint group, optionally scoped as LINT@LOCATION[,LOCATION]...
    #[arg(
        short = 'D',
        long = "deny",
        value_name = "LINT[@LOCATION[,LOCATION]...]",
        value_parser = clap::value_parser!(LintOverrideSelector),
        action = ArgAction::Append
    )]
    pub deny: Vec<LintOverrideSelector>,

    /// Forbid a lint or lint group so later flags cannot lower it, optionally scoped as LINT@LOCATION[,LOCATION]...
    #[arg(
        short = 'F',
        long = "forbid",
        value_name = "LINT[@LOCATION[,LOCATION]...]",
        value_parser = clap::value_parser!(LintOverrideSelector),
        action = ArgAction::Append
    )]
    pub forbid: Vec<LintOverrideSelector>,

    /// Diagnostic output style
    #[arg(long = "output-style", value_enum, default_value = "full")]
    pub output_style: OutputStyle,
}

pub fn run(args: &Args, matches: &ArgMatches) -> miette::Result<()> {
    let options = LintOptions::new(ordered_overrides(matches)?);

    if args.stdin {
        let mut source = String::new();
        std::io::stdin()
            .read_to_string(&mut source)
            .map_err(|error| miette::miette!("failed to read stdin: {error}"))?;
        let report = source
            .lint(&options)
            .map_err(|error| miette::miette!("lint failed: {error}"))?;
        report.render_with_stats(
            LintReportRenderContext::new(args.output_style).with_source("<stdin>", &source),
        );
        return if report.has_errors() {
            Err(miette::miette!(
                "lint diagnostics denied by configured lint levels"
            ))
        } else {
            Ok(())
        };
    }

    if args.files.is_empty() {
        return Err(miette::miette!(
            "lint requires at least one file, or --stdin to read from standard input"
        ));
    }

    let mut failed = false;
    let mut stats = LintStats::default();
    for file in &args.files {
        let report = file
            .lint(&options)
            .map_err(|error| miette::miette!("lint failed for {}: {error}", file.display()))?;
        report.render(LintReportRenderContext::new(args.output_style).with_file(file));
        stats.record_report(&report);
        failed |= report.has_errors();
    }
    stats.render(args.output_style);

    if failed {
        Err(miette::miette!(
            "lint diagnostics denied by configured lint levels"
        ))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct LintReportRenderContext<'a> {
    output_style: OutputStyle,
    file: Option<&'a Path>,
    source_name: Option<&'a str>,
    source: Option<&'a str>,
}

impl<'a> LintReportRenderContext<'a> {
    const fn new(output_style: OutputStyle) -> Self {
        Self {
            output_style,
            file: None,
            source_name: None,
            source: None,
        }
    }

    const fn with_file(mut self, file: &'a Path) -> Self {
        self.file = Some(file);
        self
    }

    const fn with_source(mut self, source_name: &'a str, source: &'a str) -> Self {
        self.source_name = Some(source_name);
        self.source = Some(source);
        self
    }
}

trait ReportRenderer {
    fn render(&self, context: LintReportRenderContext<'_>);
    fn render_with_stats(&self, context: LintReportRenderContext<'_>);
    fn render_full(&self, context: LintReportRenderContext<'_>);
    fn render_compact(&self, context: LintReportRenderContext<'_>);
}

impl ReportRenderer for LintReport {
    fn render(&self, context: LintReportRenderContext<'_>) {
        if context.output_style.is_full() {
            self.render_full(context);
        } else {
            self.render_compact(context);
        }
    }

    fn render_with_stats(&self, context: LintReportRenderContext<'_>) {
        self.render(context);
        let mut stats = LintStats::default();
        stats.record_report(self);
        stats.render(context.output_style);
    }

    fn render_full(&self, context: LintReportRenderContext<'_>) {
        let loaded_source = context
            .file
            .and_then(|file| std::fs::read_to_string(file).ok());
        let context = LintDiagnosticReportContext::new()
            .with_optional_file(context.file)
            .with_optional_source_name(context.source_name)
            .with_optional_source(context.source.or(loaded_source.as_deref()));
        for diagnostic in self.diagnostics() {
            eprintln!("{:?}", diagnostic.to_report(context));
        }
    }

    fn render_compact(&self, context: LintReportRenderContext<'_>) {
        for diagnostic in self.diagnostics() {
            let location = diagnostic
                .location()
                .map_or_else(String::new, compact_location);
            if let Some(path) = context.file {
                eprintln!(
                    "{}: {}[{}]{location}: {}",
                    path.display(),
                    diagnostic.level(),
                    diagnostic.lint(),
                    diagnostic.message()
                );
            } else {
                eprintln!(
                    "{}[{}]{location}: {}",
                    diagnostic.level(),
                    diagnostic.lint(),
                    diagnostic.message()
                );
            }
            if let Some(help) = diagnostic.help() {
                eprintln!("help: {help}");
            }
        }
    }
}

fn compact_location(location: &acdc_parser::SourceLocation) -> String {
    let start = &location.location.start;
    let end = &location.location.end;
    if start == end {
        format!(" at {}:{}", start.line, start.column)
    } else {
        format!(
            " at {}:{}, {}:{}",
            start.line, start.column, end.line, end.column
        )
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct LintStats {
    counts: HashMap<LintId, usize>,
    total: usize,
}

impl LintStats {
    fn record_report(&mut self, report: &LintReport) {
        for diagnostic in report.diagnostics() {
            *self.counts.entry(diagnostic.lint()).or_default() += 1;
            self.total += 1;
        }
    }

    const fn is_empty(&self) -> bool {
        self.total == 0
    }

    fn sorted_counts(&self) -> Vec<(LintId, usize)> {
        let mut counts = self
            .counts
            .iter()
            .map(|(lint, count)| (*lint, *count))
            .collect::<Vec<_>>();
        counts.sort_by(|(left_lint, left_count), (right_lint, right_count)| {
            right_count
                .cmp(left_count)
                .then_with(|| left_lint.name().cmp(right_lint.name()))
        });
        counts
    }

    fn render(&self, output_style: OutputStyle) {
        if output_style.is_full() {
            self.render_full();
        }
    }

    fn render_full(&self) {
        if self.is_empty() {
            return;
        }

        eprintln!("\nlint stats:");
        for (lint, count) in self.sorted_counts() {
            eprintln!("  {count:>4} {lint}");
        }
        eprintln!("  {:>4} total diagnostics", self.total);
    }
}

fn ordered_overrides(matches: &ArgMatches) -> miette::Result<Vec<LintOverride>> {
    let mut indexed = Vec::new();
    collect_overrides(matches, "allow", LintLevel::Allow, &mut indexed)?;
    collect_overrides(matches, "warn", LintLevel::Warn, &mut indexed)?;
    collect_overrides(matches, "deny", LintLevel::Deny, &mut indexed)?;
    collect_overrides(matches, "forbid", LintLevel::Forbid, &mut indexed)?;

    indexed.sort_by_key(|(index, _)| *index);
    Ok(indexed
        .into_iter()
        .map(|(_, lint_override)| lint_override)
        .collect())
}

fn collect_overrides(
    matches: &ArgMatches,
    id: &'static str,
    level: LintLevel,
    indexed: &mut Vec<(usize, LintOverride)>,
) -> miette::Result<()> {
    let Some(indices) = matches.indices_of(id) else {
        return Ok(());
    };
    let Some(selectors) = matches.get_many::<LintOverrideSelector>(id) else {
        return Err(miette::miette!(
            "internal error: missing values for lint flag `{id}`"
        ));
    };

    for (index, selector) in indices.zip(selectors) {
        for lint_override in selector.clone().into_overrides(level) {
            indexed.push((index, lint_override));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use acdc_lint::{LintDiagnostic, LintId, LintSelector, LintSourcePosition, LintSourceRange};
    use clap::CommandFactory;

    use super::*;
    use crate::Cli;

    fn lint_matches<const N: usize>(args: [&str; N]) -> miette::Result<ArgMatches> {
        let matches = Cli::command().get_matches_from(args);
        match matches.subcommand() {
            Some(("lint", lint_matches)) => Ok(lint_matches.clone()),
            _ => Err(miette::miette!("test command must select lint")),
        }
    }

    fn output_style(matches: &ArgMatches) -> miette::Result<OutputStyle> {
        matches
            .get_one::<OutputStyle>("output_style")
            .copied()
            .ok_or_else(|| miette::miette!("missing output_style"))
    }

    #[test]
    fn preserves_lint_level_flag_order() -> miette::Result<()> {
        let matches = lint_matches([
            "acdc",
            "lint",
            "-D",
            "one-sentence-per-line",
            "-A",
            "one-sentence-per-line",
            "doc.adoc",
        ])?;
        let overrides = ordered_overrides(&matches)?;

        assert_eq!(
            overrides,
            vec![
                LintOverride::new(
                    LintLevel::Deny,
                    LintSelector::Lint(acdc_lint::LintId::OneSentencePerLine),
                ),
                LintOverride::new(
                    LintLevel::Allow,
                    LintSelector::Lint(acdc_lint::LintId::OneSentencePerLine),
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn parses_location_scoped_lint_level_flags() -> miette::Result<()> {
        let matches = lint_matches([
            "acdc",
            "lint",
            "-A",
            "section-title-capitalization@37",
            "doc.adoc",
        ])?;
        let overrides = ordered_overrides(&matches)?;

        assert_eq!(
            overrides,
            vec![LintOverride::with_location(
                LintLevel::Allow,
                LintSelector::Lint(LintId::SectionTitleCapitalization),
                LintSourceRange::point(LintSourcePosition::new(37, None)),
            )]
        );
        Ok(())
    }

    #[test]
    fn expands_comma_separated_lint_location_flags() -> miette::Result<()> {
        let matches = lint_matches([
            "acdc",
            "lint",
            "--allow",
            "delimited-block-minimal-delimiter@977,968",
            "doc.adoc",
        ])?;
        let overrides = ordered_overrides(&matches)?;

        assert_eq!(
            overrides,
            vec![
                LintOverride::with_location(
                    LintLevel::Allow,
                    LintSelector::Lint(LintId::DelimitedBlockMinimalDelimiter),
                    LintSourceRange::point(LintSourcePosition::new(977, None)),
                ),
                LintOverride::with_location(
                    LintLevel::Allow,
                    LintSelector::Lint(LintId::DelimitedBlockMinimalDelimiter),
                    LintSourceRange::point(LintSourcePosition::new(968, None)),
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn lint_stats_count_lints_in_frequency_order() {
        let report = LintReport::new(vec![
            LintDiagnostic::new(
                LintId::SectionTitleCapitalization,
                LintLevel::Warn,
                "lowercase title",
            ),
            LintDiagnostic::new(
                LintId::OneSentencePerLine,
                LintLevel::Warn,
                "multiple sentences",
            ),
            LintDiagnostic::new(
                LintId::SectionTitleCapitalization,
                LintLevel::Warn,
                "lowercase title",
            ),
        ]);
        let mut stats = LintStats::default();
        stats.record_report(&report);

        assert_eq!(stats.total, 3);
        assert_eq!(
            stats.sorted_counts(),
            vec![
                (LintId::SectionTitleCapitalization, 2),
                (LintId::OneSentencePerLine, 1),
            ]
        );
    }

    #[test]
    fn lint_stats_sort_equal_counts_by_lint_name() {
        let report = LintReport::new(vec![
            LintDiagnostic::new(LintId::TrailingWhitespace, LintLevel::Warn, "trailing"),
            LintDiagnostic::new(LintId::HardTab, LintLevel::Warn, "tab"),
        ]);
        let mut stats = LintStats::default();
        stats.record_report(&report);

        assert_eq!(
            stats.sorted_counts(),
            vec![(LintId::HardTab, 1), (LintId::TrailingWhitespace, 1)]
        );
    }

    #[test]
    fn parses_lint_output_styles() -> miette::Result<()> {
        assert_eq!(
            output_style(&lint_matches([
                "acdc",
                "lint",
                "--output-style",
                "full",
                "doc.adoc"
            ])?)?,
            OutputStyle::Full
        );
        assert_eq!(
            output_style(&lint_matches([
                "acdc",
                "lint",
                "--output-style",
                "compact",
                "doc.adoc"
            ])?)?,
            OutputStyle::Compact
        );
        Ok(())
    }

    #[test]
    fn formats_compact_locations() {
        let point =
            acdc_parser::SourceLocation::at_position(None, acdc_parser::Position::new(3, 1));
        assert_eq!(compact_location(&point), " at 3:1");

        let mut span = acdc_parser::Location::point(acdc_parser::Position::new(1, 3));
        span.end = acdc_parser::Position::new(1, 36);
        let span = acdc_parser::SourceLocation::at_location(None, span);
        assert_eq!(compact_location(&span), " at 1:3, 1:36");
    }
}
