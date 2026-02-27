use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::{
    DiscreteHeader, IndexTermKind, InlineMacro, InlineNode, Section, UNNUMBERED_SECTION_STYLES,
};
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
            InlineNode::VerbatimText(v) => v.content.clone(),
            InlineNode::RawText(r) => r.content.clone(),
            InlineNode::StandaloneCurvedApostrophe(_) => "\u{2019}".to_string(),
            InlineNode::LineBreak(_) => " ".to_string(),
            InlineNode::CalloutRef(c) => format!("<{}>", c.number),
            InlineNode::Macro(m) => extract_macro_text(m),
            // InlineAnchor is an invisible marker; unknown future variants fall through
            InlineNode::InlineAnchor(_) | _ => String::new(),
        })
        .collect::<String>()
}

fn extract_macro_text(m: &InlineMacro) -> String {
    match m {
        InlineMacro::Image(img) => img.source.to_string(),
        InlineMacro::Icon(icon) => icon.target.to_string(),
        InlineMacro::Keyboard(kbd) => kbd.keys.join("+"),
        InlineMacro::Button(b) => b.label.clone(),
        InlineMacro::Menu(menu) => {
            let mut parts = vec![menu.target.clone()];
            parts.extend(menu.items.iter().cloned());
            parts.join(" > ")
        }
        InlineMacro::Link(l) => l.text.clone().unwrap_or_else(|| l.target.to_string()),
        InlineMacro::Url(u) => {
            let text = extract_title_text(&u.text);
            if text.is_empty() {
                u.target.to_string()
            } else {
                text
            }
        }
        InlineMacro::Mailto(m) => {
            let text = extract_title_text(&m.text);
            if text.is_empty() {
                m.target.to_string()
            } else {
                text
            }
        }
        InlineMacro::Autolink(a) => a.url.to_string(),
        InlineMacro::CrossReference(x) => {
            let text = extract_title_text(&x.text);
            if text.is_empty() {
                x.target.clone()
            } else {
                text
            }
        }
        InlineMacro::Footnote(f) => format!("[{}]", f.number),
        InlineMacro::Pass(p) => p.text.clone().unwrap_or_default(),
        InlineMacro::Stem(s) => s.content.clone(),
        InlineMacro::IndexTerm(it) => match &it.kind {
            IndexTermKind::Flow(term) => term.clone(),
            IndexTermKind::Concealed { .. } | _ => String::new(),
        },
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::{
        Bold, CalloutRef, CalloutRefKind, Form, InlineNode, Link, Location, Plain, Source, Verbatim,
    };

    fn plain(s: &str) -> InlineNode {
        InlineNode::PlainText(Plain {
            content: s.to_string(),
            location: Location::default(),
            escaped: false,
        })
    }

    fn verbatim(s: &str) -> InlineNode {
        InlineNode::VerbatimText(Verbatim {
            content: s.to_string(),
            location: Location::default(),
        })
    }

    fn bold(nodes: Vec<InlineNode>) -> InlineNode {
        InlineNode::BoldText(Bold {
            role: None,
            id: None,
            form: Form::Constrained,
            content: nodes,
            location: Location::default(),
        })
    }

    #[test]
    fn extract_bold_wrapping_plain_text() {
        let title = [bold(vec![plain("bold title")])];
        assert_eq!(extract_title_text(&title), "bold title");
    }

    #[test]
    fn extract_verbatim_text_in_title() {
        let title = [plain("Title with "), verbatim("code"), plain(" text")];
        assert_eq!(extract_title_text(&title), "Title with code text");
    }

    #[test]
    fn extract_link_macro_with_text() {
        let link = InlineNode::Macro(InlineMacro::Link(
            Link::new(
                Source::Name("https://example.com".to_string()),
                Location::default(),
            )
            .with_text(Some("Example".to_string())),
        ));
        let title = [plain("See "), link];
        assert_eq!(extract_title_text(&title), "See Example");
    }

    #[test]
    fn extract_link_macro_without_text() {
        let link = InlineNode::Macro(InlineMacro::Link(Link::new(
            Source::Name("https://example.com".to_string()),
            Location::default(),
        )));
        let title = [link];
        assert_eq!(extract_title_text(&title), "https://example.com");
    }

    #[test]
    fn extract_mixed_content() {
        let title = [bold(vec![plain("bold")]), plain(" and "), verbatim("code")];
        assert_eq!(extract_title_text(&title), "bold and code");
    }

    #[test]
    fn extract_callout_ref() {
        let title = [
            plain("Code "),
            InlineNode::CalloutRef(CalloutRef {
                kind: CalloutRefKind::Explicit,
                number: 1,
                location: Location::default(),
            }),
        ];
        assert_eq!(extract_title_text(&title), "Code <1>");
    }

    #[test]
    fn extract_standalone_curved_apostrophe() {
        let title = [
            plain("it"),
            InlineNode::StandaloneCurvedApostrophe(acdc_parser::StandaloneCurvedApostrophe {
                location: Location::default(),
            }),
            plain("s"),
        ];
        assert_eq!(extract_title_text(&title), "it\u{2019}s");
    }
}
