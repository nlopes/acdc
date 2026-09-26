//! Execute command blocks from `AsciiDoc` files.
//!
//! Commands are listing or source blocks set with the `command` role.

use std::path::{Path, PathBuf};

use acdc_execute::{CommandBlock, CommandGraph, CommandId, ExecError};
use acdc_parser::{ParseResult, SafeMode};
use clap::{ArgAction, Args as ClapArgs};
use regex::Regex;

use crate::error::{self, WarningReport, WarningReportContext};

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

    /// Print selected commands in execution order instead of running them
    #[arg(long)]
    pub dry_run: bool,

    /// Stop at the first command that exits unsuccessfully
    #[arg(long)]
    pub exit_on_failure: bool,

    /// Safe mode to use while parsing the document
    ///
    /// This limits document reads and includes; it does not sandbox commands.
    #[arg(short = 'S', long, value_parser = clap::value_parser!(SafeMode), default_value = "safe")]
    pub safe_mode: SafeMode,
}

pub fn run(args: &Args) -> miette::Result<()> {
    let parser_options = acdc_parser::Options::builder()
        .with_safe_mode(args.safe_mode)
        .build()
        .map_err(|e| error::display(&e))?;
    let parsed =
        acdc_parser::parse_file(&args.file, &parser_options).map_err(|e| error::display(&e))?;
    report_warnings(&parsed, &args.file);
    let graph = CommandGraph::try_from(parsed.document())
        .map_err(|e| miette::miette!("{}: {e}", args.file.display()))?;
    let selected = select(&graph, &args.ids, &args.id_regexes)?;

    if args.dry_run {
        print_plan(&selected);
    } else {
        execute_plan(&selected, args.exit_on_failure)?;
    }

    Ok(())
}

fn report_warnings(parsed: &ParseResult, file: &Path) {
    let context = WarningReportContext::new().with_optional_file(Some(file));
    for warning in parsed.warnings() {
        eprintln!("{:?}", warning.to_report(context));
    }
}

/// Resolve the `--id` / `--id-regex` selectors to the commands to run, with
/// their transitive dependencies, in execution order. No selectors selects
/// every command; any selector that matches nothing is an error.
fn select(
    graph: &CommandGraph,
    ids: &[String],
    id_regexes: &[Regex],
) -> miette::Result<Vec<CommandBlock>> {
    let mut selected: Vec<CommandId> = Vec::new();

    if ids.is_empty() && id_regexes.is_empty() {
        selected.extend(graph.ids().cloned());
    } else {
        for id in ids {
            let id = CommandId::new(id)
                .map_err(|e| miette::miette!("invalid --id value {id:?}: {e}"))?;
            if !graph.contains(id.as_str()) {
                return Err(miette::miette!("unknown command id: {id}"));
            }
            selected.push(id);
        }
        for regex in id_regexes {
            let mut matches = 0;
            for id in graph.ids() {
                if regex.is_match(id.as_str()) {
                    selected.push(id.clone());
                    matches += 1;
                }
            }
            if matches == 0 {
                return Err(miette::miette!(
                    "--id-regex `{}` matched no commands",
                    regex.as_str()
                ));
            }
        }
    }

    let queue = graph
        .queue_for(&selected)
        .map_err(|e| miette::miette!("{e}"))?;
    Ok(queue.collect())
}

/// Print the selected commands and their scripts in the order they would run.
fn print_plan(blocks: &[CommandBlock]) {
    print!("{}", format_plan(blocks));
}

fn format_plan(blocks: &[CommandBlock]) -> String {
    let mut output = String::new();
    for block in blocks {
        output.push_str(block.metadata.id.as_str());
        output.push_str(" (");
        output.push_str(&block.metadata.interpreter);
        output.push_str(")\n");
        for line in block.script.split_inclusive('\n') {
            output.push_str("  ");
            output.push_str(line);
        }
        if !block.script.ends_with('\n') {
            output.push('\n');
        }
    }
    output
}

