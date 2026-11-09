use std::{
    error::Error,
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use acdc_converters_core::{
    InlineTextTransform, TraversalContext,
    visitor::{BlockVisitor, DocumentVisitor},
};
use acdc_parser::{
    AdmonitionVariant, Options,
    ast::{
        AttributeValueRef, BlockKind, BlockRef, DocumentRef, HeaderRef, LocationRef, SectionRef,
    },
    parse_file,
};
use crossterm::style::Stylize;

/// Show a human-readable structural outline of an `AsciiDoc` document
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Language rules: asciidoctor or draft
    #[arg(long, value_name = "MODE", default_value_t = acdc_parser::CompatibilityMode::default())]
    compatibility_mode: acdc_parser::CompatibilityMode,

    /// Safe mode to use while parsing the document
    #[arg(short = 'S', long, default_value = "unsafe")]
    safe_mode: acdc_parser::SafeMode,

    /// Maximum selected include text per document, in bytes (64 MiB by default)
    ///
    /// Zero permits only empty selections. The 10 MiB per-include limit still applies.
    #[arg(long, value_name = "BYTES", default_value_t = Options::default().max_total_include_bytes())]
    max_total_include_bytes: usize,

    /// Input `AsciiDoc` file
    file: PathBuf,

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
        location: Option<LocationRef<'_>>,
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
                location.start().line(),
                location.start().column(),
                location.end().line(),
                location.end().column()
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

impl<'doc, W: Write> DocumentVisitor<'doc> for TreeVisitor<W> {
    fn visit_document(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        document: DocumentRef<'doc>,
    ) -> Result<(), Self::Error> {
        if self.color {
            writeln!(self.writer, "{}", "Document".blue().bold())?;
        } else {
            writeln!(self.writer, "Document")?;
        }
        let visible = |block: &BlockRef<'_>| {
            !matches!(
                block.kind(),
                BlockKind::DocumentAttribute(_) | BlockKind::Comment(_)
            )
        };
        let child_count =
            usize::from(document.header().is_some()) + document.blocks().filter(visible).count();
        let mut child_index = 0;
        if let Some(header) = document.header() {
            child_index += 1;
            self.with_child(child_index == child_count, |visitor| {
                visitor.visit_header(traversal, header)
            })?;
        }
        for block in document.blocks().filter(visible) {
            child_index += 1;
            self.with_child(child_index == child_count, |visitor| {
                traversal.visit_block(visitor, block)
            })?;
        }
        Ok(())
    }

    fn visit_header(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        header: HeaderRef<'doc>,
    ) -> Result<(), Self::Error> {
        self.print_tree_line("Header", None, Some(header.location()))?;
        let authors = header.authors().count();
        self.with_child(authors == 0, |visitor| {
            if let Some(title) = header.title().filter(|title| !title.is_empty()) {
                let title_text = InlineTextTransform::default().content_to_string(title);
                visitor.print_tree_line("Title", Some(&truncate(&title_text, 60)), None)?;
                if let Some(subtitle) = header.subtitle() {
                    visitor.with_child(true, |v| {
                        let subtitle_text =
                            InlineTextTransform::default().content_to_string(subtitle);
                        v.print_tree_line("Subtitle", Some(&truncate(&subtitle_text, 60)), None)
                    })?;
                }
            }
            Ok(())
        })?;
        if authors != 0 {
            self.with_child(true, |visitor| {
                visitor.print_tree_line("Authors", Some(&format!("{authors} author(s)")), None)
            })?;
        }
        Ok(())
    }
}

impl<'doc, W: Write> BlockVisitor<'doc> for TreeVisitor<W> {
    type Error = io::Error;

