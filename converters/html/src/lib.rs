use std::{
    io::{BufWriter, Write},
    path::Path,
};

use acdc_converters_common::{Options, PrettyDuration, Processable};
use acdc_core::Source;
use acdc_parser::{Document, DocumentAttributes, TocEntry};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Parse(#[from] acdc_parser::Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),
}

#[derive(Clone, Debug)]
pub struct Processor {
    options: Options,
    pub document_attributes: DocumentAttributes,
    pub toc_entries: Vec<TocEntry>,
}

impl Processor {
    fn to_file<P: AsRef<Path>>(
        &self,
        doc: &Document,
        _original: P,
        path: P,
    ) -> Result<(), crate::Error> {
        let mut file = std::fs::File::create(path)?;
        let options = RenderOptions {
            last_updated: file.metadata()?.modified().ok().map(chrono::DateTime::from),
            ..RenderOptions::default()
        };
        let mut writer = BufWriter::new(&mut file);
        let processor = Processor {
            toc_entries: doc.toc_entries.clone(),
            document_attributes: doc.attributes.clone(),
            ..self.clone()
        };
        doc.render(&mut writer, &processor, &options)?;
        writer.flush()?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct RenderOptions {
    last_updated: Option<chrono::DateTime<chrono::Utc>>,
    inlines_basic: bool,
    inlines_substitutions: bool,
}

/// A simple trait for helping in rendering `AsciiDoc` content.
trait Render {
    type Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error>;
}

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

    fn run(&self) -> Result<(), Self::Error> {
        let options = acdc_parser::Options {
            safe_mode: self.options.safe_mode.clone(),
            timings: self.options.timings,
            document_attributes: self.document_attributes.clone(),
        };

        match &self.options.source {
            Source::Files(files) => {
                for file in files {
                    if self.options.timings {
                        println!("Input file: {}", file.to_string_lossy());
                    }
                    let html_path = file.with_extension("html");
                    tracing::debug!(source = ?file, destination = ?html_path, "processing file");

                    // Read and parse the document
                    let now = std::time::Instant::now();
                    let mut total_elapsed = std::time::Duration::new(0, 0);
                    let doc = acdc_parser::parse_file(file, &options)?;
                    let elapsed = now.elapsed();
                    tracing::debug!(time = elapsed.pretty_print_precise(3), source = ?file, destination = ?html_path, "time to read and parse source");
                    total_elapsed += elapsed;
                    if self.options.timings {
                        println!(
                            "  Time to read and parse source: {}",
                            elapsed.pretty_print()
                        );
                    }

                    // Convert the document
                    let now = std::time::Instant::now();
                    self.to_file(&doc, file, &html_path)?;
                    let elapsed = now.elapsed();
                    tracing::debug!(time = elapsed.pretty_print_precise(3), source = ?file, destination = ?html_path, "time to convert document");
                    total_elapsed += elapsed;
                    tracing::debug!(time = total_elapsed.pretty_print_precise(3), source = ?file, destination = ?html_path, "total time (read, parse and convert)");
                    if self.options.timings {
                        println!("  Time to convert document: {}", elapsed.pretty_print());
                        println!(
                            "  Total time (read, parse and convert): {}",
                            total_elapsed.pretty_print()
                        );
                    }
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
        let mut render_options = RenderOptions {
            ..RenderOptions::default()
        };
        let options = acdc_parser::Options {
            safe_mode: self.options.safe_mode.clone(),
            timings: self.options.timings,
            document_attributes: self.document_attributes.clone(),
        };
        match &self.options.source {
            Source::Files(files) => {
                let mut buffer = Vec::new();
                for file in files {
                    render_options.last_updated = std::fs::metadata(file)?
                        .modified()
                        .ok()
                        .map(chrono::DateTime::from);
                    acdc_parser::parse_file(file, &options)?.render(
                        &mut buffer,
                        self,
                        &render_options,
                    )?;
                }
                Ok(String::from_utf8(buffer)?)
            }
            Source::String(content) => {
                let mut buffer = Vec::new();
                acdc_parser::parse(content, &options)?.render(
                    &mut buffer,
                    self,
                    &render_options,
                )?;
                Ok(String::from_utf8(buffer)?)
            }
            Source::Stdin => {
                let stdin = std::io::stdin();
                let mut reader = std::io::BufReader::new(stdin.lock());
                let doc = acdc_parser::parse_from_reader(&mut reader, &options)?;
                let mut buffer = Vec::new();
                doc.render(&mut buffer, self, &render_options)?;
                Ok(String::from_utf8(buffer)?)
            }
        }
    }
}

mod admonition;
mod block;
mod delimited;
mod document;
mod image;
mod inlines;
mod list;
mod paragraph;
mod section;
mod table;
mod toc;
