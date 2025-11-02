use clap::{Parser, Subcommand};

mod error;
mod subcommands;

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
    use tracing_subscriber::{EnvFilter, prelude::*};

    let env_filter = EnvFilter::try_from_env("ACDC_LOG");

    if let Ok(filter) = env_filter {
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