/// Run the selected commands in order, inheriting stdio, environment, and the
/// current working directory.
fn execute_plan(blocks: &[CommandBlock], exit_on_failure: bool) -> miette::Result<()> {
    let mut failures = Vec::new();
    for block in blocks {
        match block.execute() {
            Ok(()) => {}
            Err(ExecError::Failed(status)) => {
                if exit_on_failure {
                    return Err(miette::miette!(
                        "command `{}` exited with {status}",
                        block.metadata.id
                    ));
                }
                failures.push(format!("`{}` exited with {status}", block.metadata.id));
            }
            Err(e) => return Err(miette::miette!("command `{}`: {e}", block.metadata.id)),
        }
    }
    if failures.is_empty() {
        return Ok(());
    }
    let plural = if failures.len() == 1 {
        "command"
    } else {
        "commands"
    };
    Err(miette::miette!(
        "{} {plural} failed: {}",
        failures.len(),
        failures.join(", ")
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::path::PathBuf;

    use acdc_parser::{Block, DelimitedBlockType, Options, SafeMode};
    use clap::Parser;
    use regex::Regex;

    use super::{Args, execute_plan, format_plan, select};
    use acdc_execute::{CommandBlock, CommandGraph};

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: Args,
    }

    fn parse_graph(src: &str) -> CommandGraph {
        let parsed = acdc_parser::parse(src, &Options::default())
            .unwrap_or_else(|error| panic!("parse failed: {error}"));
        CommandGraph::try_from(parsed.document())
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
            .into_iter()
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
            acdc_parser::Location::default(),
        );
        assert_eq!(format_plan(&[block]), "build (sh)\n  echo hello\n  \n");
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

    // ------------------------------------------------------------------
    // Execution
    // ------------------------------------------------------------------

    fn fail_and_marker_plan(marker: &str) -> (Vec<CommandBlock>, String) {
        let src = format!(
            "{}\n{}",
            cmd("bad", None, "exit 1"),
            cmd("after", None, &format!("echo run >> {marker}")),
        );
        let graph = parse_graph(&src);
        let blocks = select(&graph, &[], &[]).unwrap_or_else(|e| panic!("{e}"));
        (blocks, marker.to_string())
    }

    #[test]
    fn successful_commands_run_in_dependency_order() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let first = dir.path().join("first");
        let second = dir.path().join("second");

        let src = format!(
            "{}\n{}",
            cmd("first", None, &format!("echo run >> {}", first.display())),
            cmd(
                "second",
                Some("first"),
                &format!("echo run >> {}", second.display())
            ),
        );
        let graph = parse_graph(&src);
        let blocks = select(&graph, &[], &[]).map_err(|e| panic!("{e}"))?;
        execute_plan(&blocks, false)?;

        let first_meta = std::fs::metadata(&first)?;
        let second_meta = std::fs::metadata(&second)?;
        assert!(
            first_meta.created()? <= second_meta.created()?,
            "first must run before second"
        );
        Ok(())
    }

    #[test]
    fn failed_command_fails_the_run() {
        let failing = CommandBlock::new(
            "bad".parse().unwrap_or_else(|e| panic!("{e}")),
            "exit 1".into(),
            None,
            acdc_parser::Location::default(),
        );
        let error = execute_plan(&[failing], false).expect_err("should fail");
        assert!(error.to_string().contains("`bad` exited with"));
    }

    #[test]
    fn continue_on_failure_attempts_later_commands() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let marker = dir.path().join("after");
        let (blocks, _) = fail_and_marker_plan(&format!("{}", marker.display()));

        let error = execute_plan(&blocks, false).expect_err("should fail");
        assert!(error.to_string().contains("`bad` exited with"));
        assert!(marker.exists(), "later commands must still be attempted");
        Ok(())
    }

    #[test]
    fn stop_on_failure_skips_later_commands() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let marker = dir.path().join("after");
        let (blocks, _) = fail_and_marker_plan(&format!("{}", marker.display()));

        assert!(execute_plan(&blocks, true).is_err());
        assert!(!marker.exists(), "later commands must not be attempted");
        Ok(())
    }

    #[test]
    fn missing_interpreter_fails_immediately() {
        let block = CommandBlock::new(
            "nope".parse().unwrap_or_else(|e| panic!("{e}")),
            "true".into(),
            Some("acdc-execute-nonexistent-interpreter".into()),
            acdc_parser::Location::default(),
        );
        let error = execute_plan(&[block], false).expect_err("should fail");
        assert!(error.to_string().contains("could not spawn interpreter"));
    }
}
