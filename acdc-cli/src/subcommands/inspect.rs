use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use acdc_converters_core::{inlines_to_string, visitor::Visitor};
use acdc_parser::{
    Admonition, AttributeValue, Audio, Block, CalloutList, DelimitedBlock, DelimitedBlockType,
    DescriptionList, DiscreteHeader, Document, Header, Image, InlineNode, ListItem, Location,
    Options, OrderedList, PageBreak, Paragraph, Section, TableOfContents, ThematicBreak,
    UnorderedList, Video, parse_file,
};
use crossterm::style::Stylize;

/// Show a human-readable structural outline of an `AsciiDoc` document
#[derive(clap::Args)]
pub struct Args {
    /// Input `AsciiDoc` file
    pub file: PathBuf,

    /// Show location information (line:column)
    #[arg(long)]
    pub show_locations: bool,

    /// Maximum depth to display (0 = unlimited)
    #[arg(long, default_value = "0")]
    pub max_depth: usize,
}

struct TreeVisitor<W: Write> {
    writer: W,
    depth: usize,
    is_last_stack: Vec<bool>,
    show_locations: bool,
    max_depth: usize,
    color: bool,
}

impl<W: Write> TreeVisitor<W> {
    fn new(writer: W, show_locations: bool, max_depth: usize, color: bool) -> Self {
        Self {
            writer,
            depth: 0,
            is_last_stack: Vec::new(),
            show_locations,
            max_depth,
            color,
        }
    }

    fn should_show(&self) -> bool {
        self.max_depth == 0 || self.depth <= self.max_depth
    }

    fn print_tree_line(
        &mut self,
        name: &str,
        detail: Option<&str>,
        location: Option<&Location>,
    ) -> io::Result<()> {
        if !self.should_show() {
            return Ok(());
        }

        // Print tree structure: ├─, └─, │
        for i in 0..self.depth {
            if i == self.depth - 1 {
                let connector = if self.is_last_stack.get(i) == Some(&true) {
                    "└─ "
                } else {
                    "├─ "
                };
                write!(self.writer, "{connector}")?;
            } else if self.is_last_stack.get(i) == Some(&true) {
                write!(self.writer, "   ")?;
            } else {
                write!(self.writer, "│  ")?;
            }
        }

        if self.color {
            write!(self.writer, "{}", name.cyan().bold())?;
        } else {
            write!(self.writer, "{name}")?;
        }

        // Print detail if present
        if let Some(d) = detail {
            if self.color {
                write!(self.writer, ": {}", d.yellow())?;
            } else {
                write!(self.writer, ": {d}")?;
            }
        }

        // Print location if enabled
        if self.show_locations
            && let Some(location) = location
        {
            let loc_str = format!(
                " @{}:{} -> {}:{}",
                location.start.line, location.start.column, location.end.line, location.end.column
            );
            if self.color {
                write!(self.writer, "{}", loc_str.dark_grey())?;
            } else {
                write!(self.writer, "{loc_str}")?;
            }
        }

        writeln!(self.writer)?;

        Ok(())
    }

    fn enter(&mut self, is_last: bool) {
        self.is_last_stack.push(is_last);
        self.depth += 1;
    }

    fn exit(&mut self) {
        self.is_last_stack.pop();
        self.depth -= 1;
    }

    fn with_child<F>(&mut self, is_last: bool, f: F) -> io::Result<()>
    where
        F: FnOnce(&mut Self) -> io::Result<()>,
    {
        self.enter(is_last);
        let result = f(self);
        self.exit();
        result
    }
}

/// Truncate text for display
fn truncate(text: &str, max_len: usize) -> String {
    let character_count = text.chars().count();
    if character_count <= max_len {
        text.to_string()
    } else {
        let prefix: String = text.chars().take(max_len).collect();
        format!("{prefix}... ({character_count} chars)")
    }
}

impl<W: Write> Visitor for TreeVisitor<W> {
    type Error = io::Error;

    fn visit_document(&mut self, doc: &Document) -> Result<(), Self::Error> {
        if self.color {
            writeln!(self.writer, "{}", "Document".blue().bold())?;
        } else {
            writeln!(self.writer, "Document")?;
        }

        let visible_blocks = doc
            .blocks
            .iter()
            .filter(|block| !matches!(block, Block::DocumentAttribute(_) | Block::Comment(_)));
        let child_count = usize::from(doc.header.is_some()) + visible_blocks.clone().count();
        let mut child_index = 0;
        if let Some(header) = &doc.header {
            child_index += 1;
            self.with_child(child_index == child_count, |visitor| {
                visitor.visit_header(header)
            })?;
        }
        for block in visible_blocks {
            child_index += 1;
            self.with_child(child_index == child_count, |visitor| {
                visitor.visit_block(block)
            })?;
        }
        Ok(())
    }

