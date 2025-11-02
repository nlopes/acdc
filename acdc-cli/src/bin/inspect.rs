use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

use acdc_converters_common::visitor::Visitor;
use acdc_parser::{
    Admonition, AttributeValue, Audio, CalloutList, DelimitedBlock, DelimitedBlockType,
    DescriptionList, DiscreteHeader, Document, Header, Image, InlineMacro, InlineNode, ListItem,
    Location, Options, OrderedList, PageBreak, Paragraph, Section, TableOfContents, ThematicBreak,
    UnorderedList, Video, parse,
};
use clap::Parser as _;
use crossterm::style::Stylize;

/// Extract plain text from inline nodes, recursively handling formatted text.
fn inlines_to_string(inlines: &[InlineNode]) -> String {
    inlines
        .iter()
        .map(|node| match node {
            InlineNode::PlainText(text) => text.content.clone(),
            InlineNode::RawText(text) => text.content.clone(),
            InlineNode::VerbatimText(text) => text.content.clone(),
            InlineNode::BoldText(bold) => inlines_to_string(&bold.content),
            InlineNode::ItalicText(italic) => inlines_to_string(&italic.content),
            InlineNode::MonospaceText(mono) => inlines_to_string(&mono.content),
            InlineNode::HighlightText(highlight) => inlines_to_string(&highlight.content),
            InlineNode::SubscriptText(sub) => inlines_to_string(&sub.content),
            InlineNode::SuperscriptText(sup) => inlines_to_string(&sup.content),
            InlineNode::CurvedQuotationText(quote) => inlines_to_string(&quote.content),
            InlineNode::CurvedApostropheText(apos) => inlines_to_string(&apos.content),
            InlineNode::StandaloneCurvedApostrophe(_) => "'".to_string(),
            InlineNode::LineBreak(_) => " ".to_string(),
            InlineNode::InlineAnchor(anchor) => format!(
                "[#{}{}]",
                anchor.id,
                anchor
                    .xreflabel
                    .as_deref()
                    .map_or(String::new(), |l| format!("|{l}"))
            ),
            InlineNode::Macro(macro_node) => match macro_node {
                InlineMacro::Link(link) => {
                    link.text.clone().unwrap_or_else(|| link.target.to_string())
                }
                InlineMacro::Url(url) => {
                    if url.text.is_empty() {
                        url.target.to_string()
                    } else {
                        inlines_to_string(&url.text)
                    }
                }
                InlineMacro::Autolink(autolink) => autolink.url.to_string(),
                InlineMacro::CrossReference(xref) => {
                    xref.text.clone().unwrap_or_else(|| xref.target.clone())
                }
                _ => String::new(),
            },
            _ => String::new(),
        })
        .collect()
}

#[derive(clap::Parser)]
#[command(name = "inspect_ast")]
#[command(about = "Inspect the AST structure of an AsciiDoc document", long_about = None)]
struct Args {
    /// Input `AsciiDoc` file
    file: PathBuf,

    /// Disable colored output
    #[arg(long)]
    no_color: bool,

    /// Show location information (line:column)
    #[arg(long)]
    show_locations: bool,

    /// Maximum depth to display (0 = unlimited)
    #[arg(long, default_value = "0")]
    max_depth: usize,
}

struct TreeVisitor<W: Write> {
    writer: W,
    depth: usize,
    is_last_stack: Vec<bool>,
    show_locations: bool,
    color: bool,
    max_depth: usize,
}

impl<W: Write> TreeVisitor<W> {
    fn new(writer: W, color: bool, show_locations: bool, max_depth: usize) -> Self {
        Self {
            writer,
            depth: 0,
            is_last_stack: Vec::new(),
            show_locations,
            color,
            max_depth,
        }
    }

    fn should_show(&self) -> bool {
        self.max_depth == 0 || self.depth <= self.max_depth
    }

    fn print_tree_line(&mut self, name: &str, detail: Option<&str>) -> io::Result<()> {
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

        // Print node name in color
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

        writeln!(self.writer)?;

        Ok(())
    }

    fn print_tree_line_with_location(
        &mut self,
        name: &str,
        detail: Option<&str>,
        location: &Location,
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

        // Print node name in color
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
        if self.show_locations {
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
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}... ({} chars)", &text[..max_len], text.len())
    }
}

impl<W: Write> Visitor for TreeVisitor<W> {
    type Error = io::Error;

    fn visit_document_start(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        if self.color {
            writeln!(self.writer, "{}", "Document".blue().bold())?;
        } else {
            writeln!(self.writer, "Document")?;
        }
        Ok(())
    }

    fn visit_header(&mut self, header: &Header) -> Result<(), Self::Error> {
        self.print_tree_line_with_location("Header", None, &header.location)?;

        self.with_child(header.authors.is_empty(), |visitor| {
            if !header.title.is_empty() {
                let title_text = truncate(&inlines_to_string(&header.title), 60);
                visitor.print_tree_line("Title", Some(&title_text))?;

                if let Some(subtitle) = &header.subtitle {
                    visitor.with_child(true, |v| {
                        let subtitle_text = truncate(&inlines_to_string(subtitle), 60);
                        v.print_tree_line("Subtitle", Some(&subtitle_text))
                    })?;
                }
            }
            Ok(())
        })?;

        if !header.authors.is_empty() {
            self.with_child(true, |visitor| {
                let detail = format!("{} author(s)", header.authors.len());
                visitor.print_tree_line("Authors", Some(&detail))
            })?;
        }

        Ok(())
    }

