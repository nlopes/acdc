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
        for (i, block) in self.blocks.iter().enumerate() {
            block.render(w)?;
            if i != self.blocks.len() - 1 {
                writeln!(w)?;
            }
        }
        Ok(())
    }
}

impl Render for acdc_parser::Header {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        if let Some(title) = &self.title {
            title.render(w)?;
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
        Ok(())
    }
}

impl Render for acdc_parser::Title {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        writeln!(w, "{}", self.title.clone().bold().white())?;
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
    use std::collections::HashMap;

    use super::*;
    use acdc_parser::{
        Author, Block, BlockMetadata, Document, Header, InlineNode, Location, Paragraph, PlainText,
        Section, Title,
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
        let mut title = Title::default();
        title.title = "Title".to_string();
        doc.header = Some(Header {
            title: Some(title),
            subtitle: None,
            authors: vec![Author {
                first_name: "John".to_string(),
                middle_name: Some("M".to_string()),
                last_name: "Doe".to_string(),
                email: Some("johndoe@example.com".to_string()),
            }],
            location: Location::default(),
        });
        doc.blocks = vec![];
        let mut buffer = Vec::new();
        doc.render(&mut buffer).unwrap();
        assert_eq!(buffer, b"\x1b[38;5;15m\x1b[1mTitle\x1b[0m\n\x1b[3mby \x1b[0m\x1b[3mJohn \x1b[0m\x1b[3mM \x1b[0m\x1b[3mDoe\x1b[0m\x1b[3m <johndoe@example.com>\x1b[0m\n");
    }

    #[test]
    fn test_render_document_with_blocks() {
        let mut doc = Document::default();
        doc.blocks = vec![
            Block::Paragraph(Paragraph {
                content: vec![InlineNode::PlainText(PlainText {
                    content: "Hello, world!".to_string(),
                    location: Location::default(),
                })],
                admonition: None,
                location: Location::default(),
                attributes: HashMap::new(),
                metadata: BlockMetadata::default(),
                title: None,
            }),
            Block::Section(Section {
                title: "Section".to_string(),
                content: vec![Block::Paragraph(Paragraph {
                    content: vec![InlineNode::PlainText(PlainText {
                        content: "Hello, section!".to_string(),
                        location: Location::default(),
                    })],
                    location: Location::default(),
                    attributes: HashMap::new(),
                    metadata: BlockMetadata::default(),
                    admonition: None,
                    title: None,
                })],
                location: Location::default(),
                attributes: HashMap::new(),
                level: 1,
                metadata: BlockMetadata::default(),
            }),
        ];
        let mut buffer = Vec::new();
        doc.render(&mut buffer).unwrap();
        assert_eq!(
            buffer,
            b"\nHello, world!\n\n> \x1b[38;5;15m\x1b[1mSection\x1b[0m <\n\nHello, section!"
        );
    }
}