    fn visit_block(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        block: BlockRef<'doc>,
    ) -> Result<(), Self::Error> {
        let location = Some(block.location());
        match block.kind() {
            BlockKind::Section(section) => self.print_section(traversal, section),
            BlockKind::Paragraph(paragraph) => {
                let text = InlineTextTransform::default().content_to_string(paragraph.content());
                self.print_tree_line("Paragraph", Some(&truncate(&text, 50)), location)
            }
            BlockKind::Listing(_) => self.print_delimited(block, "Listing"),
            BlockKind::Literal(_) => self.print_delimited(block, "Literal"),
            BlockKind::Example(_) => self.print_delimited(block, "Example"),
            BlockKind::Quote(_) => self.print_delimited(block, "Quote"),
            BlockKind::Sidebar(_) => self.print_delimited(block, "Sidebar"),
            BlockKind::Open(_) => self.print_delimited(block, "Open"),
            BlockKind::Verse(_) => self.print_delimited(block, "Verse"),
            BlockKind::Passthrough(_) => self.print_delimited(block, "Pass"),
            BlockKind::Stem(_) => self.print_delimited(block, "Stem"),
            BlockKind::DelimitedComment(_) => self.print_delimited(block, "Comment"),
            BlockKind::Table(_) => self.print_delimited(block, "Table"),
            BlockKind::UnorderedList(list) => {
                self.print_items("UnorderedList", list.items().count(), location)
            }
            BlockKind::OrderedList(list) => {
                self.print_items("OrderedList", list.items().count(), location)
            }
            BlockKind::DescriptionList(list) => {
                self.print_items("DescriptionList", list.items().count(), location)
            }
            BlockKind::CalloutList(list) => {
                self.print_items("CalloutList", list.items().count(), location)
            }
            BlockKind::Admonition(admonition) => {
                let variant = match admonition.variant() {
                    AdmonitionVariant::Note => "Note",
                    AdmonitionVariant::Tip => "Tip",
                    AdmonitionVariant::Important => "Important",
                    AdmonitionVariant::Warning => "Warning",
                    AdmonitionVariant::Caution => "Caution",
                };
                self.print_tree_line("Admonition", Some(variant), location)
            }
            BlockKind::Image(image) => self.print_tree_line(
                "Image",
                Some(&truncate(&image.source().to_string(), 50)),
                location,
            ),
            BlockKind::Video(video) => self.print_tree_line(
                "Video",
                Some(&truncate(&video.source().to_string(), 50)),
                location,
            ),
            BlockKind::Audio(audio) => self.print_tree_line(
                "Audio",
                Some(&truncate(&audio.source().to_string(), 50)),
                location,
            ),
            BlockKind::PageBreak(_) => self.print_tree_line("PageBreak", None, location),
            BlockKind::ThematicBreak(_) => self.print_tree_line("ThematicBreak", None, location),
            BlockKind::TableOfContents(_) => {
                self.print_tree_line("TableOfContents", None, location)
            }
            BlockKind::DiscreteHeading(header) => {
                let title = header
                    .title()
                    .map(|title| InlineTextTransform::default().content_to_string(title))
                    .unwrap_or_default();
                let detail = format!("Level {} - {}", header.level(), truncate(&title, 40));
                self.print_tree_line("DiscreteHeader", Some(&detail), location)
            }
            BlockKind::DocumentAttribute(_) | BlockKind::Comment(_) => Ok(()),
            _ => self.print_tree_line("Unknown", None, location),
        }
    }
}

