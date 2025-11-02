use std::io::{self, BufWriter, Write};

use acdc_converters_common::visitor::{Visitor, WritableVisitor};
use acdc_parser::Author;
use crossterm::{
    QueueableCommand,
    style::{Print, PrintStyledContent, Stylize},
};

use crate::{Error, TerminalVisitor};

pub(crate) fn visit_header<V: WritableVisitor<Error = Error>>(
    header: &acdc_parser::Header,
    visitor: &mut V,
    processor: &crate::Processor,
) -> Result<(), Error> {
    let processor = processor.clone();
    let buffer = Vec::new();
    let inner = BufWriter::new(buffer);
    let mut temp_visitor = TerminalVisitor::new(inner, processor);

    for node in &header.title {
        temp_visitor.visit_inline_node(node)?;
    }
    if let Some(subtitle) = &header.subtitle {
        let w = temp_visitor.writer_mut();
        write!(w, ": ")?;
        let _ = w;
        for node in subtitle {
            temp_visitor.visit_inline_node(node)?;
        }
    }

    let buffer = temp_visitor
        .into_writer()
        .into_inner()
        .map_err(io::IntoInnerError::into_error)?;
    let title_content = String::from_utf8(buffer)
        .map_err(|e| {
            tracing::error!(?e, "Failed to convert document title to UTF-8 string");
            e
        })
        .unwrap_or_default()
        .trim()
        .to_string();

    let w = visitor.writer_mut();
    w.queue(PrintStyledContent(title_content.bold().underlined()))?;

    if !header.authors.is_empty() {
        writeln!(w)?;
        w.queue(PrintStyledContent("by ".italic()))?;
        // Join the authors with commas, except for the last one, using a functional approach
        header
            .authors
            .iter()
            .enumerate()
            .try_for_each(|(i, author)| {
                visit_author(author, w)?;
                if i != header.authors.len() - 1 {
                    w.queue(Print(", "))?;
                }
                Ok::<(), io::Error>(())
            })?;
        writeln!(w)?;
    }
    w.queue(Print("\n\n"))?;
    Ok(())
}

fn visit_author<W: Write + ?Sized>(author: &Author, w: &mut W) -> Result<(), io::Error> {
    w.queue(PrintStyledContent(
        format!("{} ", author.first_name).italic(),
    ))?;
    if let Some(middle_name) = &author.middle_name {
        w.queue(PrintStyledContent(format!("{middle_name} ").italic()))?;
    }
    w.queue(PrintStyledContent(author.last_name.clone().italic()))?;
    if let Some(email) = &author.email {
        w.queue(PrintStyledContent(format!(" <{email}>").italic()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Options, Processor};
    use acdc_parser::{
        Author, Block, BlockMetadata, Document, Header, InlineNode, Location, Paragraph, Plain,
        Section,
    };

    #[test]
    fn test_render_document() -> Result<(), Error> {
        use std::{cell::Cell, rc::Rc};
        let doc = Document::default();
        let options = Options::default();
        let processor = Processor {
            options,
            document_attributes: doc.attributes.clone(),
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
        };
        let buffer = Vec::new();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_document(&doc)?;
        let buffer = visitor.into_writer();
        assert_eq!(buffer, b"");
        Ok(())
    }

    #[test]
    fn test_render_document_with_header() -> Result<(), Error> {
        let mut doc = Document::default();
        let title = vec![InlineNode::PlainText(Plain {
            content: "Title".to_string(),
            location: Location::default(),
        })];
        doc.header = Some(Header {
            title,
            subtitle: None,
            authors: vec![Author {
                first_name: "John".to_string(),
                middle_name: Some("M".to_string()),
                last_name: "Doe".to_string(),
                initials: "JMD".to_string(),
                email: Some("johndoe@example.com".to_string()),
            }],
            location: Location::default(),
        });
        doc.blocks = vec![];
        let buffer = Vec::new();
        let options = Options::default();
        let processor = Processor {
            options,
            document_attributes: doc.attributes.clone(),
            toc_entries: vec![],
            example_counter: std::rc::Rc::new(std::cell::Cell::new(0)),
        };
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_document(&doc)?;
        let buffer = visitor.into_writer();
        assert_eq!(buffer, b"\x1b[1m\x1b[4mTitle\x1b[0m\n\x1b[3mby \x1b[0m\x1b[3mJohn \x1b[0m\x1b[3mM \x1b[0m\x1b[3mDoe\x1b[0m\x1b[3m <johndoe@example.com>\x1b[0m\n\n\n");
        Ok(())
    }

    #[test]
    fn test_render_document_with_blocks() -> Result<(), Error> {
        let mut doc = Document::default();
        doc.blocks = vec![
            Block::Paragraph(Paragraph {
                content: vec![InlineNode::PlainText(Plain {
                    content: "Hello, world!".to_string(),
                    location: Location::default(),
                })],
                location: Location::default(),
                metadata: BlockMetadata::default(),
                title: Vec::new(),
            }),
            Block::Section(Section {
                title: vec![InlineNode::PlainText(Plain {
                    content: "Section".to_string(),
                    location: Location::default(),
                })],
                content: vec![Block::Paragraph(Paragraph {
                    content: vec![InlineNode::PlainText(Plain {
                        content: "Hello, section!".to_string(),
                        location: Location::default(),
                    })],
                    location: Location::default(),
                    metadata: BlockMetadata::default(),
                    title: Vec::new(),
                })],
                location: Location::default(),
                level: 1,
                metadata: BlockMetadata::default(),
            }),
        ];
        let buffer = Vec::new();
        let options = Options::default();
        let processor = Processor {
            options,
            document_attributes: doc.attributes.clone(),
            toc_entries: vec![],
            example_counter: std::rc::Rc::new(std::cell::Cell::new(0)),
        };
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_document(&doc)?;
        let buffer = visitor.into_writer();
        assert_eq!(buffer, b"Hello, world!\n\n> Section <\nHello, section!\n\n");
        Ok(())
    }
}