    fn visit_header(&mut self, header: &Header) -> Result<(), Self::Error> {
        self.print_tree_line("Header", None, Some(&header.location))?;

        self.with_child(header.authors.is_empty(), |visitor| {
            if !header.title.is_empty() {
                let title_text = truncate(&inlines_to_string(&header.title), 60);
                visitor.print_tree_line("Title", Some(&title_text), None)?;

                if let Some(subtitle) = &header.subtitle {
                    visitor.with_child(true, |v| {
                        let subtitle_text = truncate(&inlines_to_string(subtitle), 60);
                        v.print_tree_line("Subtitle", Some(&subtitle_text), None)
                    })?;
                }
            }
            Ok(())
        })?;

        if !header.authors.is_empty() {
            self.with_child(true, |visitor| {
                let detail = format!("{} author(s)", header.authors.len());
                visitor.print_tree_line("Authors", Some(&detail), None)
            })?;
        }

        Ok(())
    }

    fn visit_section(&mut self, section: &Section) -> Result<(), Self::Error> {
        let detail = format!("Level {}", section.level);
        self.print_tree_line("Section", Some(&detail), Some(&section.location))?;

        let child_count = usize::from(!section.title.is_empty()) + section.content.len();
        let mut child_index = 0;
        if !section.title.is_empty() {
            child_index += 1;
            self.with_child(child_index == child_count, |visitor| {
                let title = inlines_to_string(&section.title);
                let title_text = truncate(&title, 60);
                visitor.print_tree_line("Title", Some(&title_text), None)
            })?;
        }
        for block in &section.content {
            child_index += 1;
            self.with_child(child_index == child_count, |visitor| {
                visitor.visit_block(block)
            })?;
        }

        Ok(())
    }

    fn visit_paragraph(&mut self, para: &Paragraph) -> Result<(), Self::Error> {
        let text = inlines_to_string(&para.content);
        let preview = truncate(&text, 50);
        self.print_tree_line("Paragraph", Some(&preview), Some(&para.location))?;
        Ok(())
    }

    fn visit_delimited_block(&mut self, block: &DelimitedBlock) -> Result<(), Self::Error> {
        let block_type = match &block.inner {
            DelimitedBlockType::DelimitedListing(_) => "Listing",
            DelimitedBlockType::DelimitedLiteral(_) => "Literal",
            DelimitedBlockType::DelimitedExample(_) => "Example",
            DelimitedBlockType::DelimitedQuote(_) => "Quote",
            DelimitedBlockType::DelimitedSidebar(_) => "Sidebar",
            DelimitedBlockType::DelimitedOpen(_) => "Open",
            DelimitedBlockType::DelimitedVerse(_) => "Verse",
            DelimitedBlockType::DelimitedPass(_) => "Pass",
            DelimitedBlockType::DelimitedStem(_) => "Stem",
            DelimitedBlockType::DelimitedComment(_) => "Comment",
            DelimitedBlockType::DelimitedTable(_) => "Table",
            _ => "Unknown",
        };

        self.print_tree_line("DelimitedBlock", Some(block_type), Some(&block.location))?;

        // Show metadata if present
        let has_metadata = block.metadata.style.is_some() || !block.metadata.attributes.is_empty();

        if has_metadata {
            self.with_child(true, |visitor| {
                if let Some(style) = &block.metadata.style {
                    visitor.print_tree_line("Style", Some(style), None)?;
                }

                for (key, value) in block.metadata.attributes.iter() {
                    let detail: String = match value {
                        AttributeValue::String(s) => s.clone().into_owned(),
                        AttributeValue::Bool(b) => b.to_string(),
                        AttributeValue::None => "(null)".to_string(),
                        _ => "(unknown)".to_string(),
                    };
                    visitor.print_tree_line(&format!("Attribute: {key}"), Some(&detail), None)?;
                }

                Ok(())
            })?;
        }

        Ok(())
    }

    fn visit_unordered_list(&mut self, list: &UnorderedList) -> Result<(), Self::Error> {
        let detail = format!("{} items", list.items.len());
        self.print_tree_line("UnorderedList", Some(&detail), Some(&list.location))?;
        Ok(())
    }

    fn visit_ordered_list(&mut self, list: &OrderedList) -> Result<(), Self::Error> {
        let detail = format!("{} items", list.items.len());
        self.print_tree_line("OrderedList", Some(&detail), Some(&list.location))?;
        Ok(())
    }

    fn visit_description_list(&mut self, list: &DescriptionList) -> Result<(), Self::Error> {
        let detail = format!("{} items", list.items.len());
        self.print_tree_line("DescriptionList", Some(&detail), Some(&list.location))?;
        Ok(())
    }

