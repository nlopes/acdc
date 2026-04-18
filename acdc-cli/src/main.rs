use clap::{Parser, Subcommand};

mod error;
mod subcommands;
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
    /// Convert `AsciiDoc` documents to various output formats
    Convert(subcommands::convert::Args),

    #[cfg(feature = "inspect")]
    /// Inspect AST structure of `AsciiDoc` documents
    Inspect(subcommands::inspect::Args),

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
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Convert(args) => subcommands::convert::run(args),

        #[cfg(feature = "inspect")]
        Commands::Inspect(args) => {
            subcommands::inspect::run(args).map_err(|e| miette::miette!("Inspect failed: {e}"))
        }

        #[cfg(feature = "tck")]
        Commands::Tck(args) => {
            subcommands::tck::run(args).map_err(|e| miette::miette!("TCK failed: {e}"))
        }
    };

    if let Err(e) = result {
        eprintln!("{e:?}");
    }
}
