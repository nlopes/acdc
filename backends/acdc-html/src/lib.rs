use std::{
    io::{BufWriter, Write},
    path::Path,
};

use acdc_backends_common::{Config, Processable, Source};
use acdc_parser::Document;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Parse(#[from] acdc_parser::Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),
}

pub struct Processor {
    config: Config,
}

trait ToFile: Render {
    fn to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        let mut writer = BufWriter::new(&mut file);
        self.render(&mut writer)?;
        writer.flush()?;
        Ok(())
    }
}

/// A simple trait for helping in rendering `AsciiDoc` content.
trait Render {
    fn render(&self, w: &mut impl std::io::Write) -> std::io::Result<()>;
}

impl ToFile for Document {}

impl Processable for Processor {
    type Config = Config;
    type Error = Error;

    #[must_use]
    fn new(config: Config) -> Self {
        Self { config }
    }

    fn run(&self) -> Result<(), Error> {
        match &self.config.source {
            Source::Files(files) => {
                for file in files {
                    let html_path = file.with_extension("html");
                    tracing::debug!(source = ?file, destination = ?html_path, "processing file");
                    acdc_parser::parse_file(file)?.to_file(&html_path)?;
                    println!("Generated HTML file: {}", html_path.to_string_lossy());
                }
            }
            _ => {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "only files are supported",
                )));
            }
        }
        Ok(())
    }

    fn output(&self) -> Result<String, Self::Error> {
        match &self.config.source {
            Source::Files(files) => {
                let mut buffer = Vec::new();
                for file in files {
                    acdc_parser::parse_file(file)?.render(&mut buffer)?;
                }
                Ok(String::from_utf8(buffer)?)
            }
            Source::String(content) => {
                let mut buffer = Vec::new();
                acdc_parser::parse(content)?.render(&mut buffer)?;
                Ok(String::from_utf8(buffer)?)
            }
            Source::Stdin => {
                let stdin = std::io::stdin();
                let mut reader = std::io::BufReader::new(stdin.lock());
                let doc = acdc_parser::parse_from_reader(&mut reader)?;
                let mut buffer = Vec::new();
                doc.render(&mut buffer)?;
                Ok(String::from_utf8(buffer)?)
            }
        }
    }
}

impl Render for Document {
    fn render(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        write!(w, "Norberto")?;
        Ok(())
    }
}
