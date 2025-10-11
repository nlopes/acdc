use std::io::Write;

use acdc_converters_common::{Options, Processable};
use acdc_core::Source;
use acdc_parser::{Document, DocumentAttributes, TocEntry};

trait ToTerminal: Render<Error = crate::Error> {
    fn to_terminal(&self, processor: &Processor) -> Result<(), Self::Error> {
        let stdout = std::io::stdout();
        let mut writer = std::io::BufWriter::new(stdout.lock());
        self.render(&mut writer, processor)?;
        writer.flush()?;
        Ok(())
    }
}

pub(crate) const FALLBACK_TERMINAL_WIDTH: usize = 80;

/// A simple trait for helping in rendering `AsciiDoc` content.
trait Render {
    type Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error>;
}

pub struct Processor {
    options: Options,
    document_attributes: DocumentAttributes,
    toc_entries: Vec<TocEntry>,
}

impl ToTerminal for Document {}

impl Processable for Processor {
    type Options = Options;
    type Error = Error;

    fn new(options: Options, document_attributes: DocumentAttributes) -> Self {
        Self {
            options,
            document_attributes,
            toc_entries: vec![],
        }
    }

    fn run(&self) -> Result<(), Error> {
        let options = acdc_parser::Options {
            safe_mode: self.options.safe_mode.clone(),
            timings: self.options.timings,
            document_attributes: self.document_attributes.clone(),
        };
        match &self.options.source {
            Source::Files(files) => {
                for file in files {
                    acdc_parser::parse_file(file, &options)?.to_terminal(self)?;
                }
            }
            Source::String(content) => {
                acdc_parser::parse(content, &options)?.to_terminal(self)?;
            }
            Source::Stdin => {
                let stdin = std::io::stdin();
                let mut reader = std::io::BufReader::new(stdin.lock());
                acdc_parser::parse_from_reader(&mut reader, &options)?.to_terminal(self)?;
            }
        }

        Ok(())
    }

    fn output(&self) -> Result<String, Self::Error> {
        let options = acdc_parser::Options {
            safe_mode: self.options.safe_mode.clone(),
            timings: self.options.timings,
            document_attributes: self.document_attributes.clone(),
        };
        match &self.options.source {
            Source::Files(files) => {
                let buffer = Vec::new();
                let mut writer = std::io::BufWriter::new(buffer);
                for file in files {
                    let doc = acdc_parser::parse_file(file, &options)?;
                    let processor = Processor {
                        options: self.options.clone(),
                        document_attributes: doc.attributes.clone(),
                        toc_entries: doc.toc_entries.clone(),
                    };
                    doc.render(&mut writer, &processor)?;
                }
                writer.flush()?;
                Ok(String::from_utf8(writer.into_inner()?)?)
            }
            Source::String(content) => {
                let doc = acdc_parser::parse(content, &options)?;
                let processor = Processor {
                    options: self.options.clone(),
                    document_attributes: doc.attributes.clone(),
                    toc_entries: doc.toc_entries.clone(),
                };
                let buffer = Vec::new();
                let mut writer = std::io::BufWriter::new(buffer);
                doc.render(&mut writer, &processor)?;
                writer.flush()?;
                Ok(String::from_utf8(writer.into_inner()?)?)
            }
            Source::Stdin => {
                let stdin = std::io::stdin();
                let mut reader = std::io::BufReader::new(stdin.lock());
                let doc = acdc_parser::parse_from_reader(&mut reader, &options)?;
                let processor = Processor {
                    options: self.options.clone(),
                    document_attributes: doc.attributes.clone(),
                    toc_entries: doc.toc_entries.clone(),
                };
                let buffer = Vec::new();
                let mut writer = std::io::BufWriter::new(buffer);
                doc.render(&mut writer, &processor)?;
                writer.flush()?;
                Ok(String::from_utf8(writer.into_inner()?)?)
            }
        }
    }
}

mod block;
mod delimited;
mod document;
mod error;
mod image;
mod inline;
mod list;
mod page_break;
mod paragraph;
mod section;
mod table;
mod thematic_break;

pub(crate) use error::Error;