    fn visit_section(&mut self, section: &Section) -> Result<(), Self::Error> {
        let detail = format!("Level {}", section.level);
        self.print_tree_line_with_location("Section", Some(&detail), &section.location)?;

        self.with_child(false, |visitor| {
            if !section.title.is_empty() {
                let title = inlines_to_string(&section.title);
                let title_text = truncate(&title, 60);
                visitor.print_tree_line("Title", Some(&title_text))?;
            }

            for (i, block) in section.content.iter().enumerate() {
                let is_last = i == section.content.len() - 1;
                visitor.with_child(is_last, |v| v.visit_block(block))?;
            }

            Ok(())
        })?;

        Ok(())
    }

    fn visit_paragraph(&mut self, para: &Paragraph) -> Result<(), Self::Error> {
        let text = inlines_to_string(&para.content);
        let preview = truncate(&text, 50);
        self.print_tree_line_with_location("Paragraph", Some(&preview), &para.location)?;
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

        self.print_tree_line_with_location("DelimitedBlock", Some(block_type), &block.location)?;

        // Show metadata if present
        let has_metadata = block.metadata.style.is_some() || !block.metadata.attributes.is_empty();

        if has_metadata {
            self.with_child(true, |visitor| {
                if let Some(style) = &block.metadata.style {
                    visitor.print_tree_line("Style", Some(style))?;
                }

                for (key, value) in block.metadata.attributes.iter() {
                    let detail = match value {
                        AttributeValue::String(s) => s.clone(),
                        AttributeValue::Bool(b) => b.to_string(),
                        AttributeValue::None => "(null)".to_string(),
                        AttributeValue::Inlines(_) => "(inline content)".to_string(),
                    };
                    visitor.print_tree_line(&format!("Attribute: {key}"), Some(&detail))?;
                }

                Ok(())
            })?;
        }

        Ok(())
    }

    fn visit_unordered_list(&mut self, list: &UnorderedList) -> Result<(), Self::Error> {
        let detail = format!("{} items", list.items.len());
        self.print_tree_line("UnorderedList", Some(&detail))?;
        Ok(())
    }

    fn visit_ordered_list(&mut self, list: &OrderedList) -> Result<(), Self::Error> {
        let detail = format!("{} items", list.items.len());
        self.print_tree_line("OrderedList", Some(&detail))?;
        Ok(())
    }

    fn visit_description_list(&mut self, list: &DescriptionList) -> Result<(), Self::Error> {
        let detail = format!("{} items", list.items.len());
        self.print_tree_line("DescriptionList", Some(&detail))?;
        Ok(())
    }

    fn visit_callout_list(&mut self, list: &CalloutList) -> Result<(), Self::Error> {
        let detail = format!("{} items", list.items.len());
        self.print_tree_line("CalloutList", Some(&detail))?;
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
        self.print_tree_line_with_location("Admonition", Some(variant), &admonition.location)?;
        Ok(())
    }

    fn visit_image(&mut self, image: &Image) -> Result<(), Self::Error> {
        let detail = image.source.to_string();
        self.print_tree_line_with_location("Image", Some(&truncate(&detail, 50)), &image.location)?;
        Ok(())
    }

    fn visit_video(&mut self, video: &Video) -> Result<(), Self::Error> {
        let detail = if let Some(source) = video.sources.first() {
            source.to_string()
        } else {
            "(no sources)".to_string()
        };
        self.print_tree_line_with_location("Video", Some(&truncate(&detail, 50)), &video.location)?;
        Ok(())
    }

    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error> {
        let detail = audio.source.to_string();
        self.print_tree_line_with_location("Audio", Some(&truncate(&detail, 50)), &audio.location)?;
        Ok(())
    }

    fn visit_page_break(&mut self, page_break: &PageBreak) -> Result<(), Self::Error> {
        self.print_tree_line_with_location("PageBreak", None, &page_break.location)?;
        Ok(())
    }

    fn visit_thematic_break(&mut self, thematic_break: &ThematicBreak) -> Result<(), Self::Error> {
        self.print_tree_line_with_location("ThematicBreak", None, &thematic_break.location)?;
        Ok(())
    }

    fn visit_table_of_contents(&mut self, toc: &TableOfContents) -> Result<(), Self::Error> {
        self.print_tree_line_with_location("TableOfContents", None, &toc.location)?;
        Ok(())
    }

    fn visit_discrete_header(&mut self, header: &DiscreteHeader) -> Result<(), Self::Error> {
        let title = inlines_to_string(&header.title);
        let detail = format!("Level {} - {}", header.level, truncate(&title, 40));
        self.print_tree_line_with_location("DiscreteHeader", Some(&detail), &header.location)?;
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Read input file
    let content = fs::read_to_string(&args.file)?;

    // Parse document
    let options = Options::default();
    let doc = parse(&content, &options)?;

    // Create tree visitor
    let stdout = io::stdout();
    let mut visitor = TreeVisitor::new(
        stdout.lock(),
        !args.no_color,
        args.show_locations,
        args.max_depth,
    );

    // Visit document
    visitor.visit_document(&doc)?;

    Ok(())
}
