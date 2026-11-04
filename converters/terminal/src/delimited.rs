use std::io::{BufWriter, IntoInnerError, Write};

use acdc_converters_core::{
    InlineTextTransform, TraversalContext,
    code::{SourceLineOptions, default_line_comment, source_language},
};
use acdc_parser::{
    CaptionKind, InlineNode, Location,
    ast::{AttributesRef, BlockKind, BlockRef, ContentRef, InlineKind, InlineRef},
};
use crossterm::{
    QueueableCommand,
    style::{
        Attribute, Color, PrintStyledContent, SetAttribute, SetBackgroundColor, SetForegroundColor,
        Stylize,
    },
};

use crate::{
    Error, Processor, TerminalVisitor, extract_heading_content,
    wrap::{pad_to_width, wrap_ansi_text},
};

struct BoxChars {
    tl: &'static str,
    tr: &'static str,
    bl: &'static str,
    br: &'static str,
    horiz: &'static str,
    vert: &'static str,
}

const ROUNDED_BOX: BoxChars = BoxChars {
    tl: "╭",
    tr: "╮",
    bl: "╰",
    br: "╯",
    horiz: "─",
    vert: "│",
};

const SQUARE_BOX: BoxChars = BoxChars {
    tl: "┌",
    tr: "┐",
    bl: "└",
    br: "┘",
    horiz: "─",
    vert: "│",
};

/// Render content inside a box with specified corner/border characters.
fn render_boxed_content<W: Write + ?Sized>(
    w: &mut W,
    label: &str,
    content: &str,
    terminal_width: usize,
    chars: &BoxChars,
    color: Color,
) -> Result<(), Error> {
    let inner_width = terminal_width.saturating_sub(4); // 2 for border + 2 for padding
    let horiz = chars.horiz;

    // Top border with label
    let label_part = if label.is_empty() {
        horiz.repeat(inner_width + 2)
    } else {
        let label_len = label.len() + 3; // "─ label "
        let remaining = (inner_width + 2).saturating_sub(label_len);
        format!("{horiz} {label} {}", horiz.repeat(remaining))
    };
    w.queue(PrintStyledContent(
        format!("{}{label_part}{}", chars.tl, chars.tr).with(color),
    ))?;
    writeln!(w)?;

    // Word-wrap content to fit inside the box, then render each line
    let wrapped = wrap_ansi_text(content, inner_width);
    for line in wrapped.lines() {
        let padded = pad_to_width(line, inner_width);
        w.queue(PrintStyledContent(format!("{} ", chars.vert).with(color)))?;
        write!(w, "{padded}")?;
        w.queue(PrintStyledContent(format!(" {}", chars.vert).with(color)))?;
        writeln!(w)?;
    }

    // Bottom border
    w.queue(PrintStyledContent(
        format!("{}{}{}", chars.bl, horiz.repeat(inner_width + 2), chars.br).with(color),
    ))?;
    writeln!(w)?;

    Ok(())
}

