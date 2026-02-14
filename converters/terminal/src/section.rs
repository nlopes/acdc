use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::{DiscreteHeader, InlineNode, Section, UNNUMBERED_SECTION_STYLES};
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

    // Skip numbering for special section styles (bibliography, glossary, etc.)
    let skip_numbering = section
        .metadata
        .style
        .as_ref()
        .is_some_and(|s| UNNUMBERED_SECTION_STYLES.contains(&s.as_str()));

    // Check for appendix
    let is_appendix = section
        .metadata
        .style
        .as_ref()
        .is_some_and(|s| s == "appendix");

    // For appendix at level 0, treat as level 1
    let effective_level = if is_appendix && section.level == 0 {
        1
    } else {
        section.level
    };

    // Build title prefix (section number, part number, or appendix label)
    let prefix = if is_appendix {
        processor
            .appendix_tracker
            .enter_appendix()
            .unwrap_or_default()
    } else if section.level == 0 && !skip_numbering {
        processor
            .part_number_tracker
            .enter_part()
            .unwrap_or_default()
    } else if !skip_numbering {
        processor
            .section_number_tracker
            .enter_section(effective_level)
            .unwrap_or_default()
    } else {
        String::new()
    };

    let raw_title = extract_title_text(&section.title);
    let title = format!("{prefix}{raw_title}");

    let tw = processor.terminal_width;

    match effective_level {
        0 | 1 => {
            // Level 0/1: Full-width rule above, bold title, rule below
            let color = processor.appearance.colors.section_h1;
            let rule = "━".repeat(tw);
            w.queue(PrintStyledContent(rule.clone().with(color)))?;
            writeln!(w)?;
            w.queue(PrintStyledContent(title.bold().with(color)))?;
            writeln!(w)?;
            w.queue(PrintStyledContent(rule.with(color)))?;
            writeln!(w)?;
        }
        2 => {
            // Level 2: Half-width rule + bold title
            let color = processor.appearance.colors.section_h2;
            let rule = "─".repeat(tw / 2);
            w.queue(PrintStyledContent(rule.with(color)))?;
            writeln!(w)?;
            w.queue(PrintStyledContent(title.bold().with(color)))?;
            writeln!(w)?;
        }
        3 => {
            // Level 3: Short rule prefix + bold title
            let color = processor.appearance.colors.section_h3;
            w.queue(PrintStyledContent("─── ".with(color)))?;
            w.queue(PrintStyledContent(title.bold().with(color)))?;
            writeln!(w)?;
        }
        4 => {
            // Level 4: Bold only (no separator)
            let styled = title.bold().with(processor.appearance.colors.section_h4);
            QueueableCommand::queue(w, PrintStyledContent(styled))?;
            writeln!(w)?;
        }
        5 => {
            // Level 5: Bold + Italic (no separator)
            let styled = title
                .bold()
                .italic()
                .with(processor.appearance.colors.section_h5);
            QueueableCommand::queue(w, PrintStyledContent(styled))?;
            writeln!(w)?;
        }
        _ => {
            // Level 6+: Italic only (no separator)
            let styled = title.italic().with(processor.appearance.colors.section_h6);
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
