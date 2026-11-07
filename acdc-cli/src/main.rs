use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

#[cfg(any(
    feature = "html",
    feature = "manpage",
    feature = "markdown",
    feature = "pdf",
    feature = "terminal",
    feature = "execute",
    feature = "lint"
))]
mod error;
mod subcommands;
#[cfg(any(
    feature = "html",
    feature = "manpage",
    feature = "markdown",
    feature = "pdf",
    feature = "terminal"
))]
mod timing;

#[derive(Parser)]
#[command(name = "acdc")]
#[command(author, version, about = "AsciiDoc toolchain", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse into native JSON or a checked ASG format
    Parse(subcommands::parse::Args),

    #[cfg(any(
        feature = "html",
        feature = "manpage",
        feature = "markdown",
        feature = "pdf",
        feature = "terminal"
    ))]
    /// Convert `AsciiDoc` documents to various output formats
    Convert(subcommands::convert::Args),

    #[cfg(feature = "execute")]
    /// Execute command blocks defined in `AsciiDoc` documents
    Execute(subcommands::execute::Args),

    #[cfg(feature = "inspect")]
    /// Show a structural outline of an `AsciiDoc` document
    Inspect(subcommands::inspect::Args),

    #[cfg(feature = "lint")]
    /// Lint `AsciiDoc` documents
    Lint(subcommands::lint::Args),

    #[cfg(feature = "tck")]
    /// Run TCK compliance tests (reads JSON from stdin)
    Tck(subcommands::tck::Args),
}

fn setup_logging() {
    // Only install a tracing subscriber when ACDC_LOG is explicitly set.
    // Without a subscriber, tracing macros are near-zero-cost no-ops,
    // avoiding per-call dispatch overhead on every parse/convert operation.
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_env("ACDC_LOG") {
        use tracing_subscriber::prelude::*;
        let layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
            .with_timer(tracing_subscriber::fmt::time::Uptime::default())
            .with_filter(filter);

        tracing_subscriber::registry().with(layer).init();
    }
}

fn main() {
    setup_logging();
    let matches = Cli::command().get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };

    #[cfg(feature = "lint")]
    let full_error_output = match &cli.command {
        Commands::Parse(_) => true,
        Commands::Lint(args) => args.output_style.is_full(),
        #[cfg(any(
            feature = "html",
            feature = "manpage",
            feature = "markdown",
            feature = "pdf",
            feature = "terminal"
        ))]
        Commands::Convert(_) => true,
        #[cfg(feature = "execute")]
        Commands::Execute(_) => true,
        #[cfg(feature = "inspect")]
        Commands::Inspect(_) => true,
        #[cfg(feature = "tck")]
        Commands::Tck(_) => true,
    };
    let result: miette::Result<()> = match cli.command {
        Commands::Parse(args) => {
            subcommands::json::finish(subcommands::parse::run(&args));
            Ok(())
        }

        #[cfg(any(
            feature = "html",
            feature = "manpage",
            feature = "markdown",
            feature = "pdf",
            feature = "terminal"
        ))]
        Commands::Convert(args) => subcommands::convert::run(&args),

        #[cfg(feature = "execute")]
        Commands::Execute(args) => subcommands::execute::run(&args),

        #[cfg(feature = "inspect")]
        Commands::Inspect(args) => {
            subcommands::inspect::run(&args).map_err(|e| miette::miette!("Inspect failed: {e}"))
        }

        #[cfg(feature = "lint")]
        Commands::Lint(args) => match matches.subcommand() {
            Some(("lint", lint_matches)) => subcommands::lint::run(&args, lint_matches),
            _ => Err(miette::miette!(
                "internal error: missing lint argument matches"
            )),
        },

        #[cfg(feature = "tck")]
        Commands::Tck(args) => {
            subcommands::json::finish(subcommands::tck::run(&args));
            Ok(())
        }
    };

    if let Err(e) = result {
        #[cfg(feature = "lint")]
        {
            if full_error_output {
                eprintln!("{e:?}");
            } else {
                eprintln!("error: {e}");
            }
        }
        #[cfg(not(feature = "lint"))]
        {
            eprintln!("{e:?}");
        }
        std::process::exit(1);
    }
}
