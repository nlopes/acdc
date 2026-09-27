//! Execute command blocks from `AsciiDoc` files.
//!
//! Commands are listing or source blocks set with the `command` role.

use std::{
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
};

use acdc_execute::{
    CommandGraph, CommandId, CommandState, ExecError, ExecutionOptions, ExecutionPlan,
    ExecutionReport, ProcessOptions, SkipReason,
};
use acdc_parser::{ParseResult, SafeMode};
use clap::{ArgAction, Args as ClapArgs};
use miette::{Diagnostic, IntoDiagnostic as _};
use regex::Regex;

use crate::error::{LocatedError, SourceCache, WarningReport, WarningReportContext};

/// Execute command blocks defined in an `AsciiDoc` file
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Input `AsciiDoc` file
    pub file: PathBuf,

    /// Select command blocks whose id exactly matches this value
    #[arg(long = "id", value_name = "ID", action = ArgAction::Append)]
    pub ids: Vec<String>,

    /// Select command blocks whose id matches this regex
    #[arg(long = "id-regex", value_name = "REGEX", action = ArgAction::Append)]
    pub id_regexes: Vec<Regex>,

    /// Print selected commands and scripts instead of running them
    #[arg(long, conflicts_with = "list")]
    pub dry_run: bool,

    /// List command ids, interpreters, sections, dependencies, and descriptions without running them
    #[arg(long)]
    pub list: bool,

    /// Working directory for child processes (defaults to the caller's directory)
    #[arg(long, value_name = "DIRECTORY")]
    pub cwd: Option<PathBuf>,

    /// Override a child environment variable; repeat to set more values
    #[arg(long = "env", value_name = "NAME=VALUE", value_parser = parse_environment, action = ArgAction::Append)]
    pub environment: Vec<(OsString, OsString)>,

    /// Stop all commands at the first unsuccessful child exit
    #[arg(long)]
    pub exit_on_failure: bool,

    /// Safe mode to use while parsing the document
    ///
    /// This limits document reads and includes; it does not sandbox commands.
    #[arg(short = 'S', long, value_parser = clap::value_parser!(SafeMode), default_value = "safe")]
    pub safe_mode: SafeMode,
}

fn parse_environment(value: &str) -> Result<(OsString, OsString), String> {
    let (name, value) = value.split_once('=').ok_or("expected NAME=VALUE")?;
    if name.is_empty() || name.contains('\0') || value.contains('\0') {
        return Err(
            "environment names must be nonempty and names and values must not contain NUL".into(),
        );
    }
    Ok((name.into(), value.into()))
}

pub fn run(args: &Args) -> miette::Result<()> {
    let parser_options = acdc_parser::Options::builder()
        .with_safe_mode(args.safe_mode)
        .build()
        .map_err(parser_report)?;
    let parsed = acdc_parser::parse_file(&args.file, &parser_options).map_err(parser_report)?;
    let graph = CommandGraph::try_from(&parsed).map_err(|error| {
        let locations = error
            .source_location()
            .into_iter()
            .chain(error.related_location())
            .cloned()
            .collect::<Vec<_>>();
        miette::Report::new(LocatedError::new(error, locations))
    })?;
    report_warnings(&parsed)?;
    drop(parsed);
    let selected = select(&graph, &args.ids, &args.id_regexes)?;

    if args.dry_run || args.list {
        let mut stdout = io::stdout().lock();
        if args.list {
            write_list(&mut stdout, &graph, &selected).into_diagnostic()?;
        } else {
            write_plan(&mut stdout, &selected).into_diagnostic()?;
        }
        stdout.flush().into_diagnostic()?;
    } else {
        let report = selected.execute(&ExecutionOptions {
            exit_on_failure: args.exit_on_failure,
            process: ProcessOptions {
                current_dir: args.cwd.clone(),
                env: args.environment.clone(),
            },
        });
        report_execution(report)?;
    }
    Ok(())
}

fn parser_report(error: acdc_parser::Error) -> miette::Report {
    let location = error.source_location().cloned();
    miette::Report::new(LocatedError::new(error, location))
}