impl<'a, W: Write> TerminalVisitor<'a, '_, W> {
    pub(crate) fn render_delimited_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
    ) -> Result<(), Error> {
        let caption_kind = CaptionKind::for_block(block);
        match block.kind() {
            BlockKind::Table(table) => {
                self.render_captioned_title_with_wrapper(
                    traversal,
                    table.title(),
                    table.attributes(),
                    caption_kind,
                    "  ",
                    "\n",
                )?;
                let processor = self.processor;
                crate::table::visit_table(traversal, table, self, processor)
            }
            BlockKind::Listing(node) => self.render_preformatted_block(
                traversal,
                node.title(),
                node.content(),
                node.attributes(),
                caption_kind,
            ),
            BlockKind::Literal(node) => self.render_preformatted_block(
                traversal,
                node.title(),
                node.content(),
                node.attributes(),
                caption_kind,
            ),
            BlockKind::Example(node) => {
                self.render_example_block(traversal, node.title(), node.blocks(), node.attributes())
            }
            BlockKind::Quote(node) => {
                self.render_quote_block(traversal, node.title(), node.blocks(), node.attributes())
            }
            BlockKind::Sidebar(node) => {
                self.render_sidebar_block(traversal, node.title(), node.blocks())
            }
            BlockKind::Open(node) => self.render_open_block(traversal, node.title(), node.blocks()),
            BlockKind::Verse(node) => {
                self.render_verse_block(traversal, node.title(), node.content(), node.attributes())
            }
            BlockKind::Passthrough(node) => {
                self.write_content(traversal, node.content())?;
                writeln!(self.writer_mut())?;
                Ok(())
            }
            BlockKind::Stem(node) => self.render_stem_block(
                traversal,
                node.title(),
                &node.notation().to_string(),
                node.text(),
            ),
            BlockKind::DelimitedComment(_) => Ok(()),
            BlockKind::Section(_)
            | BlockKind::Paragraph(_)
            | BlockKind::OrderedList(_)
            | BlockKind::UnorderedList(_)
            | BlockKind::DescriptionList(_)
            | BlockKind::CalloutList(_)
            | BlockKind::Admonition(_)
            | BlockKind::Image(_)
            | BlockKind::Audio(_)
            | BlockKind::Video(_)
            | BlockKind::DiscreteHeader(_)
            | BlockKind::TableOfContents(_)
            | BlockKind::DocumentAttribute(_)
            | BlockKind::Comment(_)
            | BlockKind::ThematicBreak(_)
            | BlockKind::PageBreak(_)
            | _ => {
                self.warn_unsupported_parser_variant("delimited block");
                Ok(())
            }
        }
    }

    /// Render a listing, literal, or source block with its terminal source options.
    fn render_preformatted_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        inlines: ContentRef<'_>,
        attributes: AttributesRef<'_>,
        caption_kind: Option<CaptionKind>,
    ) -> Result<(), Error> {
        // Detect language for syntax highlighting
        let language = source_language(attributes);

        if matches!(
            attributes.get("style"),
            Some(acdc_parser::ast::AttributeValueRef::Text("source"))
        ) && language == Some("php")
            && super::terminal_visitor::has_token(attributes, "opts", "mixed")
            && self.processor.mark_fallback("php-mixed-highlighting")
        {
            self.diagnostics.warn_with_advice(
                "PHP source block mixed-mode highlighting is not supported by the terminal backend; rendering with the normal PHP highlighter",
                "Use the `html+php` source language when it gives acceptable highlighting, or use Asciidoctor HTML for explicit `%mixed` highlighting.",
            );
        }

        self.render_captioned_title_with_wrapper(
            traversal,
            title,
            attributes,
            caption_kind,
            "\n",
            "\n",
        )?;

        let processor = self.processor;
        let tw = processor.terminal_width;
        let color = processor.appearance.colors.label_listing;

        // Top separator with optional language label
        let top_sep = if let Some(lang) = language {
            let label = format!("[ {lang} ]");
            let half = tw.saturating_sub(label.len()) / 2;
            format!(
                "{}{label}{}",
                "─".repeat(half),
                "─".repeat(tw.saturating_sub(half + label.len()))
            )
        } else {
            "─".repeat(tw)
        };
        let w = self.writer_mut();
        writeln!(w, "{}", top_sep.clone().with(color))?;

        acdc_converters_core::index::visit_index_inlines(inlines.inlines(), &mut |term, _| {
            crate::inlines::register_index_term(term, processor, traversal)
        })?;
        // Render code content directly (no left border)
        let content = render_preformatted_content(inlines, attributes, processor)?;
        let w = self.writer_mut();
        write!(w, "{content}")?;
        if !content.ends_with('\n') {
            writeln!(w)?;
        }

        // Bottom separator
        writeln!(w, "{}", "─".repeat(tw).with(color))?;

        Ok(())
    }

    /// Render an example block with box borders.
    fn render_example_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        blocks: impl Iterator<Item = BlockRef<'a>>,
        attributes: AttributesRef<'_>,
    ) -> Result<(), Error> {
        let processor = self.processor;
        let label = if title.is_some() {
            let caption = processor
                .caption_prefix(attributes, Some(CaptionKind::Example))
                .unwrap_or_default();
            let title_text = extract_heading_content(title, &processor.references);
            format!("{caption}{title_text}")
        } else {
            String::new()
        };

        // Render content to buffer
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());
        for nested_block in blocks {
            traversal.visit_block(&mut temp_visitor, nested_block)?;
        }
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(IntoInnerError::into_error)?;
        let content = String::from_utf8_lossy(&buffer);

        let color = Color::Cyan;
        render_boxed_content(
            self.writer_mut(),
            &label,
            content.trim_end(),
            processor.terminal_width,
            &SQUARE_BOX,
            color,
        )?;

        Ok(())
    }

    /// Render a quote block with `│` left border.
    fn render_quote_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        blocks: impl Iterator<Item = BlockRef<'a>>,
        attributes: AttributesRef<'_>,
    ) -> Result<(), Error> {
        let processor = self.processor;

        // Render title if present
        self.render_title_with_wrapper(traversal, title, "", "\n")?;

        // Render content to temporary buffer
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());

        for nested_block in blocks {
            traversal.visit_block(&mut temp_visitor, nested_block)?;
        }

        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(IntoInnerError::into_error)?;

        let content = String::from_utf8_lossy(&buffer);
        let color = processor.appearance.colors.admon_tip; // Green for quotes

        // Word-wrap content to fit within the "│ " prefix
        let available = processor.terminal_width.saturating_sub(2);
        let wrapped = wrap_ansi_text(&content, available);

        // Left border with `│` on each line, content in italic
        let w = self.writer_mut();
        for line in wrapped.lines() {
            w.queue(PrintStyledContent("│ ".with(color)))?;
            let styled_line = line.italic();
            QueueableCommand::queue(w, PrintStyledContent(styled_line))?;
            writeln!(w)?;
        }

        // Empty closing border line
        if !content.is_empty() {
            w.queue(PrintStyledContent("│".with(color)))?;
            writeln!(w)?;
        }

        self.render_attribution(attributes)?;

        Ok(())
    }

    fn render_open_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        blocks: impl Iterator<Item = BlockRef<'a>>,
    ) -> Result<(), Error> {
        let processor = self.processor;
        let label = extract_heading_content(title, &processor.references);
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut visitor = TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());
        for block in blocks {
            traversal.visit_block(&mut visitor, block)?;
        }
        let buffer = visitor
            .into_writer()
            .into_inner()
            .map_err(IntoInnerError::into_error)?;
        let content = String::from_utf8_lossy(&buffer);
        render_boxed_content(
            self.writer_mut(),
            &label,
            content.trim_end(),
            processor.terminal_width,
            &ROUNDED_BOX,
            Color::DarkYellow,
        )
    }

    /// Render a sidebar block with rounded box borders.
    fn render_sidebar_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        blocks: impl Iterator<Item = BlockRef<'a>>,
    ) -> Result<(), Error> {
        self.render_sidebar_content(traversal, title, |visitor, traversal| {
            traversal.visit_blocks(visitor, blocks)
        })
    }

    pub(crate) fn render_sidebar_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        body: impl FnOnce(
            &mut TerminalVisitor<'a, '_, BufWriter<Vec<u8>>>,
            &mut TraversalContext<'a>,
        ) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let processor = self.processor;

        let label = if title.is_some() {
            extract_heading_content(title, &processor.references)
        } else {
            String::new()
        };

        // Render content to buffer
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());
        body(&mut temp_visitor, traversal)?;
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(IntoInnerError::into_error)?;
        let content = String::from_utf8_lossy(&buffer);

        let color = Color::Blue;
        render_boxed_content(
            self.writer_mut(),
            &label,
            content.trim_end(),
            processor.terminal_width,
            &ROUNDED_BOX,
            color,
        )?;

        Ok(())
    }

    /// Render a verse block (poetry) with `┊` left border preserving line breaks.
    fn render_verse_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        inlines: ContentRef<'_>,
        attributes: AttributesRef<'_>,
    ) -> Result<(), Error> {
        let processor = self.processor;

        self.render_title_with_wrapper(traversal, title, "", "\n")?;

        // Render verse content to buffer to process line by line
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());
        temp_visitor.write_content(traversal, inlines)?;
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(IntoInnerError::into_error)?;

        let content = String::from_utf8_lossy(&buffer);
        let color = Color::Magenta;

        let w = self.writer_mut();
        for line in content.lines() {
            w.queue(PrintStyledContent("┊ ".with(color)))?;
            write!(w, "{line}")?;
            writeln!(w)?;
        }
        // Closing border
        w.queue(PrintStyledContent("┊".with(color)))?;
        writeln!(w)?;

        self.render_attribution(attributes)?;

        Ok(())
    }

    /// Render a STEM/math block with styled borders.
    fn render_stem_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        notation: &str,
        content: &str,
    ) -> Result<(), Error> {
        self.render_title_if_present(traversal, title)?;

        let processor = self.processor;
        let tw = processor.terminal_width;
        let color = processor.appearance.colors.label_listing;

        // Top separator with notation label
        let label = format!(" {notation} ");
        let half = tw.saturating_sub(label.len()) / 2;
        let top = format!(
            "{}{}{}",
            "─".repeat(half),
            label,
            "─".repeat(tw.saturating_sub(half + label.len()))
        );

        let w = self.writer_mut();
        writeln!(w, "{}", top.with(color))?;
        writeln!(w, "{content}")?;
        writeln!(w, "{}", "─".repeat(tw).with(color))?;
        Ok(())
    }

    /// Render an authored block title, including an explicitly empty title.
    fn render_title_if_present(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
    ) -> Result<(), Error> {
        self.render_title_with_wrapper(traversal, title, "  ", "\n")
    }
}

