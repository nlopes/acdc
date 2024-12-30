/// Process TCK requests
///
/// Currently we support only through `stdin` requests.
///
/// We expect `json` of the following format:
///
/// ```json
/// {
///   "contents": "...",
///   "path": "/a/path/to/the/input/file",
///   "type": "block" # "block" or "inline"
/// }
/// ```
///
/// NOTE: at present we do not do anything with the type - we simply pass the contents to
/// the parser and write the output to `stdout`.
use std::io::{self, BufReader, Write};

use acdc_converters_common::{Config, Processable, Source};
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Deserialize(#[from] serde_json::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Parse(#[from] acdc_parser::Error),
}

#[derive(Debug, Deserialize)]
struct TckInput {
    contents: String,
    path: String,
    r#type: String,
}

#[derive(Debug)]
pub struct Processor {
    config: Config,
}

impl Processable for Processor {
    type Config = Config;
    type Error = Error;

    #[must_use]
    fn new(config: Config) -> Self {
        Self { config }
    }

    #[tracing::instrument]
    fn run(&self) -> Result<(), Error> {
        if self.config.source != Source::Stdin {
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "only stdin is supported",
            )));
        }
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        let tck_input: TckInput = serde_json::from_reader(&mut reader)?;
        tracing::debug!(
            path = tck_input.path,
            r#type = tck_input.r#type,
            "processing TCK input",
        );
        let doc = acdc_parser::parse(&tck_input.contents)?;
        let mut stdout = io::stdout();
        serde_json::to_writer(&stdout, &doc)?;
        stdout.flush()?;
        Ok(())
    }

    fn output(&self) -> Result<String, Self::Error> {
        unimplemented!("output purposefully not implemented for the tck processor")
    }
}
