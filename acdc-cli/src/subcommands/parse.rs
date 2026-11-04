use std::{
    io::{self, Read},
    path::{Path, PathBuf},
};

use acdc_parser::{CompatibilityMode, Options, SafeMode};

use super::json::{self, Error, Format};

/// Parse a document or inline fragment into native JSON or checked ASG
#[derive(clap::Args)]
pub struct Args {
    /// Input file, or - for stdin
    #[arg(default_value = "-")]
    pub input: PathBuf,
    /// Native AST JSON, pinned upstream ASG, or the acdc ASG v1 extension
    #[arg(long, value_enum, default_value = "native-json")]
    pub format: Format,
    /// Document blocks or an inline fragment
    #[arg(long = "type", value_enum, default_value = "block")]
    pub kind: InputKind,
    /// Language rules: asciidoctor or draft
    #[arg(long, value_name = "MODE", default_value_t = CompatibilityMode::default())]
    pub compatibility_mode: CompatibilityMode,
    /// Safe mode to use while parsing
    #[arg(short = 'S', long, default_value = "unsafe")]
    pub safe_mode: SafeMode,
    /// Maximum selected include text per document, in bytes
    #[arg(long, value_name = "BYTES", default_value_t = Options::default().max_total_include_bytes())]
    pub max_total_include_bytes: usize,
    /// Output file, or - for stdout
    #[arg(short = 'o', long, default_value = "-")]
    pub output: PathBuf,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum InputKind {
    Block,
    Inline,
}

pub fn run(args: &Args) -> Result<(), Error> {
    args.format.check_available()?;
    let options = Options::builder()
        .with_compatibility_mode(args.compatibility_mode)
        .with_safe_mode(args.safe_mode)
        .with_max_total_include_bytes(args.max_total_include_bytes)
        .build()?;
    let value = match args.kind {
        InputKind::Block => {
            let parsed = if args.input == Path::new("-") {
                acdc_parser::parse_from_reader(io::stdin().lock(), &options)?
            } else {
                acdc_parser::parse_file(&args.input, &options)?
            };
            json::warnings(parsed.warnings())?;
            json::document(parsed.document(), args.format)?
        }
        InputKind::Inline => {
            let mut source = String::new();
            if args.input == Path::new("-") {
                io::stdin().lock().read_to_string(&mut source)?;
            } else {
                source = std::fs::read_to_string(&args.input)?;
            }
            let parsed = acdc_parser::parse_inline(&source, &options)?;
            json::warnings(parsed.warnings())?;
            json::inlines(parsed.inlines(), args.format)?
        }
    };
    json::write(&value, &args.output)
}
