use std::{
    io::Read,
    path::{Path, PathBuf},
};

use acdc_lint::{LintLevel, LintOptions, LintOverride, LintReport, LintSelector};
use clap::{ArgAction, ArgMatches, Args as ClapArgs};

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
}

pub fn run(args: &Args, matches: &ArgMatches) -> miette::Result<()> {
    let options = LintOptions::new(ordered_overrides(matches)?);

    if args.stdin {
        let mut source = String::new();
        std::io::stdin()
            .read_to_string(&mut source)
            .map_err(|error| miette::miette!("failed to read stdin: {error}"))?;
        let report = acdc_lint::lint_source(Some("<stdin>"), &source, &options)
            .map_err(|error| miette::miette!("lint failed: {error}"))?;
        return finish_report(&report);
    }

    if args.files.is_empty() {
        return Err(miette::miette!(
            "lint requires at least one file, or --stdin to read from standard input"
        ));
    }

    let mut failed = false;
    for file in &args.files {
        let report = acdc_lint::lint_path(file, &options)
            .map_err(|error| miette::miette!("lint failed for {}: {error}", file.display()))?;
        render_report(Some(file), &report);
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

fn finish_report(report: &LintReport) -> miette::Result<()> {
    render_report(None, report);
    if report.has_errors() {
        Err(miette::miette!(
            "lint diagnostics denied by configured lint levels"
        ))
    } else {
        Ok(())
    }
}

fn render_report(file: Option<&Path>, report: &LintReport) {
    for diagnostic in report.diagnostics() {
        if let Some(path) = file {
            eprintln!("{}: {diagnostic}", path.display());
        } else {
            eprintln!("{diagnostic}");
        }
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
}
