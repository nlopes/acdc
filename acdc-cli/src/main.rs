use std::{
    fmt,
    io::{self, BufReader, Read},
    path::PathBuf,
};

use anyhow::Result;
use clap::{Parser, ValueEnum};

#[derive(Debug, ValueEnum, Clone)]
enum Backend {
    Html,
    #[cfg(feature = "terminal")]
    Terminal,
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Backend::Html => write!(f, "html"),

            #[cfg(feature = "terminal")]
            Backend::Terminal => write!(f, "terminal"),
        }
    }
}

#[derive(Debug, ValueEnum, Clone)]
enum Doctype {
    Article,
    Book,
    Manpage,
    Inline,
}

impl fmt::Display for Doctype {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Doctype::Article => write!(f, "article"),
            Doctype::Book => write!(f, "book"),
            Doctype::Manpage => write!(f, "manpage"),
            Doctype::Inline => write!(f, "inline"),
        }
    }
}

/// Parses files
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List of files to parse
    #[arg(required = true, conflicts_with = "tck_mode")]
    files: Vec<PathBuf>,

    /// backend output format
    #[arg(long, required = true, conflicts_with = "tck_mode", default_value_t = Backend::Html)]
    backend: Backend,

    /// document type to use when converting document
    #[arg(long, required = true, conflicts_with = "tck_mode", default_value_t = Doctype::Article)]
    doctype: Doctype,

    /// Run in TCK compatible mode, taking a single `AsciiDoc` document from `stdin` and
    /// outputting JSON to `stdout`
    #[arg(long)]
    tck_mode: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.tck_mode {
        handle_tck_mode()?;
    } else {
        handle_normal_mode(args)?;
    }

    Ok(())
}

fn handle_tck_mode() -> Result<()> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut input = String::new();
    reader.read_to_string(&mut input)?;

    use std::io::Write;
    let doc = acdc_parser::parse(&input)?;
    let mut stdout = io::stdout();
    serde_json::to_writer_pretty(&stdout, &doc)?;
    stdout.flush()?;
    Ok(())
}

fn handle_normal_mode(args: Args) -> Result<()> {
    match args.backend {
        Backend::Html => {
            todo!("html backend")
        }

        #[cfg(feature = "terminal")]
        Backend::Terminal => {
            for file in &args.files {
                acdc_term::parse_file(file)?;
            }
        }
    }
    Ok(())
}