pub(crate) fn render_preformatted_content(
    inlines: ContentRef<'_>,
    attributes: AttributesRef<'_>,
    processor: &Processor<'_>,
) -> Result<String, Error> {
    render_preformatted_inlines(
        inlines.inlines(),
        source_language(attributes),
        processor,
        |source| SourceLineOptions::from_attributes(attributes, source),
    )
}

pub(crate) fn render_preformatted_inlines<'inline>(
    inlines: impl Iterator<Item = InlineRef<'inline>>,
    language: Option<&str>,
    processor: &Processor<'_>,
    line_options: impl FnOnce(&str) -> SourceLineOptions,
) -> Result<String, Error> {
    let inlines = inlines.collect::<Vec<_>>();
    let (source, callouts, formatted) = source_with_callout_placeholders(&inlines, language);
    let mut output = Vec::new();
    // Explicit quote formatting takes precedence over syntax colors. Sending
    // its terminal escapes through the highlighter would display or corrupt them.
    if let Some(language) = language.filter(|_| !formatted) {
        crate::syntax::highlight_text(&mut output, &source, language, processor)?;
    } else {
        output.extend_from_slice(source.as_bytes());
    }
    let mut output = String::from_utf8(output).unwrap_or_default();
    for (placeholder, number) in callouts {
        let marker = format!(
            "{}{}<{number}>{}{}",
            SetForegroundColor(Color::Yellow),
            SetAttribute(Attribute::Bold),
            SetAttribute(Attribute::NormalIntensity),
            SetForegroundColor(Color::Reset),
        );
        output = output.replace(&placeholder, &marker);
    }
    let options = line_options(&source);
    Ok(apply_source_line_options(&output, &options))
}