    fn visit_callout_list(&mut self, list: &CalloutList) -> Result<(), Self::Error> {
        let detail = format!("{} items", list.items.len());
        self.print_tree_line("CalloutList", Some(&detail), Some(&list.location))?;
        Ok(())
    }

    fn visit_list_item(&mut self, _item: &ListItem) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_admonition(&mut self, admonition: &Admonition) -> Result<(), Self::Error> {
        let variant = match admonition.variant {
            acdc_parser::AdmonitionVariant::Note => "Note",
            acdc_parser::AdmonitionVariant::Tip => "Tip",
            acdc_parser::AdmonitionVariant::Important => "Important",
            acdc_parser::AdmonitionVariant::Warning => "Warning",
            acdc_parser::AdmonitionVariant::Caution => "Caution",
        };
        self.print_tree_line("Admonition", Some(variant), Some(&admonition.location))?;
        Ok(())
    }

    fn visit_image(&mut self, image: &Image) -> Result<(), Self::Error> {
        let detail = image.source.to_string();
        self.print_tree_line("Image", Some(&truncate(&detail, 50)), Some(&image.location))?;
        Ok(())
    }

    fn visit_video(&mut self, video: &Video) -> Result<(), Self::Error> {
        let detail = if let Some(source) = video.sources.first() {
            source.to_string()
        } else {
            "(no sources)".to_string()
        };
        self.print_tree_line("Video", Some(&truncate(&detail, 50)), Some(&video.location))?;
        Ok(())
    }

    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error> {
        let detail = audio.source.to_string();
        self.print_tree_line("Audio", Some(&truncate(&detail, 50)), Some(&audio.location))?;
        Ok(())
    }

    fn visit_page_break(&mut self, page_break: &PageBreak) -> Result<(), Self::Error> {
        self.print_tree_line("PageBreak", None, Some(&page_break.location))?;
        Ok(())
    }

    fn visit_thematic_break(&mut self, thematic_break: &ThematicBreak) -> Result<(), Self::Error> {
        self.print_tree_line("ThematicBreak", None, Some(&thematic_break.location))?;
        Ok(())
    }

    fn visit_table_of_contents(&mut self, toc: &TableOfContents) -> Result<(), Self::Error> {
        self.print_tree_line("TableOfContents", None, Some(&toc.location))?;
        Ok(())
    }

    fn visit_discrete_header(&mut self, header: &DiscreteHeader) -> Result<(), Self::Error> {
        let title = inlines_to_string(&header.title);
        let detail = format!("Level {} - {}", header.level, truncate(&title, 40));
        self.print_tree_line("DiscreteHeader", Some(&detail), Some(&header.location))?;
        Ok(())
    }

    // Required trait methods with no-op implementations for inline nodes
    fn visit_inline_nodes(&mut self, _inlines: &[InlineNode]) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_inline_node(&mut self, _inline: &InlineNode) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_text(&mut self, _text: &str) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::default();
    let parsed = parse_file(&args.file, &options)?;

    let stdout = io::stdout();
    let color = stdout.is_terminal();
    let mut visitor = TreeVisitor::new(stdout.lock(), args.show_locations, args.max_depth, color);
    visitor.visit_document(parsed.document())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use acdc_parser::{Options, parse};

    use super::*;

    #[test]
    fn truncates_at_unicode_scalar_boundaries() {
        assert_eq!(truncate("éclair", 2), "éc... (6 chars)");
        assert_eq!(truncate("éclair", 6), "éclair");
    }

    #[test]
    fn renders_truthful_plain_tree_connectors() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse("= Document\n\n== Section\n\nBody.\n", &Options::default())?;
        let mut output = Vec::new();
        TreeVisitor::new(&mut output, false, 0, false).visit_document(parsed.document())?;
        let output = String::from_utf8(output)?;

        assert!(output.contains("Document\n├─ Header"));
        assert!(output.contains("└─ Section: Level 1"));
        assert!(output.contains("   ├─ Title: Section"));
        assert!(output.contains("   └─ Paragraph: Body."));
        assert!(!output.contains('\u{1b}'));
        Ok(())
    }

    #[test]
    fn max_depth_hides_deeper_nodes() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse("= Document\n\n== Section\n\nBody.\n", &Options::default())?;
        let mut output = Vec::new();
        TreeVisitor::new(&mut output, false, 1, false).visit_document(parsed.document())?;
        let output = String::from_utf8(output)?;

        assert!(output.contains("Section: Level 1"));
        assert!(!output.contains("Title: Section"));
        assert!(!output.contains("Paragraph: Body."));
        Ok(())
    }
}
