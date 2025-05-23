use std::io::Write;

use acdc_converters_common::{Options, Processable};
use acdc_core::Source;
use acdc_parser::Document;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Parse(#[from] acdc_parser::Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    IntoInner(#[from] std::io::IntoInnerError<std::io::BufWriter<Vec<u8>>>),
}

trait ToTerminal: Render {
    fn to_terminal(&self) -> std::io::Result<()> {
        let stdout = std::io::stdout();
        let mut writer = std::io::BufWriter::new(stdout.lock());
        self.render(&mut writer)?;
        writer.flush()?;
        Ok(())
    }
}

/// A simple trait for helping in rendering `AsciiDoc` content.
trait Render {
    #[allow(clippy::missing_errors_doc)]
    fn render(&self, w: &mut impl std::io::Write) -> std::io::Result<()>;
}

pub struct Processor {
    options: Options,
}

impl ToTerminal for Document {}

impl Processable for Processor {
    type Options = Options;
    type Error = Error;

    #[must_use]
    fn new(options: Options) -> Self {
        Self { options }
    }

    fn run(&self) -> Result<(), Error> {
        let options = acdc_parser::Options {
            safe_mode: self.options.safe_mode.clone(),
            timings: self.options.timings,
            document_attributes: self.options.document_attributes.clone(),
        };
        match &self.options.source {
            Source::Files(files) => {
                for file in files {
                    acdc_parser::parse_file(file, &options)?.to_terminal()?;
                }
            }
            Source::String(content) => {
                acdc_parser::parse(content, &options)?.to_terminal()?;
            }
            Source::Stdin => {
                let stdin = std::io::stdin();
                let mut reader = std::io::BufReader::new(stdin.lock());
                acdc_parser::parse_from_reader(&mut reader, &options)?.to_terminal()?;
            }
        }

        Ok(())
    }

    fn output(&self) -> Result<String, Self::Error> {
        let options = acdc_parser::Options {
            safe_mode: self.options.safe_mode.clone(),
            timings: self.options.timings,
            document_attributes: self.options.document_attributes.clone(),
        };
        match &self.options.source {
            Source::Files(files) => {
                let buffer = Vec::new();
                let mut writer = std::io::BufWriter::new(buffer);
                for file in files {
                    let doc = acdc_parser::parse_file(file, &options)?;
                    doc.render(&mut writer)?;
                }
                writer.flush()?;
                Ok(String::from_utf8(writer.into_inner()?)?)
            }
            Source::String(content) => {
                let doc = acdc_parser::parse(content, &options)?;
                let buffer = Vec::new();
                let mut writer = std::io::BufWriter::new(buffer);
                doc.render(&mut writer)?;
                writer.flush()?;
                Ok(String::from_utf8(writer.into_inner()?)?)
            }
            Source::Stdin => {
                let stdin = std::io::stdin();
                let mut reader = std::io::BufReader::new(stdin.lock());
                let doc = acdc_parser::parse_from_reader(&mut reader, &options)?;
                let buffer = Vec::new();
                let mut writer = std::io::BufWriter::new(buffer);
                doc.render(&mut writer)?;
                writer.flush()?;
                Ok(String::from_utf8(writer.into_inner()?)?)
            }
        }
    }
}

mod block;
mod delimited;
mod document;
mod inline;
mod list;
mod paragraph;
mod section;
mod table;