impl<W: Write> TreeVisitor<W> {
    fn print_section<'doc>(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        section: SectionRef<'doc>,
    ) -> io::Result<()> {
        self.print_tree_line(
            "Section",
            Some(&format!("Level {}", section.level())),
            Some(section.location()),
        )?;
        let title = section.title().filter(|title| !title.is_empty());
        let child_count = usize::from(title.is_some()) + section.blocks().count();
        let mut child_index = 0;
        if let Some(title) = title {
            child_index += 1;
            self.with_child(child_index == child_count, |visitor| {
                let title = InlineTextTransform::default().content_to_string(title);
                visitor.print_tree_line("Title", Some(&truncate(&title, 60)), None)
            })?;
        }
        for block in section.blocks() {
            child_index += 1;
            self.with_child(child_index == child_count, |visitor| {
                traversal.visit_block(visitor, block)
            })?;
        }
        Ok(())
    }

    fn print_items(
        &mut self,
        name: &str,
        count: usize,
        location: Option<LocationRef<'_>>,
    ) -> io::Result<()> {
        self.print_tree_line(name, Some(&format!("{count} items")), location)
    }

    fn print_delimited(&mut self, block: BlockRef<'_>, name: &str) -> io::Result<()> {
        self.print_tree_line("DelimitedBlock", Some(name), Some(block.location()))?;
        let Some(attributes) = block.attributes() else {
            return Ok(());
        };
        let style = match attributes.get("style") {
            Some(AttributeValueRef::Text(style)) => Some(style),
            _ => None,
        };
        // The metadata summary omits identity and content attributes.
        let mut controls = attributes
            .iter()
            .filter_map(|(key, value)| {
                if matches!(key, "style" | "id") {
                    return None;
                }
                let detail = match value {
                    AttributeValueRef::Text(value) => value,
                    AttributeValueRef::Bool(value) => {
                        if value {
                            "true"
                        } else {
                            "false"
                        }
                    }
                    AttributeValueRef::None => "(null)",
                    AttributeValueRef::Content(_)
                    | AttributeValueRef::ContentList(_)
                    | AttributeValueRef::Tokens(_)
                    | AttributeValueRef::Substitutions(_)
                    | _ => return None,
                };
                Some((key, detail))
            })
            .collect::<Vec<_>>();
        controls.sort_unstable_by_key(|(name, _)| *name);
        if style.is_some() || !controls.is_empty() {
            self.with_child(true, |visitor| {
                if let Some(style) = style {
                    visitor.print_tree_line("Style", Some(style), None)?;
                }
                for (key, detail) in controls {
                    visitor.print_tree_line(&format!("Attribute: {key}"), Some(detail), None)?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }
}

pub(crate) fn run(args: &Args) -> Result<(), Box<dyn Error>> {
    let options = Options::builder()
        .with_safe_mode(args.safe_mode)
        .with_compatibility_mode(args.compatibility_mode)
        .with_max_total_include_bytes(args.max_total_include_bytes)
        .build()?;
    let parsed = parse_file(&args.file, &options)?;

    let stdout = io::stdout();
    let color = stdout.is_terminal();
    let mut visitor = TreeVisitor::new(stdout.lock(), args.show_locations, args.max_depth, color);
    visitor.visit_document(
        &mut TraversalContext::new(parsed.document().attributes()),
        parsed.document(),
    )?;

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
    fn renders_truthful_plain_tree_connectors() -> Result<(), Box<dyn Error>> {
        let parsed = parse("= Document\n\n== Section\n\nBody.\n", &Options::default())?;
        let mut output = Vec::new();
        TreeVisitor::new(&mut output, false, 0, false).visit_document(
            &mut TraversalContext::new(parsed.document().attributes()),
            parsed.document(),
        )?;
        let output = String::from_utf8(output)?;

        assert!(output.contains("Document\n├─ Header"));
        assert!(output.contains("└─ Section: Level 1"));
        assert!(output.contains("   ├─ Title: Section"));
        assert!(output.contains("   └─ Paragraph: Body."));
        assert!(!output.contains('\u{1b}'));
        Ok(())
    }

    #[test]
    fn max_depth_hides_deeper_nodes() -> Result<(), Box<dyn Error>> {
        let parsed = parse("= Document\n\n== Section\n\nBody.\n", &Options::default())?;
        let mut output = Vec::new();
        TreeVisitor::new(&mut output, false, 1, false).visit_document(
            &mut TraversalContext::new(parsed.document().attributes()),
            parsed.document(),
        )?;
        let output = String::from_utf8(output)?;

        assert!(output.contains("Section: Level 1"));
        assert!(!output.contains("Title: Section"));
        assert!(!output.contains("Paragraph: Body."));
        Ok(())
    }

    #[test]
    fn delimited_controls_are_sorted_without_duplicating_dedicated_fields()
    -> Result<(), Box<dyn Error>> {
        for mode in [
            acdc_parser::CompatibilityMode::Asciidoctor,
            acdc_parser::CompatibilityMode::Draft,
        ] {
            let parsed = parse(
                "[source,rust,id=script,role=sample,opts=nowrap,title='*Title*',zeta=last,alpha=first]\n----\nlet x = 1;\n----\n",
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let mut output = Vec::new();
            TreeVisitor::new(&mut output, true, 0, false).visit_document(
                &mut TraversalContext::new(parsed.document().attributes()),
                parsed.document(),
            )?;
            let output = String::from_utf8(output)?;
            assert!(output.contains("DelimitedBlock: Listing @1:1"));
            assert!(output.contains("Style: source"));
            assert!(output.contains("Attribute: language: rust"));
            assert!(output.contains("Attribute: alpha: first"));
            assert!(output.contains("Attribute: zeta: last"));
            let controls = output
                .lines()
                .filter(|line| line.contains("Attribute: "))
                .collect::<Vec<_>>();
            let mut sorted = controls.clone();
            sorted.sort_unstable();
            assert_eq!(controls, sorted);
            for field in ["id", "title", "role", "opts", "style"] {
                assert!(!output.contains(&format!("Attribute: {field}:")));
            }
        }
        Ok(())
    }
}