fn report_warnings(parsed: &ParseResult) -> miette::Result<()> {
    let mut context = WarningReportContext::new();
    let mut stderr = io::stderr().lock();
    for warning in parsed.warnings() {
        writeln!(stderr, "{:?}", warning.to_report(&mut context)).into_diagnostic()?;
    }
    Ok(())
}

/// Selectors form a union; each must match, and selected commands include their prerequisites.
fn select<'graph>(
    graph: &'graph CommandGraph,
    ids: &[String],
    id_regexes: &[Regex],
) -> miette::Result<ExecutionPlan<'graph>> {
    if ids.is_empty() && id_regexes.is_empty() {
        return Ok(graph.plan_all());
    }
    let mut selected = Vec::new();
    for id in ids {
        let id = CommandId::new(id)
            .map_err(|error| miette::miette!("invalid --id value {id:?}: {error}"))?;
        if !graph.contains(id.as_str()) {
            return Err(miette::miette!("unknown command id: {id}"));
        }
        selected.push(id);
    }
    for regex in id_regexes {
        let previous_count = selected.len();
        selected.extend(
            graph
                .ids()
                .filter(|id| regex.is_match(id.as_str()))
                .cloned(),
        );
        if selected.len() == previous_count {
            return Err(miette::miette!(
                "--id-regex `{}` matched no commands",
                regex.as_str()
            ));
        }
    }
    graph.plan_for(&selected).into_diagnostic()
}

fn write_plan(output: &mut impl Write, plan: &ExecutionPlan<'_>) -> io::Result<()> {
    for block in plan.commands() {
        writeln!(
            output,
            "{} ({})",
            block.metadata.id, block.metadata.interpreter
        )?;
        for line in block.script.split_inclusive('\n') {
            write!(output, "  {line}")?;
        }
        if !block.script.ends_with('\n') {
            writeln!(output)?;
        }
    }
    Ok(())
}

