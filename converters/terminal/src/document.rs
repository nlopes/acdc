use std::io::{self, BufWriter, Write};

use acdc_converters_core::{
    Converter, TraversalContext,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::Author;
use crossterm::{
    QueueableCommand,
    style::{Print, PrintStyledContent, Stylize},
};

use crate::{Error, TerminalVisitor};

impl<'a, W: Write> TerminalVisitor<'a, '_, W> {
    pub(crate) fn render_header(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        header: &acdc_parser::Header,
    ) -> Result<(), Error> {
        let cloned_processor = self.processor;
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor =
            TerminalVisitor::new(inner, cloned_processor, self.diagnostics.reborrow());

        for node in &header.title {
            temp_visitor.visit_inline_node(traversal, node)?;
        }
        if let Some(subtitle) = &header.subtitle {
            write!(temp_visitor.writer, ": ")?;
            for node in subtitle {
                temp_visitor.visit_inline_node(traversal, node)?;
            }
        }

        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;
        let title_content = String::from_utf8(buffer)
            .map_err(|e| {
                tracing::debug!(?e, "failed to convert document title to UTF-8 string");
                e
            })
            .unwrap_or_default()
            .trim()
            .to_string();

        let processor = self.processor;
        let w = self.writer_mut();
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

        // Render revision info if present
        let text_attribute = |name| {
            processor
                .document_attributes()
                .get(name)
                .and_then(|value| value.text())
        };
        let revnumber = text_attribute("revnumber");
        let revdate = text_attribute("revdate");
        let revremark = text_attribute("revremark");

        if revnumber.is_some() || revdate.is_some() {
            if let Some(revnumber) = revnumber {
                let label = processor
                    .document_attributes()
                    .get("version-label")
                    .and_then(|value| value.text())
                    .filter(|label| !label.is_empty());
                let revision = label.map_or_else(
                    || revnumber.to_string(),
                    |label| format!("{label} {revnumber}"),
                );
                w.queue(PrintStyledContent(revision.dim()))?;
                if revdate.is_some() {
                    w.queue(PrintStyledContent(", ".dim()))?;
                }
            }
            if let Some(revdate) = revdate {
                w.queue(PrintStyledContent(revdate.to_string().dim()))?;
            }
            writeln!(w)?;
            if let Some(revremark) = revremark {
                w.queue(PrintStyledContent(revremark.to_string().dim().italic()))?;
                writeln!(w)?;
            }
        }

        w.queue(Print("\n"))?;
        Ok(())
    }
}

fn visit_author<W: Write + ?Sized>(author: &Author, w: &mut W) -> Result<(), io::Error> {
    w.queue(PrintStyledContent(
        format!("{} ", author.first_name).italic(),
    ))?;
    if let Some(middle_name) = &author.middle_name {
        w.queue(PrintStyledContent(format!("{middle_name} ").italic()))?;
    }
    w.queue(PrintStyledContent(author.last_name.italic()))?;
    if let Some(email) = &author.email {
        w.queue(PrintStyledContent(format!(" <{email}>").italic()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_test_processor_with;
    use acdc_converters_core::{Diagnostics, WarningSource};
    use acdc_parser::{
        Author, Block, Document, Header, InlineNode, Location, Paragraph, Plain, Section, Title,
    };

    #[test]
    fn test_render_document() -> Result<(), Error> {
        let doc = Document::default();
        let processor = create_test_processor_with(doc.attributes.clone());
        let buffer = Vec::new();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.visit_document(&mut traversal, &doc)?;
        let buffer = visitor.into_writer();
        assert_eq!(buffer, b"");
        Ok(())
    }

    #[test]
    fn test_render_document_with_header() -> Result<(), Error> {
        let mut doc = Document::default();
        let title = Title::new(vec![InlineNode::PlainText(Plain {
            content: "Title",
            location: Location::default(),
            escaped: false,
        })]);
        doc.header = Some(Header::new(title, Location::default()).with_authors(vec![
            Author::from_parts("John", Some("M"), "Doe", "JMD").with_email("johndoe@example.com"),
        ]));
        doc.blocks = vec![];
        let buffer = Vec::new();
        let processor = create_test_processor_with(doc.attributes.clone());
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.visit_document(&mut traversal, &doc)?;
        let buffer = visitor.into_writer();
        assert_eq!(buffer, b"\x1b[1m\x1b[4mTitle\x1b[0m\n\x1b[3mby \x1b[0m\x1b[3mJohn \x1b[0m\x1b[3mM \x1b[0m\x1b[3mDoe\x1b[0m\x1b[3m <johndoe@example.com>\x1b[0m\n\n");
        Ok(())
    }

    #[test]
    fn test_render_document_with_blocks() -> Result<(), Error> {
        let mut doc = Document::default();
        doc.blocks = vec![
            Block::Paragraph(Paragraph::new(
                vec![InlineNode::PlainText(Plain {
                    content: "Hello, world!",
                    location: Location::default(),
                    escaped: false,
                })],
                Location::default(),
            )),
            Block::Section(Section::new(
                Title::new(vec![InlineNode::PlainText(Plain {
                    content: "Section",
                    location: Location::default(),
                    escaped: false,
                })]),
                1,
                vec![Block::Paragraph(Paragraph::new(
                    vec![InlineNode::PlainText(Plain {
                        content: "Hello, section!",
                        location: Location::default(),
                        escaped: false,
                    })],
                    Location::default(),
                ))],
                Location::default(),
            )),
        ];
        let buffer = Vec::new();
        let processor = create_test_processor_with(doc.attributes.clone());
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.visit_document(&mut traversal, &doc)?;
        let buffer = visitor.into_writer();
        let output = String::from_utf8_lossy(&buffer);

        // Verify output contains expected content (with new section formatting)
        assert!(
            output.contains("Hello, world!"),
            "Should contain paragraph text"
        );
        assert!(output.contains("Section"), "Should contain section title");
        assert!(
            output.contains("Hello, section!"),
            "Should contain section content"
        );

        Ok(())
    }
}
