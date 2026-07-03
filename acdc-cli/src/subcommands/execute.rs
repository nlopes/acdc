//! Skeleton for executing command blocks from `AsciiDoc` files.
//!
//! # Handoff contract
//!
//! The implementation is expected to:
//!
//! - collect listing/source blocks carrying the `command` role and an explicit ID;
//! - recursively visit eligible nested blocks and preserve document order, including
//!   blocks originating in included files;
//! - select every command when no selector is supplied, otherwise use the union of
//!   exact and regex selectors without executing a command more than once;
//! - report missing selectors and duplicate command IDs as normal CLI diagnostics;
//! - return a failing CLI result if any command fails, with `--exit-on-failure`
//!   controlling whether later commands are attempted.
//!
//! Before implementing execution, decide whether scripts use original verbatim source
//! text or parser-transformed inline content, which interpreter and working directory
//! apply, and which environment is inherited. Parser safe mode only limits document
//! reads; it does not sandbox commands.

use std::path::{Path, PathBuf};

use acdc_parser::{Document, Location, ParseResult, SafeMode};
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
        .build();
    let parsed =
        acdc_parser::parse_file(&args.file, &parser_options).map_err(|e| error::display(&e))?;
    let parsed = report_warnings(parsed, &args.file);
    let candidates = collect_command_candidates(parsed.document(), &args.file)?;
    let plan = select_candidates(args, candidates)?;

    if args.dry_run {
        render_dry_run(&plan)?;
    } else {
        execute_plan(&plan, args.exit_on_failure)?;
    }

    Ok(())
}

#[allow(dead_code)]
#[derive(Debug)]
struct CommandCandidate {
    id: String,
    /// Planned command text. The implementation must decide whether this is original
    /// verbatim source or reconstructed parser content before populating it.
    script: String,
    source: CommandSource,
}

#[allow(dead_code)]
#[derive(Debug)]
struct CommandSource {
    /// Root document supplied on the command line.
    root_document: PathBuf,
    /// Parser-remapped location. Its positions retain the include chain for content
    /// originating outside the root document.
    location: Location,
}

#[allow(dead_code)]
#[derive(Debug)]
struct ExecutionPlan {
    commands: Vec<CommandCandidate>,
}

fn report_warnings(parsed: ParseResult, file: &Path) -> ParseResult {
    let context = WarningReportContext::new().with_optional_file(Some(file));
    for warning in parsed.warnings() {
        eprintln!("{:?}", warning.to_report(context));
    }
    parsed
}

fn collect_command_candidates(
    _document: &Document<'_>,
    _root_document: &Path,
) -> miette::Result<Vec<CommandCandidate>> {
    todo!("collect [.command] listing/source blocks from the parsed AsciiDoc document")
}

fn select_candidates(
    _args: &Args,
    _candidates: Vec<CommandCandidate>,
) -> miette::Result<ExecutionPlan> {
    todo!("apply --id and --id-regex selectors while preserving document order")
}

fn render_dry_run(_plan: &ExecutionPlan) -> miette::Result<()> {
    todo!("print selected command scripts in the sequence they would be executed")
}

fn execute_plan(_plan: &ExecutionPlan, _exit_on_failure: bool) -> miette::Result<()> {
    todo!("execute selected command scripts")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use acdc_parser::{Block, DelimitedBlockType, Options, SafeMode};
    use clap::Parser;

    use super::Args;

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: Args,
    }

    #[test]
    fn parses_execute_skeleton_flags() {
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
    fn parses_the_planned_command_block_shape() -> miette::Result<()> {
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
}