fn write_list(
    output: &mut impl Write,
    graph: &CommandGraph,
    plan: &ExecutionPlan<'_>,
) -> io::Result<()> {
    for block in plan.commands() {
        write!(
            output,
            "- id={}, interpreter={:?}",
            block.metadata.id, block.metadata.interpreter
        )?;
        if let Some(section) = &block.metadata.section_title {
            write!(output, ", section={section:?}")?;
        }
        let mut dependencies = graph
            .dependencies(block.metadata.id.as_str())
            .into_iter()
            .flatten()
            .map(CommandId::as_str)
            .collect::<Vec<_>>();
        dependencies.sort_unstable();
        if !dependencies.is_empty() {
            write!(output, ", deps=\"")?;
            for (index, dependency) in dependencies.iter().enumerate() {
                if index > 0 {
                    write!(output, ",")?;
                }
                write!(output, "{dependency}")?;
            }
            write!(output, "\"")?;
        }
        if let Some(description) = &block.metadata.description {
            write!(output, ", description={description:?}")?;
        }
        writeln!(output)?;
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum CommandIssue {
    #[error("command `{id}` {error}")]
    Failed {
        id: CommandId,
        #[source]
        error: ExecError,
    },
    #[error("command `{id}` skipped because prerequisite `{dependency}` did not succeed")]
    Dependency {
        id: CommandId,
        dependency: CommandId,
    },
    #[error("command `{id}` skipped after failure of `{command}`")]
    Stopped { id: CommandId, command: CommandId },
}

#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("command execution failed ({failed} failed, {skipped} skipped)")]
struct ExecutionFailed {
    failed: usize,
    skipped: usize,
    #[related]
    issues: Vec<LocatedError<CommandIssue>>,
}

fn report_execution(report: ExecutionReport<'_>) -> miette::Result<()> {
    if report.is_success() {
        return Ok(());
    }
    let mut failed = 0;
    let mut skipped = 0;
    let mut issues = Vec::new();
    let mut sources = SourceCache::default();
    for outcome in report.into_outcomes() {
        let diagnostic = match outcome.state {
            CommandState::Succeeded => continue,
            CommandState::Failed(error) => {
                failed += 1;
                LocatedError::with_cached_source(
                    CommandIssue::Failed {
                        id: outcome.command.metadata.id.clone(),
                        error,
                    },
                    &outcome.command.location,
                    &mut sources,
                )
            }
            CommandState::Skipped(reason) => {
                skipped += 1;
                let id = outcome.command.metadata.id.clone();
                let issue = match reason {
                    SkipReason::DependencyFailed { dependency } => CommandIssue::Dependency {
                        id,
                        dependency: dependency.clone(),
                    },
                    SkipReason::StoppedAfterFailure { command } => CommandIssue::Stopped {
                        id,
                        command: command.clone(),
                    },
                };
                LocatedError::without_source(issue, &outcome.command.location)
            }
        };
        issues.push(diagnostic);
    }
    Err(miette::Report::new(ExecutionFailed {
        failed,
        skipped,
        issues,
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::path::PathBuf;

    use acdc_parser::{Block, DelimitedBlockType, Options, SafeMode};
    use clap::Parser;
    use regex::Regex;

    use super::{Args, select, write_list, write_plan};
    use acdc_execute::{CommandBlock, CommandGraph, CommandGraphBuilder};

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: Args,
    }

    fn parse_graph(src: &str) -> CommandGraph {
        let parsed = acdc_parser::parse(src, &Options::default())
            .unwrap_or_else(|error| panic!("parse failed: {error}"));
        CommandGraph::try_from(&parsed)
            .unwrap_or_else(|error| panic!("graph should build: {error}"))
    }

    fn select_ids(graph: &CommandGraph, ids: &[&str], regexes: &[&str]) -> Vec<String> {
        let regexes: Vec<Regex> = regexes
            .iter()
            .map(|r| Regex::new(r).unwrap_or_else(|e| panic!("test regex {r:?}: {e}")))
            .collect();
        let ids: Vec<String> = ids.iter().map(ToString::to_string).collect();
        select(graph, &ids, &regexes)
            .unwrap_or_else(|error| panic!("selection should succeed: {error}"))
            .commands()
            .map(|block| block.metadata.id.as_str().to_string())
            .collect()
    }

    fn cmd(id: &str, deps: Option<&str>, body: &str) -> String {
        let deps = deps.map(|d| format!(", deps=\"{d}\"")).unwrap_or_default();
        format!("[.command, id={id}{deps}]\n----\n{body}\n----\n")
    }

    #[test]
    fn parses_execute_flags() {
        let cli = TestCli::parse_from([
            "test",
            "README.adoc",
            "--id",
            "build",
            "--id",
            "test",
            "--id-regex",
            "^deploy-",
            "--dry-run",
            "--exit-on-failure",
        ]);

        assert_eq!(cli.args.file, PathBuf::from("README.adoc"));
        assert_eq!(cli.args.ids, ["build", "test"]);
        assert_eq!(cli.args.id_regexes.len(), 1);
        assert_eq!(
            cli.args.id_regexes.first().map(regex::Regex::as_str),
            Some("^deploy-")
        );
        assert!(cli.args.dry_run);
        assert!(cli.args.exit_on_failure);
        assert_eq!(cli.args.safe_mode, SafeMode::Safe);
    }

    #[test]
    fn rejects_invalid_id_regex() {
        let err = TestCli::try_parse_from(["test", "README.adoc", "--id-regex", "["]);
        assert!(err.is_err());
    }

    #[test]
    fn dry_run_preserves_trailing_blank_lines() {
        let block = CommandBlock::new(
            "build".parse().unwrap_or_else(|e| panic!("{e}")),
            "echo hello\n\n".into(),
            None,
            acdc_parser::SourceLocation::at_location(None, acdc_parser::Location::default()),
        );
        let mut builder = CommandGraphBuilder::new();
        builder.add(block, Vec::new());
        let graph = builder.build().unwrap();
        let mut output = Vec::new();
        write_plan(&mut output, &graph.plan_all()).unwrap();
        assert_eq!(output, b"build (sh)\n  echo hello\n  \n");
    }

    #[test]
    fn parses_the_command_block_shape() -> miette::Result<()> {
        let input = "[.command, id=build]\n----\necho hello\n----\n";
        let parsed = acdc_parser::parse(input, &Options::default())
            .map_err(|error| miette::miette!(error.to_string()))?;
        let Some(Block::DelimitedBlock(block)) = parsed.document().blocks.first() else {
            return Err(miette::miette!(
                "expected command markup to parse as a delimited block"
            ));
        };

        assert!(matches!(
            block.inner,
            DelimitedBlockType::DelimitedListing(_)
        ));
        assert_eq!(block.metadata.roles, ["command"]);
        assert_eq!(
            block.metadata.id.as_ref().map(|anchor| anchor.id),
            Some("build")
        );

        Ok(())
    }

    // ------------------------------------------------------------------
    // Selection
    // ------------------------------------------------------------------

    #[test]
    fn no_selectors_selects_all_commands_in_execution_order() {
        let src = format!(
            "{}\n{}\n{}",
            cmd("c", Some("b"), "echo c"),
            cmd("a", None, "echo a"),
            cmd("b", Some("a"), "echo b"),
        );
        let graph = parse_graph(&src);
        assert_eq!(select_ids(&graph, &[], &[]), ["a", "b", "c"]);
    }

    #[test]
    fn exact_id_selects_one_command() {
        let src = format!("{}\n{}", cmd("a", None, "echo a"), cmd("b", None, "echo b"));
        let graph = parse_graph(&src);
        assert_eq!(select_ids(&graph, &["b"], &[]), ["b"]);
    }

    #[test]
    fn regex_selects_matching_commands() {
        let src = format!(
            "{}\n{}\n{}",
            cmd("test-unit", None, "echo 1"),
            cmd("test-integration", None, "echo 2"),
            cmd("build", None, "echo 3"),
        );
        let graph = parse_graph(&src);
        assert_eq!(
            select_ids(&graph, &[], &["^test-"]),
            ["test-unit", "test-integration"]
        );
    }

    #[test]
    fn exact_and_regex_selectors_form_a_union() {
        let src = format!(
            "{}\n{}\n{}\n{}",
            cmd("build", None, "echo build"),
            cmd("deploy-staging", Some("build"), "echo staging"),
            cmd("deploy-prod", Some("build"), "echo prod"),
            cmd("docs", None, "echo docs"),
        );
        let graph = parse_graph(&src);
        let mut ids = select_ids(&graph, &["docs"], &["^deploy-"]);
        ids.sort();
        // `build` is pulled in as a transitive dependency of both deploy targets.
        assert_eq!(ids, ["build", "deploy-prod", "deploy-staging", "docs"]);
    }

    #[test]
    fn selection_includes_transitive_dependencies() {
        let src = format!(
            "{}\n{}\n{}",
            cmd("gen", None, "echo gen"),
            cmd("build", Some("gen"), "echo build"),
            cmd("other", None, "echo other"),
        );
        let graph = parse_graph(&src);
        assert_eq!(select_ids(&graph, &["build"], &[]), ["gen", "build"]);
    }

    #[test]
    fn each_command_is_selected_only_once() {
        let src = format!(
            "{}\n{}",
            cmd("deploy-x", None, "echo x"),
            cmd("build", None, "echo b")
        );
        let graph = parse_graph(&src);
        let ids = select_ids(&graph, &["deploy-x"], &["^deploy-"]);
        assert_eq!(ids, ["deploy-x"]);
    }

    #[test]
    fn unknown_exact_id_is_an_error() {
        let graph = parse_graph(&cmd("a", None, "echo a"));
        let regexes = Vec::new();
        let ids = vec!["missing".to_string()];
        let error = select(&graph, &ids, &regexes).expect_err("should fail");
        assert!(error.to_string().contains("unknown command id: missing"));
    }

    #[test]
    fn regex_matching_no_commands_is_an_error() {
        let graph = parse_graph(&cmd("a", None, "echo a"));
        let regexes = vec![Regex::new("^nope-").unwrap()];
        let error = select(&graph, &[], &regexes).expect_err("should fail");
        assert!(error.to_string().contains("matched no commands"));
    }

    #[test]
    fn invalid_exact_id_value_is_an_error() {
        let graph = parse_graph(&cmd("a", None, "echo a"));
        let regexes = Vec::new();
        let ids = vec!["bad id".to_string()];
        let error = select(&graph, &ids, &regexes).expect_err("should fail");
        assert!(error.to_string().contains("invalid --id value"));
    }

    #[test]
    fn list_and_dry_run_conflict() {
        let error = TestCli::try_parse_from(["test", "commands.adoc", "--list", "--dry-run"]);
        assert!(error.is_err());
    }

    #[test]
    fn parses_ordered_environment_overrides_and_cwd() {
        let cli = TestCli::parse_from([
            "test",
            "commands.adoc",
            "--cwd",
            "work",
            "--env",
            "VALUE=first",
            "--env",
            "VALUE=last=kept",
            "--env",
            "EMPTY=",
        ]);
        assert_eq!(cli.args.cwd, Some(PathBuf::from("work")));
        assert_eq!(
            cli.args.environment,
            vec![
                ("VALUE".into(), "first".into()),
                ("VALUE".into(), "last=kept".into()),
                ("EMPTY".into(), "".into()),
            ]
        );
    }

    #[test]
    fn rejects_missing_environment_assignment_or_name() {
        for value in ["NAME", "=value"] {
            assert!(TestCli::try_parse_from(["test", "commands.adoc", "--env", value]).is_err());
        }
    }

    #[test]
    fn list_includes_descriptions_and_prerequisites_without_scripts() {
        let graph = parse_graph(concat!(
            "= Commands\n\n",
            "[.command,id=build,description=Compile]\n----\necho build\n----\n\n",
            "== Quality\n\n",
            "[source,bash,role=command,id=lint,deps=build]\n----\necho lint\n----\n\n",
            "=== *Unit* tests\n\n",
            "[source,python,role=command,id=test,deps=\"lint,build,lint\",interpreter=python3]\n",
            "----\nprint('test')\n----\n",
        ));
        let plan = select(&graph, &["test".into()], &[]).unwrap();
        let mut output = Vec::new();
        write_list(&mut output, &graph, &plan).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            concat!(
                "- id=build, interpreter=\"sh\", description=\"Compile\"\n",
                "- id=lint, interpreter=\"bash\", section=\"Quality\", deps=\"build\"\n",
                "- id=test, interpreter=\"python3\", section=\"Unit tests\", deps=\"build,lint\"\n",
            )
        );
    }

    #[test]
    fn list_quotes_paths_and_escapes_control_characters() {
        let mut builder = CommandGraphBuilder::new();
        let mut command = CommandBlock::new(
            "multiline".parse().unwrap(),
            String::new(),
            Some("tools/my \"shell\"\n\u{1b}".into()),
            acdc_parser::SourceLocation::at_location(None, acdc_parser::Location::default()),
        )
        .with_description(Some("first\r\nsecond\nthird\rfourth\t\u{1b}".into()));
        command.metadata.section_title = Some("Setup \"tools\"\n\u{1b}".into());
        builder.add(command, Vec::new());
        let graph = builder.build().unwrap();
        let mut output = Vec::new();
        write_list(&mut output, &graph, &graph.plan_all()).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            concat!(
                r#"- id=multiline, interpreter="tools/my \"shell\"\n\u{1b}""#,
                r#", section="Setup \"tools\"\n\u{1b}""#,
                r#", description="first\r\nsecond\nthird\rfourth\t\u{1b}""#,
                "\n",
            )
        );
    }

    #[test]
    fn dry_run_adds_only_a_display_newline() {
        let mut builder = CommandGraphBuilder::new();
        builder.add(
            CommandBlock::new(
                "literal".parse().unwrap(),
                "last line".into(),
                None,
                acdc_parser::SourceLocation::at_location(None, acdc_parser::Location::default()),
            ),
            Vec::new(),
        );
        let graph = builder.build().unwrap();
        let plan = graph.plan_all();
        let mut output = Vec::new();
        write_plan(&mut output, &plan).unwrap();
        assert_eq!(output, b"literal (sh)\n  last line\n");
        assert_eq!(plan.commands().next().unwrap().script, "last line");
    }

    #[test]
    fn output_write_errors_are_returned() {
        struct BrokenOutput;
        impl std::io::Write for BrokenOutput {
            fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let graph = parse_graph(&cmd("build", None, "true"));
        let plan = graph.plan_all();
        assert_eq!(
            write_plan(&mut BrokenOutput, &plan).unwrap_err().kind(),
            std::io::ErrorKind::BrokenPipe
        );
        assert_eq!(
            write_list(&mut BrokenOutput, &graph, &plan)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::BrokenPipe
        );
    }
}
