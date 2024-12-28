use std::{io::Write, path::Path};

use acdc_backends_common::{Config, Processable};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Parse(#[from] acdc_parser::Error),
}

/// A simple trait for helping in rendering `AsciiDoc` content.
pub trait Render {
    #[allow(clippy::missing_errors_doc)]
    fn render(&self, w: &mut impl std::io::Write) -> std::io::Result<()>;
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

    fn run(&self) -> Result<(), Error> {
        for file in &self.config.files {
            parse_file(file)?;
        }
        Ok(())
    }
}

/// Parses a file and renders it to the terminal.
///
/// # Errors
///
/// Will return parsing or rendering errors.
fn parse_file<P: AsRef<Path>>(file: P) -> Result<(), Error> {
    let doc = acdc_parser::parse_file(file)?;
    let mut stdout = std::io::stdout();
    doc.render(&mut stdout)?;
    stdout.flush()?;
    Ok(())
}

mod block;
mod delimited;
mod document;
mod inline;
mod list;
mod paragraph;
mod section;
mod table;
