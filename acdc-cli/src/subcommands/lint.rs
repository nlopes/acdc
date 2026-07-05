use std::{
    io::Read,
    path::{Path, PathBuf},
};

use acdc_lint::{LintLevel, LintOptions, LintOverride, LintReport, LintSelector, Lintable};
use clap::{ArgAction, ArgMatches, Args as ClapArgs, ValueEnum};

use crate::error;

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
    #[arg(conflicts_with = "stdin")]
    pub files: Vec<PathBuf>,

    /// Suppress a lint or lint group
    #[arg(
        short = 'A',
        long = "allow",
        value_name = "LINT",
        value_parser = clap::value_parser!(LintSelector),
        action = ArgAction::Append
    )]
    pub allow: Vec<LintSelector>,

    /// Warn on a lint or lint group
    #[arg(
        short = 'W',
        long = "warn",
        value_name = "LINT",
        value_parser = clap::value_parser!(LintSelector),
        action = ArgAction::Append
    )]
    pub warn: Vec<LintSelector>,

    /// Deny a lint or lint group
    #[arg(
        short = 'D',
        long = "deny",
        value_name = "LINT",
        value_parser = clap::value_parser!(LintSelector),
        action = ArgAction::Append
    )]
    pub deny: Vec<LintSelector>,

    /// Forbid a lint or lint group so later flags cannot lower it
    #[arg(
        short = 'F',
        long = "forbid",
        value_name = "LINT",
        value_parser = clap::value_parser!(LintSelector),
        action = ArgAction::Append
    )]
    pub forbid: Vec<LintSelector>,

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
        return finish_report(
            None,
            Some("<stdin>"),
            Some(&source),
            &report,
            args.output_style,
        );
    }

    if args.files.is_empty() {
        return Err(miette::miette!(
            "lint requires at least one file, or --stdin to read from standard input"
        ));
    }

    let mut failed = false;
    for file in &args.files {
        let report = file
            .lint(&options)
            .map_err(|error| miette::miette!("lint failed for {}: {error}", file.display()))?;
        render_report(Some(file), None, None, &report, args.output_style);
        failed |= report.has_errors();
    }

    if failed {
        Err(miette::miette!(
            "lint diagnostics denied by configured lint levels"
        ))
    } else {
        Ok(())
    }
}

fn finish_report(
    file: Option<&Path>,
    source_name: Option<&str>,
    source: Option<&str>,
    report: &LintReport,
    output_style: OutputStyle,
) -> miette::Result<()> {
    render_report(file, source_name, source, report, output_style);
    if report.has_errors() {
        Err(miette::miette!(
            "lint diagnostics denied by configured lint levels"
        ))
    } else {
        Ok(())
    }
}

fn render_report(
    file: Option<&Path>,
    source_name: Option<&str>,
    source: Option<&str>,
    report: &LintReport,
    output_style: OutputStyle,
) {
    if output_style.is_full() {
        render_report_full(file, source_name, source, report);
    } else {
        render_report_compact(file, report);
    }
}

fn render_report_full(
    file: Option<&Path>,
    source_name: Option<&str>,
    source: Option<&str>,
    report: &LintReport,
) {
    for diagnostic in report.diagnostics() {
        eprintln!(
            "{:?}",
            error::lint_diagnostic_report(diagnostic, file, source_name, source)
        );
    }
}

fn render_report_compact(file: Option<&Path>, report: &LintReport) {
    for diagnostic in report.diagnostics() {
        let location = diagnostic
            .location()
            .map_or_else(String::new, compact_location);
        if let Some(path) = file {
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
    let Some(selectors) = matches.get_many::<LintSelector>(id) else {
        return Err(miette::miette!(
            "internal error: missing values for lint flag `{id}`"
        ));
    };

    indexed.extend(
        indices
            .zip(selectors.copied())
            .map(|(index, selector)| (index, LintOverride::new(level, selector))),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
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
