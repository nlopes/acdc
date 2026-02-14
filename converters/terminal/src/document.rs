use std::io::{self, BufWriter, Write};

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{AttributeValue, Author};
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
    let cloned_processor = processor.clone();
    let buffer = Vec::new();
    let inner = BufWriter::new(buffer);
    let mut temp_visitor = TerminalVisitor::new(inner, cloned_processor);

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

    // Render revision info if present
    let revnumber = processor.document_attributes.get("revnumber");
    let revdate = processor.document_attributes.get("revdate");
    let revremark = processor.document_attributes.get("revremark");

    if revnumber.is_some() || revdate.is_some() {
        if let Some(AttributeValue::String(revnumber)) = revnumber {
            // Strip leading "v" if present (asciidoctor behavior)
            let version = revnumber.strip_prefix('v').unwrap_or(revnumber);
            w.queue(PrintStyledContent(format!("version {version}").dim()))?;
            if revdate.is_some() {
                w.queue(PrintStyledContent(", ".dim()))?;
            }
        }
        if let Some(AttributeValue::String(revdate)) = revdate {
            w.queue(PrintStyledContent(revdate.clone().dim()))?;
        }
        writeln!(w)?;
        if let Some(AttributeValue::String(revremark)) = revremark {
            w.queue(PrintStyledContent(revremark.clone().dim().italic()))?;
            writeln!(w)?;
        }
    }

    w.queue(Print("\n"))?;
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
        Author, Block, Document, Header, InlineNode, Location, Paragraph, Plain, Section, Title,
    };

    #[test]
    fn test_render_document() -> Result<(), Error> {
        use crate::Appearance;
        use acdc_converters_core::section::{
            AppendixTracker, PartNumberTracker, SectionNumberTracker,
        };
        use std::{cell::Cell, rc::Rc};
        let doc = Document::default();
        let options = Options::default();
        let appearance = Appearance::detect();
        let section_number_tracker = SectionNumberTracker::new(&doc.attributes);
        let part_number_tracker =
            PartNumberTracker::new(&doc.attributes, section_number_tracker.clone());
        let appendix_tracker =
            AppendixTracker::new(&doc.attributes, section_number_tracker.clone());
        let processor = Processor {
            options,
            document_attributes: doc.attributes.clone(),
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
            appearance,
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            terminal_width: crate::FALLBACK_TERMINAL_WIDTH,
            index_entries: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            has_valid_index_section: false,
            list_indent: std::rc::Rc::new(std::cell::Cell::new(0)),
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
        use crate::Appearance;
        let mut doc = Document::default();
        let title = Title::new(vec![InlineNode::PlainText(Plain {
            content: "Title".to_string(),
            location: Location::default(),
            escaped: false,
        })]);
        doc.header = Some(Header::new(title, Location::default()).with_authors(vec![
            Author::new("John", Some("M"), Some("Doe"))
                    .with_email("johndoe@example.com".to_string()),
            ]));
        doc.blocks = vec![];
        let buffer = Vec::new();
        let options = Options::default();
        let appearance = Appearance::detect();
        let section_number_tracker =
            acdc_converters_core::section::SectionNumberTracker::new(&doc.attributes);
        let part_number_tracker = acdc_converters_core::section::PartNumberTracker::new(
            &doc.attributes,
            section_number_tracker.clone(),
        );
        let appendix_tracker = acdc_converters_core::section::AppendixTracker::new(
            &doc.attributes,
            section_number_tracker.clone(),
        );
        let processor = Processor {
            options,
            document_attributes: doc.attributes.clone(),
            toc_entries: vec![],
            example_counter: std::rc::Rc::new(std::cell::Cell::new(0)),
            appearance,
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            terminal_width: crate::FALLBACK_TERMINAL_WIDTH,
            index_entries: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            has_valid_index_section: false,
            list_indent: std::rc::Rc::new(std::cell::Cell::new(0)),
        };
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_document(&doc)?;
        let buffer = visitor.into_writer();
        assert_eq!(buffer, b"\x1b[1m\x1b[4mTitle\x1b[0m\n\x1b[3mby \x1b[0m\x1b[3mJohn \x1b[0m\x1b[3mM \x1b[0m\x1b[3mDoe\x1b[0m\x1b[3m <johndoe@example.com>\x1b[0m\n\n");
        Ok(())
    }

    #[test]
    fn test_render_document_with_blocks() -> Result<(), Error> {
        use crate::Appearance;
        let mut doc = Document::default();
        doc.blocks = vec![
            Block::Paragraph(Paragraph::new(
                vec![InlineNode::PlainText(Plain {
                    content: "Hello, world!".to_string(),
                    location: Location::default(),
                    escaped: false,
                })],
                Location::default(),
            )),
            Block::Section(Section::new(
                Title::new(vec![InlineNode::PlainText(Plain {
                    content: "Section".to_string(),
                    location: Location::default(),
                    escaped: false,
                })]),
                1,
                vec![Block::Paragraph(Paragraph::new(
                    vec![InlineNode::PlainText(Plain {
                        content: "Hello, section!".to_string(),
                        location: Location::default(),
                        escaped: false,
                    })],
                    Location::default(),
                ))],
                Location::default(),
            )),
        ];
        let buffer = Vec::new();
        let options = Options::default();
        let appearance = Appearance::detect();
        let section_number_tracker =
            acdc_converters_core::section::SectionNumberTracker::new(&doc.attributes);
        let part_number_tracker = acdc_converters_core::section::PartNumberTracker::new(
            &doc.attributes,
            section_number_tracker.clone(),
        );
        let appendix_tracker = acdc_converters_core::section::AppendixTracker::new(
            &doc.attributes,
            section_number_tracker.clone(),
        );
        let processor = Processor {
            options,
            document_attributes: doc.attributes.clone(),
            toc_entries: vec![],
            example_counter: std::rc::Rc::new(std::cell::Cell::new(0)),
            appearance,
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            terminal_width: crate::FALLBACK_TERMINAL_WIDTH,
            index_entries: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            has_valid_index_section: false,
            list_indent: std::rc::Rc::new(std::cell::Cell::new(0)),
        };
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_document(&doc)?;
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