pub(crate) fn code_text_transform() -> InlineTextTransform<'static, 'static> {
    InlineTextTransform::default()
        .line_break("\n")
        .rendered_replacements(false)
}

fn source_with_callout_placeholders(
    nodes: &[InlineRef<'_>],
    language: Option<&str>,
) -> (String, Vec<(String, usize)>, bool) {
    let mut source = String::new();
    let mut callouts = Vec::new();
    let mut formatted = false;
    let transform = code_text_transform();
    let comment_prefix = default_line_comment(language);

    for (index, node) in nodes.iter().copied().enumerate() {
        if let InlineKind::Verbatim(verbatim) = node.kind() {
            let mut content = verbatim.text().to_string();
            if index
                .checked_sub(1)
                .is_some_and(|previous| is_xml_callout(nodes, previous))
            {
                content = content.strip_prefix("-->").unwrap_or(&content).to_string();
            }
            if index.checked_add(1).is_some_and(|next| {
                matches!(
                    nodes.get(next).map(|node| node.kind()),
                    Some(InlineKind::Callout(_))
                )
            }) {
                if index
                    .checked_add(1)
                    .is_some_and(|next| is_xml_callout(nodes, next))
                {
                    content = content.strip_suffix("<!--").unwrap_or(&content).to_string();
                } else {
                    content = strip_callout_guard(&content, comment_prefix);
                }
            }
            let text = InlineNode::VerbatimText(acdc_parser::Verbatim {
                content: &content,
                location: Location::from(verbatim.location()),
            });
            let _ = transform.write_inlines(&mut source, std::iter::once((&text).into()));
        } else if let InlineKind::Callout(callout) = node.kind() {
            let offset = u32::try_from(callouts.len()).unwrap_or(0xFFFD).min(0xFFFD);
            let placeholder = char::from_u32(0xF0000 + offset)
                .unwrap_or('\u{F0000}')
                .to_string();
            source.push_str(&placeholder);
            callouts.push((placeholder, callout.number()));
        } else if let Some(text) = formatted_code_text(node) {
            source.push_str(&text);
            formatted = true;
        } else {
            let _ = transform.write_inlines(&mut source, std::iter::once(node));
        }
    }
    (source, callouts, formatted)
}

fn formatted_code_text(node: InlineRef<'_>) -> Option<String> {
    let children = acdc_converters_core::code::code_inline_content(node)?;
    let transform = code_text_transform();
    let text = children
        .inlines()
        .map(|child| {
            formatted_code_text(child).unwrap_or_else(|| {
                let mut text = String::new();
                let _ = transform.write_inlines(&mut text, std::iter::once(child));
                text
            })
        })
        .collect::<String>();
    Some(match node.kind() {
        InlineKind::Bold(_) => wrap_code_style(
            &text,
            SetAttribute(Attribute::Bold),
            SetAttribute(Attribute::NormalIntensity),
        ),
        InlineKind::Italic(_) => wrap_code_style(
            &text,
            SetAttribute(Attribute::Italic),
            SetAttribute(Attribute::NoItalic),
        ),
        InlineKind::Highlight(highlight) => match highlight.role() {
            Some("underline") => wrap_code_style(
                &text,
                SetAttribute(Attribute::Underlined),
                SetAttribute(Attribute::NoUnderline),
            ),
            Some("line-through") => wrap_code_style(
                &text,
                SetAttribute(Attribute::CrossedOut),
                SetAttribute(Attribute::NotCrossedOut),
            ),
            _ => wrap_code_style(
                &text,
                SetBackgroundColor(Color::Yellow),
                SetBackgroundColor(Color::Reset),
            ),
        },
        InlineKind::Monospace(_) | InlineKind::Superscript(_) | InlineKind::Subscript(_) => text,
        InlineKind::CurvedQuotation(_) => format!("“{text}”"),
        InlineKind::CurvedApostrophe(_) => format!("‘{text}’"),
        InlineKind::Plain(_)
        | InlineKind::Raw(_)
        | InlineKind::Verbatim(_)
        | InlineKind::StandaloneCurvedApostrophe(_)
        | InlineKind::Symbol(_)
        | InlineKind::LineBreak(_)
        | InlineKind::Anchor(_)
        | InlineKind::Footnote(_)
        | InlineKind::Icon(_)
        | InlineKind::Image(_)
        | InlineKind::Keyboard(_)
        | InlineKind::Button(_)
        | InlineKind::Menu(_)
        | InlineKind::Url(_)
        | InlineKind::Link(_)
        | InlineKind::Mailto(_)
        | InlineKind::Autolink(_)
        | InlineKind::CrossReference(_)
        | InlineKind::Pass(_)
        | InlineKind::Stem(_)
        | InlineKind::IndexTerm(_)
        | InlineKind::Callout(_)
        | _ => return None,
    })
}

fn wrap_code_style(
    text: &str,
    start: impl std::fmt::Display,
    end: impl std::fmt::Display,
) -> String {
    // Line-number decoration resets intensity, so reopen formatting on each line.
    text.split('\n')
        .map(|line| format!("{start}{line}{end}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_xml_callout(nodes: &[InlineRef<'_>], index: usize) -> bool {
    matches!(
        nodes.get(index).map(|node| node.kind()),
        Some(InlineKind::Callout(_))
    ) && index.checked_sub(1).is_some_and(|previous| {
        matches!(
            nodes.get(previous).map(|node| node.kind()),
            Some(InlineKind::Verbatim(text)) if text.text().ends_with("<!--")
        )
    }) && index.checked_add(1).is_some_and(|next| {
        matches!(
            nodes.get(next).map(|node| node.kind()),
            Some(InlineKind::Verbatim(text)) if text.text().starts_with("-->")
        )
    })
}

fn strip_callout_guard(text: &str, language_prefix: Option<&str>) -> String {
    let trimmed = text.trim_end();
    for prefix in [
        language_prefix,
        Some("//"),
        Some("#"),
        Some("--"),
        Some(";;"),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(content) = trimmed.strip_suffix(prefix) {
            return format!("{} ", content.trim_end());
        }
    }
    text.to_string()
}

fn apply_source_line_options(content: &str, options: &SourceLineOptions) -> String {
    if options.is_empty() {
        return content.to_string();
    }
    let line_count = content.trim_end_matches('\n').split('\n').count();
    let number_width = options.line_number_start.map_or(0, |start| {
        start
            .saturating_add(line_count.saturating_sub(1))
            .to_string()
            .len()
    });
    let mut output = String::new();
    for (index, line) in content.trim_end_matches('\n').split('\n').enumerate() {
        if let Some(start) = options.line_number_start {
            use std::fmt::Write as _;
            let number = start.saturating_add(index);
            let _ = write!(
                output,
                "{}{:>number_width$} │ {}",
                SetAttribute(Attribute::Dim),
                number,
                SetAttribute(Attribute::NormalIntensity),
            );
        }
        if options.highlighted_lines.contains(&(index + 1)) {
            use std::fmt::Write as _;
            let _ = write!(
                output,
                "{}{}{}",
                SetBackgroundColor(Color::Rgb {
                    r: 64,
                    g: 64,
                    b: 64,
                }),
                line,
                SetBackgroundColor(Color::Reset),
            );
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }
    output
}

/// Extract plain text from inline nodes (for labels/titles).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TerminalVisitor, create_test_processor};
    use acdc_parser::{Block, DelimitedBlock, DelimitedBlockType};

    use acdc_converters_core::{Converter, Diagnostics, WarningSource};
    use acdc_parser::{Location, Paragraph, Plain, Title};

    /// Create simple plain text inline nodes for testing
    fn create_test_inlines(content: &str) -> Vec<InlineNode<'_>> {
        vec![InlineNode::PlainText(Plain {
            content,
            location: Location::default(),
            escaped: false,
        })]
    }

    /// Create simple plain text title for testing
    fn create_test_title(content: &str) -> Title<'_> {
        Title::new(vec![InlineNode::PlainText(Plain {
            content,
            location: Location::default(),
            escaped: false,
        })])
    }

    #[test]
    fn test_listing_block_basic() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code content here")),
            "----",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("────"),
            "Should have horizontal separators"
        );
        assert!(
            output_str.contains("code content here"),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_listing_block_with_title() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            "----",
            Location::default(),
        )
        .with_title(create_test_title("My Code Listing"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("My Code Listing"),
            "Should contain title"
        );
        assert!(output_str.contains("code here"), "Should contain content");
        assert!(
            output_str.contains("────"),
            "Should have horizontal separators"
        );

        Ok(())
    }

    #[test]
    fn test_literal_block_basic() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedLiteral(create_test_inlines("literal text")),
            "....",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("────"),
            "Should have horizontal separators"
        );
        assert!(
            output_str.contains("literal text"),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_literal_block_with_title() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedLiteral(create_test_inlines("literal content")),
            "....",
            Location::default(),
        )
        .with_title(create_test_title("Literal Block Title"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Literal Block Title"),
            "Should contain title"
        );
        assert!(
            output_str.contains("literal content"),
            "Should contain content"
        );
        assert!(
            output_str.contains("────"),
            "Should have horizontal separators"
        );

        Ok(())
    }

    #[test]
    fn test_example_block_basic() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("example text"),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(content),
            "====",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            !output_str.contains("Example"),
            "Untitled examples should not have a label"
        );
        assert!(output_str.contains("┌"), "Should have box top border");
        assert!(output_str.contains("└"), "Should have box bottom border");
        assert!(
            output_str.contains("example text"),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_example_block_with_title() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("example content"),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(content),
            "====",
            Location::default(),
        )
        .with_title(create_test_title("Custom Example Title"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Example"), "Should have Example label");
        assert!(
            output_str.contains("Custom Example Title"),
            "Should contain custom title"
        );
        assert!(
            output_str.contains("example content"),
            "Should contain content"
        );
        assert!(output_str.contains("┌"), "Should have box border");

        Ok(())
    }

    #[test]
    fn test_quote_block_basic() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("This is a quote."),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedQuote(content),
            "____",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("This is a quote."),
            "Should contain content"
        );
        assert!(output_str.contains("│"), "Should have left border");

        Ok(())
    }

    #[test]
    fn test_quote_block_with_title() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("Quote content here."),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedQuote(content),
            "____",
            Location::default(),
        )
        .with_title(create_test_title("Quote Title"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Quote Title"), "Should contain title");
        assert!(
            output_str.contains("Quote content here."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_quote_block_multiple_paragraphs() -> Result<(), Error> {
        let content = vec![
            Block::Paragraph(Paragraph::new(
                create_test_inlines("First paragraph."),
                Location::default(),
            )),
            Block::Paragraph(Paragraph::new(
                create_test_inlines("Second paragraph."),
                Location::default(),
            )),
        ];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedQuote(content),
            "____",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("First paragraph."),
            "Should contain first paragraph"
        );
        assert!(
            output_str.contains("Second paragraph."),
            "Should contain second paragraph"
        );

        Ok(())
    }

    #[test]
    fn test_sidebar_block_basic() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("Sidebar content."),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedSidebar(content),
            "****",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("╭"), "Should have rounded top border");
        assert!(
            output_str.contains("╰"),
            "Should have rounded bottom border"
        );
        assert!(
            output_str.contains("Sidebar content."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_sidebar_block_with_title() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("Sidebar text here."),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedSidebar(content),
            "****",
            Location::default(),
        )
        .with_title(create_test_title("Sidebar Title"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("╭"), "Should have rounded border");
        assert!(output_str.contains("Sidebar Title"), "Should contain title");
        assert!(
            output_str.contains("Sidebar text here."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_sidebar_block_multiple_paragraphs() -> Result<(), Error> {
        let content = vec![
            Block::Paragraph(Paragraph::new(
                create_test_inlines("First sidebar paragraph."),
                Location::default(),
            )),
            Block::Paragraph(Paragraph::new(
                create_test_inlines("Second sidebar paragraph."),
                Location::default(),
            )),
        ];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedSidebar(content),
            "****",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("First sidebar paragraph."),
            "Should contain first paragraph"
        );
        assert!(
            output_str.contains("Second sidebar paragraph."),
            "Should contain second paragraph"
        );

        Ok(())
    }

    #[test]
    fn test_open_block_basic() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("Open block content."),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedOpen(content),
            "--",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Open block content."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_open_block_with_title() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("Content here."),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedOpen(content),
            "--",
            Location::default(),
        )
        .with_title(create_test_title("Open Block Title"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Open Block Title"),
            "Should contain title"
        );
        assert!(
            output_str.contains("Content here."),
            "Should contain content"
        );

        Ok(())
    }

    #[test]
    fn test_verse_block_basic() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedVerse(create_test_inlines(
                "Roses are red\nViolets are blue",
            )),
            "____",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Roses are red"),
            "Should contain verse content"
        );
        assert!(output_str.contains("┊"), "Should have dotted left border");

        Ok(())
    }

    #[test]
    fn test_verse_block_with_title() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedVerse(create_test_inlines("Poetry line 1\nPoetry line 2")),
            "____",
            Location::default(),
        )
        .with_title(create_test_title("Poem Title"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Poem Title"), "Should contain title");
        assert!(
            output_str.contains("Poetry line 1"),
            "Should contain verse content"
        );

        Ok(())
    }

    #[test]
    fn test_pass_block_basic() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedPass(create_test_inlines("<raw>passthrough</raw>")),
            "++++",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("<raw>passthrough</raw>"),
            "Should contain passthrough content as-is"
        );

        Ok(())
    }

    #[test]
    fn test_stem_block_placeholder() -> Result<(), Error> {
        use acdc_parser::{StemContent, StemNotation};

        let stem_content = StemContent::new("x = y^2", StemNotation::Latexmath);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedStem(stem_content),
            "++++",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("latexmath"),
            "Should show notation type"
        );
        assert!(output_str.contains("x = y^2"), "Should show STEM content");
        assert!(output_str.contains("───"), "Should have styled borders");

        Ok(())
    }

    #[test]
    fn test_comment_block_not_rendered() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedComment(create_test_inlines("This is a comment")),
            "////",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            !output_str.contains("This is a comment"),
            "Comment content should not be rendered"
        );

        Ok(())
    }

    #[test]
    fn test_empty_listing_block() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(Vec::new()),
            "----",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("────"),
            "Should have horizontal separators even when empty"
        );

        Ok(())
    }

    #[test]
    fn test_empty_quote_block() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedQuote(Vec::new()),
            "____",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        // Empty quote should produce empty or whitespace output
        assert!(
            output_str.is_empty() || output_str.trim().is_empty(),
            "Empty quote block should produce empty or whitespace output"
        );

        Ok(())
    }

    #[test]
    fn test_listing_with_special_characters() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines(
                "<html>&amp; special chars \"quotes\" 'apostrophes'",
            )),
            "----",
            Location::default(),
        );

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("<html>&amp; special chars"),
            "Should preserve special characters"
        );

        Ok(())
    }

    #[test]
    fn test_nested_example_with_listing() -> Result<(), Error> {
        let content = vec![Block::Paragraph(Paragraph::new(
            create_test_inlines("This example shows: code snippet"),
            Location::default(),
        ))];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(content),
            "====",
            Location::default(),
        )
        .with_title(create_test_title("Nested Content"));

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_delimited_block(&mut traversal, (&block).into())?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Nested Content"),
            "Should contain title"
        );
        assert!(
            output_str.contains("This example shows: code snippet"),
            "Should contain nested content"
        );

        Ok(())
    }

    #[test]
    fn test_example_block_numbering_sequence() -> Result<(), Error> {
        let processor = create_test_processor();

        let block1 = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![Block::Paragraph(Paragraph::new(
                create_test_inlines("first example"),
                Location::default(),
            ))]),
            "====",
            Location::default(),
        )
        .with_title(create_test_title("First Example"));

        let block2 = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![Block::Paragraph(Paragraph::new(
                create_test_inlines("second example"),
                Location::default(),
            ))]),
            "====",
            Location::default(),
        )
        .with_title(create_test_title("Second Example"));

        let block3 = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![Block::Paragraph(Paragraph::new(
                create_test_inlines("third example"),
                Location::default(),
            ))]),
            "====",
            Location::default(),
        )
        .with_title(create_test_title("Third Example"));

        let mut buffer1 = Vec::new();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor1 = TerminalVisitor::new(&mut buffer1, &processor, diagnostics.reborrow());
        visitor1.render_delimited_block(&mut traversal, (&block1).into())?;

        let mut buffer2 = Vec::new();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor2 = TerminalVisitor::new(&mut buffer2, &processor, diagnostics.reborrow());
        visitor2.render_delimited_block(&mut traversal, (&block2).into())?;

        let mut buffer3 = Vec::new();
        let mut warnings = Vec::new();
        let source = WarningSource::new("terminal");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor3 = TerminalVisitor::new(&mut buffer3, &processor, diagnostics.reborrow());
        visitor3.render_delimited_block(&mut traversal, (&block3).into())?;

        let output1 = String::from_utf8_lossy(&buffer1);
        let output2 = String::from_utf8_lossy(&buffer2);
        let output3 = String::from_utf8_lossy(&buffer3);

        assert!(
            output1.contains("Example 1."),
            "First example should have number 1, got: {output1}"
        );
        assert!(
            output2.contains("Example 2."),
            "Second example should have number 2, got: {output2}"
        );
        assert!(
            output3.contains("Example 3."),
            "Third example should have number 3, got: {output3}"
        );

        Ok(())
    }
}
