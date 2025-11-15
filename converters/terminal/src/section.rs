use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{DiscreteHeader, InlineNode, Section};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::Processor;

pub(crate) fn visit_section<V: WritableVisitor<Error = crate::Error>>(
    section: &Section,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), crate::Error> {
    let w = visitor.writer_mut();
    writeln!(w)?;

    match section.level {
        1 => {
            // Level 1: Short separator + Bold (mdcat style)
            let separator = "┄".with(processor.appearance.colors.section_h1).bold();
            QueueableCommand::queue(w, PrintStyledContent(separator))?;

            let title = extract_title_text(&section.title);
            let styled = title.bold().with(processor.appearance.colors.section_h1);
            QueueableCommand::queue(w, PrintStyledContent(styled))?;
            writeln!(w)?;
        }
        2 => {
            // Level 2: Double separator + Bold (mdcat style)
            let separator = "┄┄".with(processor.appearance.colors.section_h2).bold();
            QueueableCommand::queue(w, PrintStyledContent(separator))?;

            let title = extract_title_text(&section.title);
            let styled = title.bold().with(processor.appearance.colors.section_h2);
            QueueableCommand::queue(w, PrintStyledContent(styled))?;
            writeln!(w)?;
        }
        3 => {
            // Level 3: Triple separator + Bold (mdcat style)
            let separator = "┄┄┄".with(processor.appearance.colors.section_h3).bold();
            QueueableCommand::queue(w, PrintStyledContent(separator))?;

            let title = extract_title_text(&section.title);
            let styled = title.bold().with(processor.appearance.colors.section_h3);
            QueueableCommand::queue(w, PrintStyledContent(styled))?;
            writeln!(w)?;
        }
        4 => {
            // Level 4: Bold only (no separator)
            let styled = extract_title_text(&section.title)
                .bold()
                .with(processor.appearance.colors.section_h4);
            QueueableCommand::queue(w, PrintStyledContent(styled))?;
            writeln!(w)?;
        }
        5 => {
            // Level 5: Bold + Italic (no separator)
            let styled = extract_title_text(&section.title)
                .bold()
                .italic()
                .with(processor.appearance.colors.section_h5);
            QueueableCommand::queue(w, PrintStyledContent(styled))?;
            writeln!(w)?;
        }
        _ => {
            // Level 6+: Italic only (no separator)
            let styled = extract_title_text(&section.title)
                .italic()
                .with(processor.appearance.colors.section_h6);
            QueueableCommand::queue(w, PrintStyledContent(styled))?;
            writeln!(w)?;
        }
    }

    Ok(())
}

pub(crate) fn visit_discrete_header<V: WritableVisitor<Error = crate::Error>>(
    header: &DiscreteHeader,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), crate::Error> {
    let w = visitor.writer_mut();
    // Discrete headers render similar to level 4 sections (bold only)
    let styled = extract_title_text(&header.title)
        .bold()
        .with(processor.appearance.colors.section_h4);
    QueueableCommand::queue(w, PrintStyledContent(styled))?;
    writeln!(w)?;
    writeln!(w)?;
    Ok(())
}

/// Extract plain text from inline nodes for section titles.
///
/// This recursively extracts text content from inline nodes, handling
/// nested formatting like bold, italic, etc.
fn extract_title_text(title: &[InlineNode]) -> String {
    title
        .iter()
        .map(|node| match node {
            InlineNode::PlainText(p) => p.content.clone(),
            InlineNode::BoldText(b) => extract_title_text(&b.content),
            InlineNode::ItalicText(i) => extract_title_text(&i.content),
            InlineNode::MonospaceText(m) => extract_title_text(&m.content),
            InlineNode::HighlightText(h) => extract_title_text(&h.content),
            InlineNode::SuperscriptText(s) => extract_title_text(&s.content),
            InlineNode::SubscriptText(s) => extract_title_text(&s.content),
            InlineNode::CurvedQuotationText(c) => extract_title_text(&c.content),
            InlineNode::CurvedApostropheText(c) => extract_title_text(&c.content),
            InlineNode::VerbatimText(_) | InlineNode::RawText(_) | InlineNode::LineBreak(_) | _ => {
                String::new()
            }
        })
        .collect::<String>()
}
