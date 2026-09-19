#[cfg(feature = "pre-spec-subs")]
use std::borrow::Cow;
use std::io::{BufWriter, Write};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::{
    Replacements, SubsFlags, TextBoundaries, apply_replacements, effective_subs_flags,
};
use acdc_converters_core::{
    InlineTextTransform,
    code::{SourceLineOptions, default_line_comment, detect_language},
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{
    Block, BlockMetadata, CaptionKind, DelimitedBlock, DelimitedBlockType, InlineNode,
};
use crossterm::{
    QueueableCommand,
    style::{
        Attribute, Color, PrintStyledContent, SetAttribute, SetBackgroundColor, SetForegroundColor,
        Stylize,
    },
};

use crate::wrap::{pad_to_width, wrap_ansi_text};
use crate::{Error, TerminalVisitor};

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
fn render_boxed_content<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    label: &str,
    content: &str,
    terminal_width: usize,
    chars: &BoxChars,
    color: crossterm::style::Color,
) -> Result<(), Error> {
    let inner_width = terminal_width.saturating_sub(4); // 2 for border + 2 for padding
    let horiz = chars.horiz;

    // Top border with label
    let w = visitor.writer_mut();
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

impl<W: Write> TerminalVisitor<'_, '_, W> {
    /// Visit a delimited block in terminal format.
    pub(crate) fn render_delimited_block(&mut self, block: &DelimitedBlock) -> Result<(), Error> {
        #[cfg(feature = "pre-spec-subs")]
        let previous_subs = self.processor.current_subs.replace(effective_subs_flags(
            block.metadata.substitutions.as_ref(),
            matches!(
                block.inner,
                DelimitedBlockType::DelimitedListing(_)
                    | DelimitedBlockType::DelimitedLiteral(_)
                    | DelimitedBlockType::DelimitedPass(_)
                    | DelimitedBlockType::DelimitedVerse(_)
            ),
        ));

        let caption_kind = CaptionKind::for_delimited(&block.inner, block.metadata.style);
        let result = match &block.inner {
            DelimitedBlockType::DelimitedTable(t) => {
                self.render_captioned_title_with_wrapper(
                    &block.title,
                    &block.metadata,
                    caption_kind,
                    "  ",
                    "\n",
                )?;
                let processor = self.processor.clone();
                crate::table::visit_table(t, &block.metadata, self, &processor)
            }
            DelimitedBlockType::DelimitedListing(inlines)
            | DelimitedBlockType::DelimitedLiteral(inlines) => {
                self.render_preformatted_block(&block.title, inlines, &block.metadata, caption_kind)
            }
            DelimitedBlockType::DelimitedExample(blocks) => {
                self.render_example_block(&block.title, blocks, &block.metadata)
            }
            DelimitedBlockType::DelimitedQuote(blocks) => {
                self.render_quote_block(&block.title, blocks, &block.metadata)
            }
            DelimitedBlockType::DelimitedSidebar(blocks) => {
                self.render_sidebar_block(&block.title, blocks)
            }
            DelimitedBlockType::DelimitedOpen(blocks) => {
                self.render_open_block(&block.title, blocks)
            }
            DelimitedBlockType::DelimitedVerse(inlines) => {
                self.render_verse_block(&block.title, inlines, &block.metadata)
            }
            DelimitedBlockType::DelimitedPass(inlines) => {
                // Passthrough content is rendered as-is
                let inlines = inlines.clone();
                self.visit_inline_nodes(&inlines)?;
                let w = self.writer_mut();
                writeln!(w)?;
                Ok(())
            }
            DelimitedBlockType::DelimitedStem(stem) => {
                let notation = stem.notation.to_string();
                self.render_stem_block(&block.title, &notation, stem.content)
            }
            DelimitedBlockType::DelimitedComment(_) => {
                // Comments are not rendered
                Ok(())
            }
            _ => {
                self.warn_unsupported_parser_variant("delimited block");
                Ok(())
            }
        };

        #[cfg(feature = "pre-spec-subs")]
        self.processor.current_subs.set(previous_subs);
        result
    }

    /// Render a listing, literal, or source block with its terminal source options.
    fn render_preformatted_block(
        &mut self,
        title: &[InlineNode],
        inlines: &[InlineNode],
        metadata: &BlockMetadata,
        caption_kind: Option<CaptionKind>,
    ) -> Result<(), Error> {
        // Detect language for syntax highlighting
        let language = detect_language(metadata);

        if metadata.style == Some("source")
            && language == Some("php")
            && metadata.options.contains(&"mixed")
            && self.processor.mark_fallback("php-mixed-highlighting")
        {
            self.diagnostics.warn_with_advice(
                "PHP source block mixed-mode highlighting is not supported by the terminal backend; rendering with the normal PHP highlighter",
                "Use the `html+php` source language when it gives acceptable highlighting, or use Asciidoctor HTML for explicit `%mixed` highlighting.",
            );
        }

        self.render_captioned_title_with_wrapper(title, metadata, caption_kind, "\n", "\n")?;

        let processor = self.processor.clone();
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

        // Render code content directly (no left border)
        let content = render_preformatted_content(inlines, metadata, &processor)?;
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
        title: &[InlineNode],
        blocks: &[Block],
        metadata: &BlockMetadata,
    ) -> Result<(), Error> {
        let processor = self.processor.clone();
        let label = if title.is_empty() {
            String::new()
        } else {
            let caption = processor
                .caption_prefix(metadata, Some(CaptionKind::Example))
                .unwrap_or_default();
            let title_text = crate::extract_heading_text(title, &processor.references);
            format!("{caption}{title_text}")
        };

        // Render content to buffer
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor =
            TerminalVisitor::new(inner, processor.clone(), self.diagnostics.reborrow());
        for nested_block in blocks {
            temp_visitor.visit_block(nested_block)?;
        }
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(std::io::IntoInnerError::into_error)?;
        let content = String::from_utf8_lossy(&buffer);

        let color = crossterm::style::Color::Cyan;
        render_boxed_content(
            self,
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
        title: &[InlineNode],
        blocks: &[Block],
        metadata: &BlockMetadata<'_>,
    ) -> Result<(), Error> {
        let processor = self.processor.clone();

        // Render title if present
        self.render_title_with_wrapper(title, "", "\n")?;

        // Render content to temporary buffer
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor =
            TerminalVisitor::new(inner, processor.clone(), self.diagnostics.reborrow());

        for nested_block in blocks {
            temp_visitor.visit_block(nested_block)?;
        }

        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(std::io::IntoInnerError::into_error)?;

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

        self.render_attribution(metadata)?;

        Ok(())
    }

    fn render_open_block(&mut self, title: &[InlineNode], blocks: &[Block]) -> Result<(), Error> {
        let processor = self.processor.clone();
        let label = crate::extract_heading_text(title, &processor.references);
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut visitor =
            TerminalVisitor::new(inner, processor.clone(), self.diagnostics.reborrow());
        for block in blocks {
            visitor.visit_block(block)?;
        }
        let buffer = visitor
            .into_writer()
            .into_inner()
            .map_err(std::io::IntoInnerError::into_error)?;
        let content = String::from_utf8_lossy(&buffer);
        render_boxed_content(
            self,
            &label,
            content.trim_end(),
            processor.terminal_width,
            &ROUNDED_BOX,
            crossterm::style::Color::DarkYellow,
        )
    }

    /// Render a sidebar block with rounded box borders.
    fn render_sidebar_block(
        &mut self,
        title: &[InlineNode],
        blocks: &[Block],
    ) -> Result<(), Error> {
        let processor = self.processor.clone();

        let label = if title.is_empty() {
            String::new()
        } else {
            crate::extract_heading_text(title, &processor.references)
        };

        // Render content to buffer
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor =
            TerminalVisitor::new(inner, processor.clone(), self.diagnostics.reborrow());
        for nested_block in blocks {
            temp_visitor.visit_block(nested_block)?;
        }
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(std::io::IntoInnerError::into_error)?;
        let content = String::from_utf8_lossy(&buffer);

        let color = crossterm::style::Color::Blue;
        render_boxed_content(
            self,
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
        title: &[InlineNode],
        inlines: &[InlineNode],
        metadata: &BlockMetadata<'_>,
    ) -> Result<(), Error> {
        let processor = self.processor.clone();

        self.render_title_with_wrapper(title, "", "\n")?;

        // Render verse content to buffer to process line by line
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor =
            TerminalVisitor::new(inner, processor.clone(), self.diagnostics.reborrow());
        temp_visitor.visit_inline_nodes(inlines)?;
        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(std::io::IntoInnerError::into_error)?;

        let content = String::from_utf8_lossy(&buffer);
        let color = crossterm::style::Color::Magenta;

        let w = self.writer_mut();
        for line in content.lines() {
            w.queue(PrintStyledContent("┊ ".with(color)))?;
            write!(w, "{line}")?;
            writeln!(w)?;
        }
        // Closing border
        w.queue(PrintStyledContent("┊".with(color)))?;
        writeln!(w)?;

        self.render_attribution(metadata)?;

        Ok(())
    }

    /// Render a STEM/math block with styled borders.
    fn render_stem_block(
        &mut self,
        title: &[InlineNode],
        notation: &str,
        content: &str,
    ) -> Result<(), Error> {
        self.render_title_if_present(title)?;

        let processor = self.processor.clone();
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

    /// Helper to render title if present.
    fn render_title_if_present(&mut self, title: &[InlineNode]) -> Result<(), Error> {
        self.render_title_with_wrapper(title, "  ", "\n")
    }
}

pub(crate) fn render_preformatted_content(
    inlines: &[InlineNode<'_>],
    metadata: &BlockMetadata<'_>,
    processor: &crate::Processor<'_>,
) -> Result<String, Error> {
    let language = detect_language(metadata);
    let (source, callouts) = source_with_callout_placeholders(inlines, language);
    let source = apply_source_substitutions(source, processor);
    let mut output = Vec::new();
    if let Some(language) = language {
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
    let options = SourceLineOptions::resolve(metadata, &source);
    Ok(apply_source_line_options(&output, &options))
}

#[cfg(feature = "pre-spec-subs")]
fn apply_source_substitutions(mut source: String, processor: &crate::Processor<'_>) -> String {
    let substitutions = processor.current_subs.get();
    if substitutions.contains(SubsFlags::ATTRIBUTES)
        && let Cow::Owned(expanded) = acdc_parser::substitute(
            &source,
            &[acdc_parser::Substitution::Attributes],
            &processor.document_attributes,
        )
    {
        source = expanded;
    }
    if let Cow::Owned(replaced) = apply_replacements(
        &source,
        substitutions,
        &Replacements::unicode(),
        TextBoundaries::BOTH,
    ) {
        source = replaced;
    }
    source
}

#[cfg(not(feature = "pre-spec-subs"))]
fn apply_source_substitutions(source: String, _processor: &crate::Processor<'_>) -> String {
    source
}

fn source_with_callout_placeholders(
    nodes: &[InlineNode<'_>],
    language: Option<&str>,
) -> (String, Vec<(String, usize)>) {
    let mut source = String::new();
    let mut callouts = Vec::new();
    let transform = InlineTextTransform::default().line_break("\n");
    let comment_prefix = default_line_comment(language);

    for (index, node) in nodes.iter().enumerate() {
        if let InlineNode::VerbatimText(verbatim) = node {
            let mut content = verbatim.content.to_string();
            if index
                .checked_sub(1)
                .is_some_and(|previous| is_xml_callout(nodes, previous))
            {
                content = content.strip_prefix("-->").unwrap_or(&content).to_string();
            }
            if index
                .checked_add(1)
                .is_some_and(|next| matches!(nodes.get(next), Some(InlineNode::CalloutRef(_))))
            {
                if index
                    .checked_add(1)
                    .is_some_and(|next| is_xml_callout(nodes, next))
                {
                    content = content.strip_suffix("<!--").unwrap_or(&content).to_string();
                } else {
                    content = strip_callout_guard(&content, comment_prefix);
                }
            }
            source.push_str(&content);
        } else if let InlineNode::CalloutRef(callout) = node {
            let offset = u32::try_from(callouts.len()).unwrap_or(0xFFFD).min(0xFFFD);
            let placeholder = char::from_u32(0xF0000 + offset)
                .unwrap_or('\u{F0000}')
                .to_string();
            source.push_str(&placeholder);
            callouts.push((placeholder, callout.number));
        } else {
            let _ = transform.write(&mut source, std::slice::from_ref(node));
        }
    }
    (source, callouts)
}

fn is_xml_callout(nodes: &[InlineNode<'_>], index: usize) -> bool {
    matches!(nodes.get(index), Some(InlineNode::CalloutRef(_)))
        && index.checked_sub(1).is_some_and(|previous| {
            matches!(
                nodes.get(previous),
                Some(InlineNode::VerbatimText(text)) if text.content.ends_with("<!--")
            )
        })
        && index.checked_add(1).is_some_and(|next| {
            matches!(
                nodes.get(next),
                Some(InlineNode::VerbatimText(text)) if text.content.starts_with("-->")
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
    use crate::TerminalVisitor;
    use crate::create_test_processor;
    use acdc_converters_core::visitor::Visitor;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor = TerminalVisitor::new(buffer, processor, diagnostics.reborrow());
        visitor.visit_delimited_block(&block)?;
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
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor1 =
            TerminalVisitor::new(&mut buffer1, processor.clone(), diagnostics.reborrow());
        visitor1.visit_delimited_block(&block1)?;

        let mut buffer2 = Vec::new();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor2 =
            TerminalVisitor::new(&mut buffer2, processor.clone(), diagnostics.reborrow());
        visitor2.visit_delimited_block(&block2)?;

        let mut buffer3 = Vec::new();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor3 =
            TerminalVisitor::new(&mut buffer3, processor.clone(), diagnostics.reborrow());
        visitor3.visit_delimited_block(&block3)?;

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
