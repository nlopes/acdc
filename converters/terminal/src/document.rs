use std::io::Write;

use crossterm::{
    QueueableCommand,
    style::{Print, PrintStyledContent, Stylize},
};

use crate::{Processor, Render};

impl Render for acdc_parser::Document {
    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> std::io::Result<()> {
        if let Some(header) = &self.header {
            header.render(w, processor)?;
        }
        if !self.blocks.is_empty() {
            let last_index = self.blocks.len() - 1;
            for (i, block) in self.blocks.iter().enumerate() {
                block.render(w, processor)?;
                if i != last_index {
                    writeln!(w)?;
                }
            }
        }

        // Render footnotes at the end of the document if any exist
        if !self.footnotes.is_empty() {
            writeln!(w)?;
            writeln!(w, "─────")?; // Simple separator
            for footnote in &self.footnotes {
                footnote.render(w, processor)?;
                writeln!(w)?;
            }
        }

        Ok(())
    }
}

impl Render for acdc_parser::Header {
    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> std::io::Result<()> {
        for node in &self.title {
            // Collect title content for styling
            let mut title_buffer = std::io::BufWriter::new(Vec::new());
            node.render(&mut title_buffer, processor)?;
            title_buffer.flush()?;
            let title_content = String::from_utf8(title_buffer.get_ref().clone())
                .map_err(|e| {
                    tracing::error!(?e, "Failed to convert document title to UTF-8 string");
                    e
                })
                .unwrap_or_default()
                .trim()
                .to_string();
            w.queue(PrintStyledContent(title_content.bold().underlined()))?;
        }

        if !self.authors.is_empty() {
            w.queue(PrintStyledContent("by ".italic()))?;
            // Join the authors with commas, except for the last one, using a functional approach
            self.authors
                .iter()
                .enumerate()
                .try_for_each(|(i, author)| {
                    author.render(w, processor)?;
                    if i != self.authors.len() - 1 {
                        w.queue(Print(", "))?;
                    }
                    Ok::<(), std::io::Error>(())
                })?;
            writeln!(w)?;
        }
        w.queue(Print("\n\n"))?;
        Ok(())
    }
}

impl Render for acdc_parser::Author {
    fn render<W: Write>(&self, w: &mut W, _processor: &Processor) -> std::io::Result<()> {
        w.queue(PrintStyledContent(format!("{} ", self.first_name).italic()))?;
        if let Some(middle_name) = &self.middle_name {
            w.queue(PrintStyledContent(format!("{middle_name} ").italic()))?;
        }
        w.queue(PrintStyledContent(self.last_name.clone().italic()))?;
        if let Some(email) = &self.email {
            w.queue(PrintStyledContent(format!(" <{email}>").italic()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::{
        Author, Block, BlockMetadata, Document, Header, InlineNode, Location, Paragraph, Plain,
        Section,
    };

    #[test]
    fn test_render_document() {
        let doc = Document::default();
        let options = crate::Options::default();
        let processor = crate::Processor { options };
        let mut buffer = Vec::new();
        doc.render(&mut buffer, &processor).unwrap();
        assert_eq!(buffer, b"");
    }

    #[test]
    fn test_render_document_with_header() {
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
        let mut buffer = Vec::new();
        let options = crate::Options::default();
        let processor = crate::Processor { options };
        doc.render(&mut buffer, &processor).unwrap();
        assert_eq!(buffer, b"\x1b[1m\x1b[4mTitle\x1b[0m\x1b[3mby \x1b[0m\x1b[3mJohn \x1b[0m\x1b[3mM \x1b[0m\x1b[3mDoe\x1b[0m\x1b[3m <johndoe@example.com>\x1b[0m\n\n\n");
    }

    #[test]
    fn test_render_document_with_blocks() {
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
        let mut buffer = Vec::new();
        let options = crate::Options::default();
        let processor = crate::Processor { options };
        doc.render(&mut buffer, &processor).unwrap();
        assert_eq!(buffer, b"Hello, world!\n\n> Section <\nHello, section!\n\n");
    }
}
