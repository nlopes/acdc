use std::io::Write;

use crossterm::{
    style::{Print, PrintStyledContent, Stylize},
    QueueableCommand,
};

use crate::Render;

impl Render for acdc_parser::Document {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        if let Some(header) = &self.header {
            header.render(w)?;
        }
        let last_index = self.blocks.len() - 1;
        for (i, block) in self.blocks.iter().enumerate() {
            block.render(w)?;
            if i != last_index {
                writeln!(w)?;
            }
        }
        Ok(())
    }
}

impl Render for acdc_parser::Header {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        for node in &self.title {
            node.render(w)?;
        }
        if !self.authors.is_empty() {
            w.queue(PrintStyledContent("by ".italic()))?;
            // Join the authors with commas, except for the last one, using a functional approach
            self.authors
                .iter()
                .enumerate()
                .try_for_each(|(i, author)| {
                    author.render(w)?;
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
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
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
        let mut buffer = Vec::new();
        doc.render(&mut buffer).unwrap();
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
        doc.render(&mut buffer).unwrap();
        assert_eq!(buffer, b"Title\x1b[3mby \x1b[0m\x1b[3mJohn \x1b[0m\x1b[3mM \x1b[0m\x1b[3mDoe\x1b[0m\x1b[3m <johndoe@example.com>\x1b[0m\n");
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
        doc.render(&mut buffer).unwrap();
        assert_eq!(buffer, b"\nHello, world!\n\n> Section <\n\nHello, section!");
    }
}
