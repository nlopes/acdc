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

impl Processor {
    fn to_file<P: AsRef<Path>>(&self, doc: &Document, original: P, path: P) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        let mut writer = BufWriter::new(&mut file);
        let options = RenderOptions {
            last_updated: std::fs::metadata(original)?
                .modified()
                .ok()
                .map(chrono::DateTime::from),
            ..RenderOptions::default()
        };
        doc.render(&mut writer, self, &options)?;
        writer.flush()?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct RenderOptions {
    last_updated: Option<chrono::DateTime<chrono::Utc>>,
    inlines_basic: bool,
}

/// A simple trait for helping in rendering `AsciiDoc` content.
trait Render {
    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> std::io::Result<()>;
}

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
                    self.to_file(&acdc_parser::parse_file(file)?, file, &html_path)?;
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
        let mut options = RenderOptions {
            ..RenderOptions::default()
        };
        match &self.config.source {
            Source::Files(files) => {
                let mut buffer = Vec::new();
                for file in files {
                    options.last_updated = std::fs::metadata(file)?
                        .modified()
                        .ok()
                        .map(chrono::DateTime::from);
                    acdc_parser::parse_file(file)?.render(&mut buffer, self, &options)?;
                }
                Ok(String::from_utf8(buffer)?)
            }
            Source::String(content) => {
                let mut buffer = Vec::new();
                acdc_parser::parse(content)?.render(&mut buffer, self, &options)?;
                Ok(String::from_utf8(buffer)?)
            }
            Source::Stdin => {
                let stdin = std::io::stdin();
                let mut reader = std::io::BufReader::new(stdin.lock());
                let doc = acdc_parser::parse_from_reader(&mut reader)?;
                let mut buffer = Vec::new();
                doc.render(&mut buffer, self, &options)?;
                Ok(String::from_utf8(buffer)?)
            }
        }
    }
}

mod block;
mod delimited;
mod document;
mod inlines;
mod list;
mod paragraph;
mod section;
mod table;
