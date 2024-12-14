use std::path::Path;

use acdc_core::{Config, Processable};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Parse(#[from] acdc_parser::Error),
}

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

    fn process_files<P: AsRef<Path>>(&self, files: &[P]) -> Result<(), Error> {
        for file in files {
            acdc_parser::parse_file(file)?;
        }
        Ok(())
    }
}
