use std::{
    borrow::Cow, collections::HashSet, fmt::Write as _, mem::replace, num::NonZeroU32, ops::Range,
    rc::Rc,
};

use acdc_converters_core::{
    Diagnostics, Doctype, InlineTextTransform, TraversalContext,
    code::{
        CodeLinkTarget, SourceLineOptions, code_inline_content, resolve_code_link,
        source_line_count,
    },
    document_attribute_text,
    icon::{IconMode, image_source as icon_image_source},
    link::{autolink_fallback, link_fallback, mailto_fallback, mailto_target},
    list::OrderedListNumbering,
    section::effective_section_level,
    substitutions::{Replacements, TextBoundaries, apply_passthrough_replacements_iter},
    table::{CellKind, GridRow, column_count, grid_rows},
    toc::{Config as TocConfig, NumberingConfig, entry_level, has_parts, number_prefixes},
    visitor::InlineVisitor,
    xref::{XrefDisplay, interdocument_xref, resolve_reference},
};
use acdc_parser::{
    CaptionKind, ColumnStyle, ColumnWidth, HorizontalAlignment, Location, SectionKind,
    Substitution, TableFrame, TableGrid, TablePresentation, TableStripes, TocEntry,
    VerticalAlignment,
    ast::{
        AnchorRef, AttributeValueRef, AttributesRef, AutolinkRef, BlockKind, BlockRef, CaptionRef,
        ColumnFormatRef, ContentRef, CrossReferenceRef, DescriptionListItemRef, DescriptionListRef,
        DocumentRef, ElementIdRef, FootnoteRef, IconRef, ImageRef, IndexTermRef,
        IndexTermRelationshipRef, InlineImageRef, InlineKind, InlineRef, ListItemRef, LocationRef,
        MenuRef, PageBreakRef, ParagraphRef, RawRef, ReferenceRef, SectionRef, SourceRef,
        TableCellRef, TableOfContentsRef, TableRef, TableRowRef, TokensRef, UnorderedListRef,
    },
};
use acdc_pdf_images::ImageMap;
use acdc_pdf_theme::{
    Heading, PageBreakBefore, Palette, PartBreakAfter, Table as TableTheme, TableAlignment,
};
use acdc_pdf_typst::{PageLayout, PageSize, Writer};
use unicode_width::UnicodeWidthChar;

use crate::{
    Error, PageNumberingPlan, PageSetup, Processor, TypstSourceConfig,
    anchors::Anchors,
    code_wrap_columns, encode_bibliography_reference_label, encode_footnote_label, encode_label,
    has_autofit_option,
    index::{CatalogEntry, CatalogRelationship, CatalogTerm, IndexCatalog, PageSequenceStyle},
    warn_with_advice_at,
};

#[derive(Default)]
struct CodeText {
    source: String,
    indexes: Vec<(usize, usize)>,
    links: Vec<CodeSpan>,
}

impl CodeText {
    fn push_inline_text(&mut self, node: InlineRef<'_>, boundaries: TextBoundaries) {
        let decode = matches!(node.kind(), InlineKind::CharacterReference(_))
            || matches!(node.kind(), InlineKind::Raw(raw) if raw.substitutions().next().is_none());
        let _ = InlineTextTransform::default()
            .line_break("\n")
            .rendered()
            .text_boundaries(boundaries)
            .decode_char_refs(decode)
            .write_inlines(&mut self.source, std::iter::once(node));
    }

    fn wrap_span(&mut self, mut outer: CodeSpan) {
        if outer.target.is_none() && outer.anchor.is_none() && outer.prefix.is_empty() {
            return;
        }
        let mut spans = Vec::new();
        let mut start = 0;
        // Flatten nested formatting into disjoint ranges for line wrapping.
        // Links keep their own targets, and footnotes keep their own actions.
        for mut span in self.links.drain(..) {
            if start < span.range.start || outer.anchor.is_some() {
                spans.push(CodeSpan {
                    range: start..span.range.start,
                    anchor: outer.anchor.take(),
                    ..outer.clone()
                });
            }
            start = span.range.end;
            if span.replacement.is_none() && span.target.is_none() {
                span.target.clone_from(&outer.target);
            }
            span.prefix.insert_str(0, &outer.prefix);
            span.suffix.push_str(&outer.suffix);
            spans.push(span);
        }
        if start < self.source.len() || outer.anchor.is_some() {
            spans.push(CodeSpan {
                range: start..self.source.len(),
                ..outer
            });
        }
        self.links = spans;
    }

    fn append(&mut self, child: Self) {
        let offset = self.source.len();
        self.source.push_str(&child.source);
        self.indexes.extend(
            child
                .indexes
                .into_iter()
                .map(|(position, id)| (position + offset, id)),
        );
        self.links.extend(child.links.into_iter().map(|mut link| {
            link.range = link.range.start + offset..link.range.end + offset;
            link
        }));
    }
}

#[derive(Clone, Default)]
struct CodeSpan {
    range: Range<usize>,
    target: Option<CodeLinkTarget>,
    anchor: Option<String>,
    replacement: Option<String>,
    prefix: String,
    suffix: String,
}

#[derive(Default)]
struct CodeAnnotations {
    anchors: Vec<Vec<usize>>,
    links: Vec<Vec<CodeSpan>>,
}

impl CodeAnnotations {
    fn is_empty(&self) -> bool {
        self.anchors.is_empty() && self.links.is_empty()
    }
}

#[derive(Clone, Copy, Default)]
enum TableCellSectionState {
    #[default]
    Outside,
    Inside {
        width_columns: usize,
    },
}

#[derive(Clone, Copy)]
enum TableTextKind {
    Text,
    InlineVerbatim,
    Literal,
}

#[derive(Clone, Copy, Default)]
enum TextWhitespace {
    #[default]
    Collapse,
    Preserve,
}

#[derive(Clone, Copy)]
enum TableColumnTrack {
    Fraction(u32),
    Percentage(u32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ImageWidth {
    Points {
        value: f64,
        constrain_to_bounds: bool,
    },
    ContainerRatio {
        value: f64,
        constrain_to_bounds: bool,
    },
    IntrinsicRatio(f64),
    ViewportRatio(f64),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BlockImageAlignment {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UnorderedMarker {
    Disc,
    Circle,
    Square,
    None,
    NoBullet,
    Unstyled,
}

impl UnorderedMarker {
    fn from_style(style: &str) -> Option<Self> {
        match style {
            "disc" => Some(Self::Disc),
            "circle" => Some(Self::Circle),
            "square" => Some(Self::Square),
            "none" => Some(Self::None),
            "no-bullet" => Some(Self::NoBullet),
            "unstyled" => Some(Self::Unstyled),
            _ => None,
        }
    }
}

impl BlockImageAlignment {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" => Some(Self::Right),
            _ => None,
        }
    }

    const fn typst(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

pub(crate) struct PdfVisitor<'a, 'd, 'm> {
    pub(crate) writer: Writer,
    pub(crate) processor: Processor<'a>,
    pub(crate) anchors: Anchors<'a>,

    assets: &'m ImageMap,
    config: TypstSourceConfig<'m>,
    page_layout: PageLayout,
    diagnostics: Diagnostics<'d>,
    pub(crate) heading: Heading,
    code_wrap_columns: usize,
    palette: &'m Palette,
    table_theme: &'m TableTheme,
    image_radius_pt: f64,
    page_width_pt: f64,
    pub(crate) chapter_signifier: Option<String>,
    pub(crate) list_depth: usize,
    pub(crate) unordered_list_depth: usize,
    pub(crate) in_inline_span: bool,
    pre_wrap_depth: usize,
    text_whitespace: TextWhitespace,
    pub(crate) in_article_abstract: bool,
    pub(crate) automatic_preamble_lead_state: AutomaticPreambleLeadState,
    table_cell_section_state: TableCellSectionState,
    pub(crate) doctype: Doctype,
    book_page_break_state: BookPageBreakState,
    pub(crate) explicit_page_break_state: ExplicitPageBreakState,
    text_boundaries: TextBoundaries,
    document: DocumentRef<'a>,
    toc_written: bool,
    pub(crate) page_numbering: PageNumberingState,
    populated_index_sections: HashSet<String>,
    bibliography_backlinks_written: HashSet<String>,
    unsupported_metadata_warnings: HashSet<&'static str>,
    ordered_unstyled_scope_depth: usize,
    unordered_style_scope_depth: usize,
    static_media_warning: StaticMediaWarningState,
    index_catalog: IndexCatalog,
    index_columns: usize,
    index_column_gap_pt: f64,
}

pub(crate) struct PageNumberingState {
    pub(crate) plan: PageNumberingPlan,
    arabic_started: bool,
}

impl PageNumberingState {
    fn new(plan: PageNumberingPlan) -> Self {
        Self {
            plan,
            arabic_started: plan.conditional_arabic_start().is_some(),
        }
    }

    pub(crate) fn write_initial(&self, writer: &mut Writer) {
        if let Some(start) = self.plan.conditional_arabic_start() {
            let start = start.get();
            let _ = writeln!(writer, "#let _acdc_arabic_page_start = {start}");
            let _ = writeln!(
                writer,
                "#set page(numbering: (..nums) => {{\n  let n = nums.pos().first()\n  if n < {start} {{ numbering(\"i\", n) }} else {{ numbering(\"1\", n - {start} + 1) }}\n}})"
            );
        } else {
            writer.raw("#let _acdc_arabic_page_start = none\n#set page(numbering: \"i\")\n");
        }
    }

    pub(crate) fn start_arabic(&mut self, writer: &mut Writer) {
        if self.arabic_started {
            return;
        }
        writer.raw("#set page(numbering: \"1\")\n#counter(page).update(1)\n");
        self.arabic_started = true;
    }
}

#[derive(PartialEq, Eq)]
enum BookPageBreakState {
    Disabled,
    Enabled,
    AfterPart,
}

#[derive(Default, PartialEq, Eq)]
pub(crate) enum ExplicitPageBreakState {
    #[default]
    Inactive,
    Weak,
}

#[derive(Default, PartialEq, Eq)]
pub(crate) enum AutomaticPreambleLeadState {
    #[default]
    Inactive,
    Pending,
}

#[derive(Default, PartialEq, Eq)]
enum StaticMediaWarningState {
    #[default]
    Pending,
    Emitted,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TocRoot {
    None,
    Part,
    Special,
}

#[derive(Clone, Copy)]
enum ParagraphAlignment {
    Left,
    Center,
    Right,
    Justify,
}

struct TableRenderContext<'doc> {
    columns: Vec<ColumnFormatRef<'doc>>,
    presentation: TablePresentation,
    column_count: usize,
    row_count: usize,
    header_rows: usize,
    column_width_columns: Vec<usize>,
}

#[derive(Clone, Copy)]
struct TableCellPosition {
    x: usize,
    y: usize,
    is_header: bool,
    is_footer: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct InlineSpanState {
    wrappers: usize,
    pre_wrap: bool,
}

impl ParagraphAlignment {
    fn from_attributes(attributes: Option<AttributesRef<'_>>) -> Option<Self> {
        attribute_tokens(attributes, "role")
            .filter_map(|role| match role {
                "text-left" => Some(Self::Left),
                "text-center" => Some(Self::Center),
                "text-right" => Some(Self::Right),
                "text-justify" => Some(Self::Justify),
                _ => None,
            })
            .last()
    }

    const fn typst_prefix(self) -> &'static str {
        match self {
            Self::Left => "#align(left)[\n",
            Self::Center => "#align(center)[\n",
            Self::Right => "#align(right)[\n",
            Self::Justify => "#par(justify: true)[\n",
        }
    }
}

impl<'a, 'd, 'm> PdfVisitor<'a, 'd, 'm> {
    pub(crate) fn new(
        processor: Processor<'a>,
        assets: &'m ImageMap,
        config: TypstSourceConfig<'m>,
        document: DocumentRef<'a>,

        diagnostics: Diagnostics<'d>,
    ) -> Self {
        let theme = config.theme;
        let emit_options = config.emit_options;
        let page_width_pt = emit_options.page.width_points(emit_options.page_layout);
        let code_wrap_columns = code_wrap_columns(
            theme,
            emit_options.page,
            emit_options.page_layout,
            emit_options.page_margin,
        );
        let doctype = processor
            .document_attributes()
            .get("doctype")
            .and_then(|value| {
                if let Some(value) = value.as_str() {
                    value.parse().ok()
                } else {
                    None
                }
            })
            .unwrap_or_else(|| processor.options().doctype());
        let is_book = doctype == Doctype::Book;
        let chapter_signifier = if is_book {
            match processor
                .document_attributes()
                .get("chapter-signifier")
                .and_then(|value| value.text())
            {
                Some(signifier) if !signifier.is_empty() => Some(signifier.to_string()),
                Some(_) => None,
                None if processor
                    .document_attributes()
                    .is_explicit("chapter-signifier") =>
                {
                    None
                }
                None => Some("Chapter".to_string()),
            }
        } else {
            None
        };
        let book_page_break_state = if is_book {
            BookPageBreakState::Enabled
        } else {
            BookPageBreakState::Disabled
        };
        let toc_entries = document
            .toc_entries()
            .map(TocEntry::from)
            .collect::<Vec<_>>();
        Self {
            writer: Writer::new(),
            anchors: Anchors::new(&toc_entries, &processor.references),
            processor,

            assets,
            config,
            page_layout: emit_options.page_layout,
            diagnostics,
            heading: theme.heading,
            code_wrap_columns,
            palette: &theme.palette,
            table_theme: &theme.table,
            image_radius_pt: theme.spacing.image_radius_pt,
            page_width_pt,
            chapter_signifier,
            list_depth: 0,
            unordered_list_depth: 0,
            in_inline_span: false,
            pre_wrap_depth: 0,
            text_whitespace: TextWhitespace::Collapse,
            in_article_abstract: false,
            automatic_preamble_lead_state: AutomaticPreambleLeadState::Inactive,
            table_cell_section_state: TableCellSectionState::Outside,
            doctype,
            book_page_break_state,
            explicit_page_break_state: ExplicitPageBreakState::Inactive,
            text_boundaries: TextBoundaries::BOTH,
            document,
            toc_written: false,
            page_numbering: PageNumberingState::new(config.page_numbering),
            populated_index_sections: HashSet::new(),
            bibliography_backlinks_written: HashSet::new(),
            unsupported_metadata_warnings: HashSet::new(),
            ordered_unstyled_scope_depth: 0,
            unordered_style_scope_depth: 0,
            static_media_warning: StaticMediaWarningState::Pending,
            index_catalog: IndexCatalog::default(),
            index_columns: theme.index.columns,
            index_column_gap_pt: theme
                .index
                .column_gap_pt
                .unwrap_or(theme.typography.body_size_pt),
        }
    }

    pub(crate) fn with_populated_index_sections(mut self, sections: HashSet<String>) -> Self {
        self.populated_index_sections = sections;
        self
    }

    pub(crate) fn index_section_is_populated(&self, id: &str) -> bool {
        self.populated_index_sections.contains(id)
    }

    pub(crate) fn write_index_catalog(&mut self, traversal: &mut TraversalContext<'a>) {
        let sequence_style = PageSequenceStyle::from_attributes(
            document_attribute_text((traversal).get("index-pagenum-sequence-style")),
            document_attribute_text((traversal).get("media")),
        );
        self.index_catalog.write(
            &mut self.writer,
            sequence_style,
            self.index_columns,
            self.index_column_gap_pt,
        );
    }

    pub(crate) fn section_break_before(&mut self, section: SectionRef<'a>) -> bool {
        if self.book_page_break_state == BookPageBreakState::Disabled {
            return false;
        }

        if section.level() == 0 && section.kind() == SectionKind::Normal {
            self.book_page_break_state = BookPageBreakState::AfterPart;
            return self.heading.part.break_before == PageBreakBefore::Always;
        }

        if effective_section_level(section.level(), section.kind()) == 1 {
            let first_chapter_of_part = self.book_page_break_state == BookPageBreakState::AfterPart;
            self.book_page_break_state = BookPageBreakState::Enabled;
            return if first_chapter_of_part {
                match self.heading.part.break_after {
                    PartBreakAfter::Always => true,
                    PartBreakAfter::Avoid => false,
                    PartBreakAfter::Auto => {
                        self.heading.chapter.break_before == PageBreakBefore::Always
                    }
                }
            } else {
                self.heading.chapter.break_before == PageBreakBefore::Always
            };
        }

        if section.level() == 0 {
            self.book_page_break_state = BookPageBreakState::Enabled;
        }
        false
    }

    pub(crate) fn render_toc(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        toc_macro: Option<TableOfContentsRef<'_>>,
        placement: &str,
    ) -> Result<(), Error> {
        if self.toc_written || self.document.toc_entries().next().is_none() {
            return Ok(());
        }

        let config = TocConfig::resolve(
            toc_macro,
            &TraversalContext::new(self.processor.document_attributes()),
        );
        let configured_placement =
            if config.placement() == "none" && self.processor.pdf_options().toc {
                "auto"
            } else {
                config.placement()
            };
        let should_render = placement == configured_placement
            || placement == "auto"
                && matches!(configured_placement, "left" | "right" | "top" | "bottom");
        if !should_render {
            return Ok(());
        }

        if placement == "auto" && self.page_numbering.plan.starts_before_toc() {
            self.page_numbering.start_arabic(&mut self.writer);
        }

        self.toc_written = true;
        let suppress_header = toc_macro.is_some_and(|toc| toc.options().contains("noheader"));
        let suppress_footer = toc_macro.is_some_and(|toc| toc.options().contains("nofooter"));
        match (suppress_header, suppress_footer) {
            (true, true) => self.writer.raw("#page(header: none, footer: none)[\n"),
            (true, false) => self.writer.raw("#page(header: none)[\n"),
            (false, true) => self.writer.raw("#page(footer: none)[\n"),
            (false, false) => {}
        }
        if let Some(title) = config.title().filter(|title| !title.is_empty()) {
            self.writer
                .raw("#heading(outlined: false, bookmarked: false)[");
            self.write_text_expr(title);
            self.writer.raw("]\n");
        }

        self.writer.raw(
            "#let _acdc_toc_entry(target, depth, body) = context {\n  link(\n    target,\n    pad(\n      left: depth * 1.25em,\n      grid(\n        columns: (auto, 1fr, auto),\n        column-gutter: 0.5em,\n        body,\n        repeat[.],\n        counter(page).display(at: target),\n      ),\n    ),\n  )\n}\n",
        );

        let document = self.document;
        let numbers = number_prefixes(
            document.toc_entries(),
            &NumberingConfig::new(traversal, None, None),
        );
        let has_parts = has_parts(document.toc_entries());
        let mut root = TocRoot::None;
        let mut hidden_article_abstract_level = None;
        for ((entry, number), label) in document
            .toc_entries()
            .zip(numbers)
            .zip(self.anchors.toc_labels.clone())
        {
            if let Some(abstract_level) = hidden_article_abstract_level {
                if entry.level() > abstract_level {
                    continue;
                }
                hidden_article_abstract_level = None;
            }
            if self.doctype == Doctype::Article && entry.kind() == SectionKind::Abstract {
                hidden_article_abstract_level = Some(entry.level());
                continue;
            }
            if entry.kind() == SectionKind::Index
                && !self.populated_index_sections.contains(entry.id())
            {
                continue;
            }
            if entry.level() > config.levels() {
                continue;
            }
            let depth = match entry.level() {
                0 => {
                    root = if entry.kind() == SectionKind::Normal {
                        TocRoot::Part
                    } else {
                        TocRoot::Special
                    };
                    0
                }
                _ if !has_parts || root != TocRoot::Part => {
                    entry_level(entry, has_parts).saturating_sub(1)
                }
                level => level,
            };
            let _ = write!(self.writer, "#_acdc_toc_entry(<{label}>, {depth}, [");
            if let Some(number) = number {
                self.write_text_expr(&number);
            }
            self.write_title_copy(traversal, entry.title())?;
            self.writer.raw("])\n");
        }
        self.writer.raw("#pagebreak()\n\n");
        if suppress_header || suppress_footer {
            self.writer.raw("]\n\n");
        }
        if placement == "auto" && self.page_numbering.plan.starts_after_toc() {
            self.page_numbering.start_arabic(&mut self.writer);
        }
        Ok(())
    }

    pub(crate) fn write_blocks(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        blocks: impl Iterator<Item = BlockRef<'a>>,
    ) -> Result<(), Error> {
        for block in blocks {
            traversal.visit_block(self, block)?;
        }
        Ok(())
    }

    pub(crate) fn write_delimited_block_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
    ) -> Result<(), Error> {
        let fallback = CaptionKind::for_block(block);
        let attributes = block.attributes();
        let collapsible_example = is_collapsible_example(attributes, fallback);
        let table = matches!(block.kind(), BlockKind::Table(_));
        if table && self.omit_zero_width_table(attributes) {
            return Ok(());
        }
        let intrinsic_table =
            table && block.title().is_some() && Self::table_has_intrinsic_width(attributes);
        if intrinsic_table {
            self.write_intrinsic_table_start();
        }
        let (sized_table, aligned_table) =
            self.write_table_wrappers_start(attributes, table && !intrinsic_table);
        let writes_own_title = matches!(block.kind(), BlockKind::Sidebar(_))
            || matches!(block.kind(), BlockKind::Open(_))
                && attribute_style(attributes) == Some("abstract")
            || collapsible_example;
        if !matches!(block.kind(), BlockKind::Passthrough(_))
            && !writes_own_title
            && !intrinsic_table
        {
            let sticky_title = table
                && !has_attribute_token(attributes, "options", "unbreakable")
                && block.title().is_some();
            if sticky_title {
                self.writer
                    .raw("#block(sticky: true, above: 0pt, below: 0pt)[\n");
            }
            self.write_block_heading(traversal, block.title(), attributes, fallback)?;
            if sticky_title {
                self.writer.raw("]\n");
            }
        }
        let result = self.write_delimited_body(traversal, block, collapsible_example);
        if intrinsic_table {
            self.write_intrinsic_table_end(traversal, block, fallback)?;
        }
        self.write_table_wrappers_end(sized_table, aligned_table);
        result
    }

    fn write_delimited_body(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
        collapsible: bool,
    ) -> Result<(), Error> {
        let attributes = block.attributes();
        match block.kind() {
            BlockKind::Example(example) if collapsible => {
                self.write_disclosure(traversal, block.title(), |visitor, traversal| {
                    visitor.write_blocks(traversal, example.blocks())
                })
            }
            BlockKind::Open(open) if collapsible => {
                self.write_disclosure(traversal, block.title(), |visitor, traversal| {
                    visitor.write_blocks(traversal, open.blocks())
                })
            }
            BlockKind::Example(example) => self.write_example(traversal, example.blocks()),
            BlockKind::Sidebar(sidebar) => {
                self.write_sidebar(traversal, block.title(), sidebar.blocks())
            }
            BlockKind::Open(open) if open.style() == Some("abstract") => self.write_abstract(
                traversal,
                block.title(),
                true,
                attributes,
                |visitor, traversal| visitor.write_blocks(traversal, open.blocks()),
            ),
            BlockKind::Open(open) => self.write_framed_blocks(traversal, None, None, open.blocks()),
            BlockKind::Quote(quote) => {
                self.write_quote_block(traversal, attributes, |visitor, traversal| {
                    visitor.write_blocks(traversal, quote.blocks())
                })
            }
            BlockKind::Verse(verse) => {
                self.write_verse_block(traversal, verse.content().inlines(), attributes)
            }
            BlockKind::Listing(listing) => {
                self.write_verbatim_block(traversal, listing.content().inlines(), attributes)
            }
            BlockKind::Literal(literal) => {
                self.write_verbatim_block(traversal, literal.content().inlines(), attributes)
            }
            BlockKind::Passthrough(pass) => {
                self.write_passthrough_block(pass.content());
                Ok(())
            }
            BlockKind::Table(table) => self.write_table(traversal, table, attributes),
            BlockKind::Stem(stem) => {
                let location = Location::from(
                    stem.attributes()
                        .location()
                        .unwrap_or_else(|| stem.location()),
                );
                self.write_stem_fallback(stem.text(), true, Some(&location));
                Ok(())
            }
            BlockKind::DelimitedComment(_) => Ok(()),
            BlockKind::TableOfContents(_)
            | BlockKind::Admonition(_)
            | BlockKind::DiscreteHeader(_)
            | BlockKind::DocumentAttribute(_)
            | BlockKind::ThematicBreak(_)
            | BlockKind::PageBreak(_)
            | BlockKind::UnorderedList(_)
            | BlockKind::OrderedList(_)
            | BlockKind::CalloutList(_)
            | BlockKind::DescriptionList(_)
            | BlockKind::Section(_)
            | BlockKind::Paragraph(_)
            | BlockKind::Image(_)
            | BlockKind::Audio(_)
            | BlockKind::Video(_)
            | BlockKind::Comment(_)
            | _ => {
                self.warn_unsupported_parser_variant(
                    "delimited block",
                    Some(&block.location().into()),
                );
                Ok(())
            }
        }
    }

    pub(crate) fn write_title(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
    ) -> Result<(), Error> {
        let Some(title) = title else {
            return Ok(());
        };
        self.write_content(traversal, title)
    }

    fn write_title_copy(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
    ) -> Result<(), Error> {
        let previous = self.index_catalog.set_suspended(true);
        let previous_anchors = replace(&mut self.anchors.suspended, true);
        let result = self.write_title(traversal, title);
        self.anchors.suspended = previous_anchors;
        self.index_catalog.set_suspended(previous);
        result
    }

    pub(crate) fn write_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        content: ContentRef<'_>,
    ) -> Result<(), Error> {
        self.write_inline_refs(traversal, content.inlines())
    }

    pub(crate) fn write_inline_refs<'inline>(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: impl DoubleEndedIterator<Item = InlineRef<'inline>> + ExactSizeIterator + Clone,
    ) -> Result<(), Error> {
        let previous_boundaries = self.text_boundaries;
        let siblings = nodes.clone();
        let result = (|| {
            let mut previous_plain_space = false;
            for (index, node) in nodes.enumerate() {
                let continuation = previous_plain_space;
                previous_plain_space = matches!(node.kind(), InlineKind::Plain(text)
                    if text.text().ends_with([' ', '\t', '\n', '\r']));
                let boundaries = if self.in_inline_span {
                    TextBoundaries::NONE
                } else {
                    previous_boundaries
                };
                self.text_boundaries = boundaries
                    .with_ordinary_replacements(false)
                    .for_inline_refs(
                        Some(node),
                        siblings.clone().take(index).rev(),
                        siblings.clone().skip(index + 1),
                    );
                if let InlineKind::Plain(plain) = node.kind()
                    && matches!(self.text_whitespace, TextWhitespace::Collapse)
                    && self.pre_wrap_depth == 0
                    && continuation
                {
                    let text = plain.text().trim_start_matches([' ', '\t', '\n', '\r']);
                    if !text.is_empty() {
                        self.write_plain(text);
                    }
                } else {
                    InlineVisitor::visit_inline_node(self, traversal, node)?;
                }
            }
            Ok(())
        })();
        self.text_boundaries = previous_boundaries;
        result
    }

    pub(crate) fn write_text_expr(&mut self, text: &str) {
        if let Some(max_columns) = self.table_cell_text_break_columns() {
            self.write_table_text_expr(text, max_columns, TableTextKind::Text);
        } else {
            self.write_text_or_raw_expr(text, TableTextKind::Text);
        }
    }

    pub(crate) fn write_inline_verbatim_nodes(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: ContentRef<'_>,
    ) -> Result<(), Error> {
        let mut code = self.inline_verbatim_text(traversal, nodes)?;
        let mut positions = code
            .links
            .iter()
            .flat_map(|span| [(span.range.start, 0), (span.range.end, 0)])
            .collect::<Vec<_>>();
        // Normalize the complete text so targets do not change whitespace at fragment boundaries.
        transform_code_text(&mut code.source, &mut positions, |text| {
            self.normalize_prose_whitespace(text)
        });
        // Compute word breaks before splitting links so annotations do not change table wrapping.
        let breaks = self
            .table_cell_text_break_columns()
            .map_or_else(Vec::new, |columns| {
                long_unbreakable_ranges(&code.source, columns)
            });
        let mut start = 0;
        for ([(position, _), (end, _)], span) in positions.as_chunks::<2>().0.iter().zip(code.links)
        {
            let label = span.anchor.as_deref().and_then(|id| self.anchors.claim(id));
            let has_content =
                span.replacement.is_some() || span.target.is_some() || !span.prefix.is_empty();
            if label.is_some() || has_content {
                let position = *position;
                if start < position {
                    self.write_inline_verbatim_fragment(&code.source, start..position, &breaks);
                }
                if let Some(label) = label {
                    let _ = write!(self.writer, "#metadata(none)<{label}>");
                }
                start = position;
                if position < *end && has_content {
                    self.writer.raw(&span.prefix);
                    if let Some(replacement) = span.replacement.as_ref() {
                        self.writer.raw(replacement);
                    } else if let Some(target) = span.target.as_ref() {
                        self.write_inline_verbatim_link(
                            target,
                            &code.source,
                            position..*end,
                            &breaks,
                        )?;
                    } else {
                        self.write_inline_verbatim_fragment(&code.source, position..*end, &breaks);
                    }
                    self.writer.raw(&span.suffix);
                    start = *end;
                }
            }
        }
        self.write_inline_verbatim_fragment(&code.source, start..code.source.len(), &breaks);
        Ok(())
    }

    fn write_inline_verbatim_link(
        &mut self,
        target: &CodeLinkTarget,
        source: &str,
        range: Range<usize>,
        breaks: &[Range<usize>],
    ) -> Result<(), Error> {
        let content = |visitor: &mut Self| {
            visitor.write_inline_verbatim_fragment(source, range, breaks);
            Ok(())
        };
        match target {
            CodeLinkTarget::External(target) => self.write_external_link(target, content),
            CodeLinkTarget::Internal(target) => self.write_labelled_link(target, content),
            CodeLinkTarget::DocumentTop => {
                self.writer.raw("#link((page: 1, x: 0pt, y: 0pt))[");
                content(self)?;
                self.writer.raw("]");
                Ok(())
            }
        }
    }

    fn write_inline_verbatim_fragment(
        &mut self,
        source: &str,
        range: Range<usize>,
        breaks: &[Range<usize>],
    ) {
        if breaks
            .iter()
            .any(|word| word.start < range.start && range.start < word.end)
        {
            // Restore the break between clusters when a word crosses an annotation boundary.
            self.writer.raw("#box(width: 0pt)");
        }
        let ranges = breaks.iter().filter_map(|word| {
            let start = word.start.max(range.start);
            let end = word.end.min(range.end);
            (start < end).then(|| start - range.start..end - range.start)
        });
        self.write_text_ranges(
            &source[range.start..range.end],
            ranges,
            TableTextKind::InlineVerbatim,
        );
    }

    fn inline_verbatim_text(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: ContentRef<'_>,
    ) -> Result<CodeText, Error> {
        let mut code = CodeText::default();
        let transform = InlineTextTransform::default()
            .rendered()
            .decode_char_refs(true);
        for node in nodes.inlines() {
            if let InlineKind::Anchor(anchor) = node.kind() {
                code.links.push(CodeSpan {
                    range: code.source.len()..code.source.len(),
                    anchor: Some(anchor.id().to_owned()),
                    ..CodeSpan::default()
                });
            } else if let InlineKind::Footnote(footnote) = node.kind() {
                self.collect_code_footnote(traversal, footnote, &mut code)?;
            } else if let Some(children) = code_inline_content(node) {
                let child = self.inline_verbatim_text(traversal, children)?;
                code.append(self.format_code_text(node, child));
            } else if let Some(link) = resolve_code_link(
                node,
                &self.processor.references,
                code_output_extension(traversal),
            ) {
                let mut child = if let Some(children) =
                    code_link_content(node).filter(|nodes| !nodes.is_empty())
                {
                    self.inline_verbatim_text(traversal, children)?
                } else {
                    CodeText {
                        source: link.text,
                        ..CodeText::default()
                    }
                };
                child.wrap_span(CodeSpan {
                    // A copied title belongs to its outer TOC or reference link.
                    target: if self.anchors.suspended {
                        None
                    } else {
                        link.target
                    },
                    anchor: link.anchor,
                    ..CodeSpan::default()
                });
                code.append(child);
            } else {
                let _ = transform.write_inlines(&mut code.source, std::iter::once(node));
            }
        }
        Ok(code)
    }

    fn collect_code_footnote(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        footnote: FootnoteRef<'_>,
        code: &mut CodeText,
    ) -> Result<(), Error> {
        let output = replace(&mut self.writer, Writer::new());
        let result = self.write_footnote(traversal, footnote);
        let markup = replace(&mut self.writer, output).into_string();
        result?;
        let start = code.source.len();
        // An indivisible placeholder keeps note-only spans and prevents wrapping inside a note.
        code.source.push('\u{fffc}');
        code.links.push(CodeSpan {
            range: start..code.source.len(),
            replacement: Some(markup),
            ..CodeSpan::default()
        });
        Ok(())
    }

    fn write_table_literal(&mut self, text: &str) {
        let max_columns = self.table_cell_text_break_columns().unwrap_or(usize::MAX);
        self.write_table_text_expr(text, max_columns, TableTextKind::Literal);
    }

    fn write_table_text_expr(&mut self, text: &str, max_columns: usize, kind: TableTextKind) {
        let ranges = long_unbreakable_ranges(text, max_columns);
        self.write_text_ranges(text, ranges, kind);
    }

    fn write_text_ranges(
        &mut self,
        text: &str,
        ranges: impl IntoIterator<Item = Range<usize>>,
        kind: TableTextKind,
    ) {
        let mut previous_end = 0;
        for range in ranges {
            if range.start > previous_end {
                self.write_text_or_raw_expr(&text[previous_end..range.start], kind);
            }
            self.writer.raw("#(");
            self.writer.string_literal(&text[range.clone()]);
            if matches!(kind, TableTextKind::InlineVerbatim | TableTextKind::Literal) {
                self.writer.raw(concat!(
                    ".clusters().map(value => raw(block: false, value))",
                    ".join(box(width: 0pt)))",
                ));
            } else {
                self.writer
                    .raw(".clusters().map(text).join(box(width: 0pt)))");
            }
            previous_end = range.end;
        }
        if previous_end < text.len() || text.is_empty() {
            self.write_text_or_raw_expr(&text[previous_end..], kind);
        }
    }

    fn write_text_or_raw_expr(&mut self, text: &str, kind: TableTextKind) {
        self.writer.raw(match kind {
            TableTextKind::Text => "#text(",
            TableTextKind::InlineVerbatim => "#raw(",
            TableTextKind::Literal => "#raw(block: false, ",
        });
        self.writer.string_literal(text);
        self.writer.raw(")");
    }

    pub(crate) fn write_block_anchor(&mut self, anchor: Option<AnchorRef<'_>>) {
        if let Some(anchor) = anchor {
            self.write_anchor_target(anchor);
        }
    }

    pub(crate) fn write_anchor_target(&mut self, anchor: AnchorRef<'_>) {
        if let Some(label) = self.anchors.claim(anchor.id()) {
            let _ = writeln!(self.writer, "#metadata(none) <{label}>");
        }
    }

    pub(crate) fn write_inline_anchor(&mut self, id: &str) {
        if let Some(label) = self.anchors.claim(id) {
            // Whitespace before the label becomes visible in inline content.
            let _ = write!(self.writer, "#metadata(none)<{label}>");
        }
    }

    pub(crate) fn write_inline_span_start(
        &mut self,
        id: Option<&str>,
        role: Option<&str>,
    ) -> InlineSpanState {
        if let Some(id) = id {
            self.write_inline_anchor(id);
        }

        let mut wrappers = 0;
        let mut pre_wrap = false;
        for role in role.into_iter().flat_map(str::split_whitespace) {
            let prefix = match role {
                "line-through" => Some("#strike["),
                "underline" => Some("#underline["),
                "overline" => Some("#overline["),
                "big" => Some("#text(size: 1.2em)["),
                "small" => Some("#text(size: 0.8em)["),
                "pre-wrap" => {
                    pre_wrap = true;
                    None
                }
                _ => None,
            };
            if let Some(prefix) = prefix {
                self.writer.raw(prefix);
                wrappers += 1;
            } else if let Some(colour) = asciidoctor_foreground_colour(role) {
                let _ = write!(self.writer, "#text(fill: rgb(\"{colour}\"))[");
                wrappers += 1;
            } else if let Some(colour) = asciidoctor_background_colour(role) {
                let _ = write!(self.writer, "#highlight(fill: rgb(\"{colour}\"))[");
                wrappers += 1;
            }
        }
        self.pre_wrap_depth += usize::from(pre_wrap);
        InlineSpanState { wrappers, pre_wrap }
    }

    pub(crate) fn write_inline_span_end(&mut self, state: InlineSpanState) {
        for _ in 0..state.wrappers {
            self.writer.raw("]");
        }
        self.pre_wrap_depth -= usize::from(state.pre_wrap);
    }

    pub(crate) fn normalize_prose_whitespace<'text>(&self, text: &'text str) -> Cow<'text, str> {
        if matches!(self.text_whitespace, TextWhitespace::Preserve) {
            Cow::Borrowed(text)
        } else {
            prose_whitespace(text, self.pre_wrap_depth > 0)
        }
    }

    pub(crate) fn write_plain(&mut self, text: &str) {
        let text = self.normalize_prose_whitespace(text);
        self.write_text_expr(&text);
    }

    pub(crate) fn write_raw(&mut self, raw: RawRef<'_>) {
        let mut text = raw.text().to_string();
        let mut applied_special_chars = false;
        for substitution in raw.substitutions() {
            match substitution {
                Substitution::SpecialChars => {
                    if applied_special_chars {
                        text = text
                            .replace('&', "&amp;")
                            .replace('>', "&gt;")
                            .replace('<', "&lt;");
                    }
                    applied_special_chars = true;
                }
                Substitution::Replacements => {
                    text = apply_passthrough_replacements_iter(
                        &text,
                        raw.substitutions(),
                        &Replacements::unicode(),
                        self.text_boundaries,
                    )
                    .into_owned();
                }
                Substitution::Attributes
                | Substitution::Macros
                | Substitution::PostReplacements
                | Substitution::Normal
                | Substitution::Verbatim
                | Substitution::Quotes
                | Substitution::Callouts
                | _ => {}
            }
        }

        let text = self.normalize_prose_whitespace(&text);
        if applied_special_chars {
            self.write_text_expr(&text);
        } else {
            self.write_text_expr(&acdc_converters_core::decode_numeric_char_refs(&text));
        }
    }

    pub(crate) fn write_quoted_span<'inline>(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        id: Option<ElementIdRef<'_>>,
        role: Option<&str>,
        prefix: &str,
        nodes: impl DoubleEndedIterator<Item = InlineRef<'inline>> + ExactSizeIterator + Clone,
        suffix: &str,
    ) -> Result<(), Error> {
        let state = self.write_inline_span_start(id.map(ElementIdRef::as_str), role);
        self.writer.raw(prefix);
        self.write_inline_refs(traversal, nodes)?;
        self.writer.raw(suffix);
        self.write_inline_span_end(state);
        Ok(())
    }

    /// Write a block's title above the block.
    ///
    /// The title is its own layout block, so the content that follows starts on
    /// a new line, and sits close to it rather than a paragraph apart.
    pub(crate) fn write_block_title(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
    ) -> Result<(), Error> {
        let Some(title) = title else {
            return Ok(());
        };
        self.writer.raw("#blocktitle[");
        self.write_title(traversal, Some(title))?;
        self.writer.raw("]\n");
        Ok(())
    }

    fn write_block_heading(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        attributes: Option<AttributesRef<'_>>,
        fallback: Option<CaptionKind>,
    ) -> Result<(), Error> {
        if self.resolved_caption(attributes, fallback).is_some() {
            self.write_captioned_title(traversal, title, attributes, fallback)
        } else {
            self.write_block_title(traversal, title)
        }
    }

    /// Write a collapsible example as an expanded disclosure for print.
    pub(crate) fn write_disclosure(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        write_body: impl FnOnce(&mut Self, &mut TraversalContext<'a>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.writer.raw("#block(width: 100%, below: 0.8em)[\n");
        self.writer
            .raw("#grid(columns: (0.8em, 1fr), column-gutter: 0.2em, align: top, ");
        self.writer
            .raw("[#box(width: 0.8em, height: 0.8em, baseline: 0.1em, align(center + horizon, ");
        self.writer
            .raw("rotate(90deg, origin: center, text(weight: \"bold\", size: 0.8em, \">\"))))], ");
        self.writer.raw("[#captiontext[");
        if let Some(title) = title {
            self.write_title(traversal, Some(title))?;
        } else {
            self.write_text_expr("Details");
        }
        self.writer
            .raw("]])\n#block(inset: (left: 1em), above: 0.3em)[\n");
        write_body(self, traversal)?;
        self.writer.raw("\n]\n]\n\n");
        Ok(())
    }

    /// Render a paragraph's selected title and body.
    ///
    /// `write_title` is false for the synthetic paragraph inside a simple
    /// admonition, whose title the admonition wrapper already wrote.
    pub(crate) fn write_paragraph_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
        write_title: bool,
        automatic_lead: bool,
    ) -> Result<(), Error> {
        if para.style() == Some("pass") {
            self.write_passthrough_block(para.content());
            return Ok(());
        }
        if para.style() == Some("sidebar") {
            return self.write_sidebar_content(traversal, para.title(), |visitor, traversal| {
                visitor.write_paragraph_alignment(Some(para.attributes()), |visitor| {
                    visitor.write_content(traversal, para.content())
                })
            });
        }

        let fallback = CaptionKind::for_style(para.style());
        if is_collapsible_example(Some(para.attributes()), fallback) {
            return self.write_disclosure(traversal, para.title(), |visitor, traversal| {
                visitor.write_paragraph_alignment(Some(para.attributes()), |visitor| {
                    visitor.write_content(traversal, para.content())
                })
            });
        }
        if para.style() == Some("abstract") {
            return self.write_abstract(
                traversal,
                para.title(),
                write_title,
                Some(para.attributes()),
                |visitor, traversal| visitor.write_content(traversal, para.content()),
            );
        }
        if write_title {
            self.write_block_heading(traversal, para.title(), Some(para.attributes()), fallback)?;
        }
        self.write_paragraph_body(traversal, para, automatic_lead)
    }

    pub(crate) fn write_abstract_title(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
    ) -> Result<(), Error> {
        let Some(title) = title else {
            return Ok(());
        };
        self.writer.raw("#abstracttitle[");
        self.write_title(traversal, Some(title))?;
        self.writer.raw("]");
        Ok(())
    }

    pub(crate) fn write_abstract(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        write_title: bool,
        attributes: Option<AttributesRef<'_>>,
        write_body: impl FnOnce(&mut Self, &mut TraversalContext<'a>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if write_title {
            self.write_abstract_title(traversal, title)?;
            self.writer.raw("\n");
        }
        self.writer.raw("#abstract[\n");
        self.write_paragraph_alignment(attributes, |visitor| write_body(visitor, traversal))?;
        self.writer.raw("\n]\n\n");
        Ok(())
    }

    /// Write the attribution line a quote or verse carries, if any.
    ///
    /// The cited work is only displayed when an author is present, matching
    /// asciidoctor-pdf. Both values keep their inline formatting.
    pub(crate) fn write_attribution(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        attributes: Option<AttributesRef<'_>>,
    ) -> Result<(), Error> {
        let Some(attribution) = attribute_content(attributes, "attribution") else {
            return Ok(());
        };
        let citetitle = attribute_content(attributes, "citetitle");
        self.writer.raw("#attribution[");
        self.write_content(traversal, attribution)?;
        if let Some(citetitle) = citetitle {
            self.write_text_expr(", ");
            self.write_content(traversal, citetitle)?;
        }
        self.writer.raw("]\n\n");
        Ok(())
    }

    fn write_paragraph_alignment(
        &mut self,
        attributes: Option<AttributesRef<'_>>,
        write_body: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let alignment = ParagraphAlignment::from_attributes(attributes);
        if let Some(alignment) = alignment {
            self.writer.raw(alignment.typst_prefix());
        }
        write_body(self)?;
        if alignment.is_some() {
            self.writer.raw("\n]");
        }
        Ok(())
    }

    fn write_aligned_paragraph_body(
        &mut self,
        attributes: Option<AttributesRef<'_>>,
        write_body: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.write_paragraph_alignment(attributes, write_body)?;
        self.writer.raw("\n\n");
        Ok(())
    }

    fn write_paragraph_roles_start(
        &mut self,
        attributes: Option<AttributesRef<'_>>,
        automatic_lead: bool,
    ) -> usize {
        let mut wrappers = 0;
        if automatic_lead {
            self.writer.raw("#text(size: 1.25em)[");
            wrappers += 1;
        }
        for role in attribute_tokens(attributes, "role") {
            let prefix = match role {
                "lead" => Some("#text(size: 1.25em)["),
                "big" => Some("#text(size: 1.2em)["),
                "small" => Some("#text(size: 0.8em)["),
                "subtitle" => {
                    Some("#text(size: 0.8em, style: \"italic\", fill: rgb(\"#999999\"))[")
                }
                "underline" => Some("#underline["),
                "line-through" => Some("#strike["),
                _ => None,
            };
            if let Some(prefix) = prefix {
                self.writer.raw(prefix);
                wrappers += 1;
            }
        }
        wrappers
    }

    fn write_paragraph_roles_end(&mut self, wrappers: usize) {
        for _ in 0..wrappers {
            self.writer.raw("]");
        }
    }

    pub(crate) fn write_quote_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        attributes: Option<AttributesRef<'_>>,
        write_body: impl FnOnce(&mut Self, &mut TraversalContext<'a>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.writer.raw("#blockquote[\n");
        write_body(self, traversal)?;
        if attribute_content(attributes, "attribution").is_some() {
            self.writer.raw("\n#text(style: \"normal\")[\n");
            self.write_attribution(traversal, attributes)?;
            self.writer.raw("]\n");
        }
        self.writer.raw("]\n\n");
        Ok(())
    }

    fn write_verse_content<'inline>(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: impl DoubleEndedIterator<Item = InlineRef<'inline>> + ExactSizeIterator + Clone,
    ) -> Result<(), Error> {
        self.writer.raw("#verse[");
        // Verse has normal inline semantics, but its authored whitespace is layout.
        let previous = replace(&mut self.text_whitespace, TextWhitespace::Preserve);
        let result = self.write_inline_refs(traversal, nodes);
        self.text_whitespace = previous;
        self.writer.raw("]");
        result
    }

    /// Write verse content, which keeps its line breaks and stays proportional.
    pub(crate) fn write_verse_block<'inline>(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: impl DoubleEndedIterator<Item = InlineRef<'inline>> + ExactSizeIterator + Clone,
        attributes: Option<AttributesRef<'_>>,
    ) -> Result<(), Error> {
        self.write_verse_content(traversal, nodes)?;
        self.writer.raw("\n\n");
        self.write_attribution(traversal, attributes)
    }

    /// Write a paragraph's content in the shape its style asks for.
    ///
    /// A `[quote]`, `[verse]`, `[literal]`, `[listing]`, `[source]`, or
    /// `[example]` paragraph reads as its delimited counterpart, matching
    /// `asciidoctor`.
    fn write_paragraph_body(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
        automatic_lead: bool,
    ) -> Result<(), Error> {
        match para.style() {
            Some("quote") => {
                self.write_quote_block(traversal, Some(para.attributes()), |visitor, traversal| {
                    visitor.write_paragraph_alignment(Some(para.attributes()), |visitor| {
                        visitor.write_content(traversal, para.content())
                    })
                })
            }
            Some("verse") => {
                self.write_aligned_paragraph_body(Some(para.attributes()), |visitor| {
                    visitor.write_verse_content(traversal, para.content().inlines())
                })?;
                self.write_attribution(traversal, Some(para.attributes()))
            }
            Some("literal" | "listing" | "source") => self.write_verbatim_block(
                traversal,
                para.content().inlines(),
                Some(para.attributes()),
            ),
            Some("example") => {
                self.write_aligned_paragraph_body(Some(para.attributes()), |visitor| {
                    visitor.writer.raw("#examplebox[\n");
                    visitor.write_content(traversal, para.content())?;
                    visitor.writer.raw("\n]");
                    Ok(())
                })
            }
            _ => {
                let wrappers =
                    self.write_paragraph_roles_start(Some(para.attributes()), automatic_lead);
                self.write_paragraph_alignment(Some(para.attributes()), |visitor| {
                    visitor.write_content(traversal, para.content())
                })?;
                self.write_paragraph_roles_end(wrappers);
                self.writer.raw("\n\n");
                Ok(())
            }
        }
    }

    /// Write a title as its block's caption, e.g. `Example 2. Title`.
    ///
    /// A caption with no prefix — an unset or disabled label — still takes the caption style,
    /// matching asciidoctor's captioned title.
    pub(crate) fn write_captioned_title(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        attributes: Option<AttributesRef<'_>>,
        fallback: Option<CaptionKind>,
    ) -> Result<(), Error> {
        self.write_captioned_title_with(traversal, "blocktitle", title, attributes, fallback)
    }

    fn write_captioned_title_with(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        wrapper: &str,
        title: Option<ContentRef<'_>>,
        attributes: Option<AttributesRef<'_>>,
        fallback: Option<CaptionKind>,
    ) -> Result<(), Error> {
        let Some(title) = title else {
            return Ok(());
        };
        let _ = write!(self.writer, "#{wrapper}[");
        if let Some(content) = attribute_content(attributes, "caption") {
            self.write_content(traversal, content)?;
        } else if let Some(prefix) = self.caption_prefix(attributes, fallback) {
            self.write_text_expr(&prefix);
        }
        self.write_title(traversal, Some(title))?;
        self.writer.raw("]\n");
        Ok(())
    }

    /// Format numbered and plain-text caption prefixes.
    ///
    /// The parser resolves the label from the document attributes in effect at the block's
    /// source position and assigns the ordinal, so a caption change part-way through a document
    /// applies from that point on, and nested blocks number inner-first.
    fn caption_prefix(
        &self,
        attributes: Option<AttributesRef<'_>>,
        fallback: Option<CaptionKind>,
    ) -> Option<String> {
        let resolved = self.resolved_caption(attributes, fallback)?;
        match resolved {
            CaptionRef::Numbered {
                label,
                number,
                kind,
            } => Some(Self::numbered_caption_prefix(
                label,
                number.map_or_else(|| self.next_caption_number(kind), NonZeroU32::get),
            )),
            CaptionRef::Custom(prefix) => Some(prefix.to_owned()),
            CaptionRef::CustomContent(_) | CaptionRef::Unnumbered | _ => None,
        }
    }

    fn resolved_caption<'doc>(
        &'doc self,
        attributes: Option<AttributesRef<'doc>>,
        fallback: Option<CaptionKind>,
    ) -> Option<CaptionRef<'doc>> {
        CaptionRef::resolve(
            attributes?,
            self.processor.document_attributes().into(),
            fallback,
        )
    }

    fn next_caption_number(&self, kind: CaptionKind) -> u32 {
        let counter = match kind {
            CaptionKind::Figure => &self.processor.figure_counter,
            CaptionKind::Listing => &self.processor.listing_counter,
            CaptionKind::Table => &self.processor.table_counter,
            CaptionKind::Example | _ => &self.processor.example_counter,
        };
        let number = counter.get() + 1;
        counter.set(number);
        number
    }

    /// Asciidoctor formats a caption as `"{label} {number}. "`, so a label set with no value
    /// leaves a leading space.
    fn numbered_caption_prefix(label: &str, number: u32) -> String {
        format!("{label} {number}. ")
    }

    /// Write an example block inside its light frame. Its title is written by the caller, as
    /// every captioned block's is.
    pub(crate) fn write_example(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        blocks: impl Iterator<Item = BlockRef<'a>>,
    ) -> Result<(), Error> {
        self.write_framed_blocks(traversal, Some("examplebox"), None, blocks)
    }

    /// Write a sidebar, which centers its title inside the shaded box rather
    /// than setting it above, matching Asciidoctor PDF.
    pub(crate) fn write_sidebar(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        blocks: impl Iterator<Item = BlockRef<'a>>,
    ) -> Result<(), Error> {
        self.write_sidebar_content(traversal, title, |visitor, traversal| {
            visitor.write_blocks(traversal, blocks)
        })
    }

    fn write_sidebar_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        body: impl FnOnce(&mut Self, &mut TraversalContext<'a>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.writer.raw("#sidebarbox[\n");
        if let Some(title) = title {
            self.writer.raw("#sidebartitle[");
            self.write_title(traversal, Some(title))?;
            self.writer.raw("]\n");
        }
        body(self, traversal)?;
        self.writer.raw("]\n\n");
        Ok(())
    }

    pub(crate) fn write_verbatim_block<'inline>(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: impl DoubleEndedIterator<Item = InlineRef<'inline>> + ExactSizeIterator + Clone,
        attributes: Option<AttributesRef<'_>>,
    ) -> Result<(), Error> {
        if has_attribute_token(attributes, "options", "mixed")
            && Self::source_language(traversal, attributes) == Some("php")
        {
            self.warn_unsupported_once(
                "php-mixed-highlighting",
                "PHP source block mixed-mode highlighting is not supported by the PDF backend; rendering with Typst's normal PHP highlighter",
                "Use the `html+php` source language when it gives acceptable highlighting, or use Asciidoctor PDF for explicit `%mixed` highlighting.",
                attributes.and_then(AttributesRef::location).map(Location::from).as_ref(),
            );
        }
        let tab_size = Self::code_tab_size(traversal, attributes);
        let CodeText {
            source,
            indexes: index_positions,
            links,
            ..
        } = self.code_text(traversal, nodes, attributes, tab_size)?;
        let autofit = has_autofit_option(attribute_tokens(attributes, "options"), traversal);
        let options = if Self::source_highlighting_enabled(traversal) {
            attributes.map_or_else(SourceLineOptions::default, |attributes| {
                SourceLineOptions::from_attributes(attributes, &source)
            })
        } else {
            SourceLineOptions::default()
        };
        let annotations = CodeAnnotations {
            anchors: self.code_index_anchor_lines(&source, index_positions, autofit, tab_size),
            links: self.code_link_lines(&source, &links, autofit, tab_size),
        };
        if autofit {
            if options.is_empty() {
                self.write_autofit_open(traversal, &source, attributes, 0);
                self.write_raw_block(traversal, &source, attributes, &annotations);
                self.writer.raw("]\n\n");
                return Ok(());
            }

            let source_lines = (1..=source_line_count(&source)).collect::<Vec<_>>();
            if source_lines.is_empty() {
                self.write_autofit_open(traversal, &source, attributes, 0);
                self.write_raw_block(traversal, &source, attributes, &annotations);
                self.writer.raw("]\n\n");
                return Ok(());
            }
            let extra_width_tenths = if options.line_number_start.is_some() {
                source_gutter_tenths(&source, &options).saturating_add(8)
            } else {
                0
            };
            self.write_autofit_open(traversal, &source, attributes, extra_width_tenths);
            self.write_source_block_with_line_options(
                traversal,
                &source,
                &source_lines,
                attributes,
                &options,
                &annotations,
            );
            self.writer.raw("]\n\n");
            return Ok(());
        }

        let wrap_columns = self
            .table_cell_code_wrap_columns()
            .unwrap_or(self.code_wrap_columns);
        if options.is_empty() {
            let source = wrap_code_text(&source, wrap_columns, tab_size);
            self.write_raw_block(traversal, &source, attributes, &annotations);
            return Ok(());
        }

        let (source, source_lines) =
            wrap_code_text_with_line_origins(&source, wrap_columns, tab_size);
        if source_lines.is_empty() {
            self.write_raw_block(traversal, &source, attributes, &annotations);
            return Ok(());
        }
        self.write_source_block_with_line_options(
            traversal,
            &source,
            &source_lines,
            attributes,
            &options,
            &annotations,
        );
        Ok(())
    }

    fn write_autofit_open(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        source: &str,
        attributes: Option<AttributesRef<'_>>,
        extra_width_tenths: usize,
    ) {
        self.writer.raw("#_acdc_autofit_code(");
        self.writer.string_literal(source);
        if let Some(language) = Self::source_language(traversal, attributes) {
            self.writer.raw(", language: ");
            self.writer.string_literal(language);
        }
        if extra_width_tenths > 0 {
            let _ = write!(
                self.writer,
                ", extra-width: {}.{}em",
                extra_width_tenths / 10,
                extra_width_tenths % 10
            );
        }
        self.writer.raw(")[\n");
    }

    fn write_raw_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        source: &str,
        attributes: Option<AttributesRef<'_>>,
        annotations: &CodeAnnotations,
    ) {
        if !annotations.is_empty() {
            self.writer.raw("#{\n");
            self.write_code_annotations(annotations);
            self.writer.raw("  show raw.line: line => ");
            if !annotations.anchors.is_empty() {
                self.writer
                    .raw("index-anchors.at(line.number - 1, default: []) + ");
            }
            self.writer.raw(if annotations.links.is_empty() {
                "line\n  "
            } else {
                "code-links(line)\n  "
            });
        }
        self.writer.raw(if annotations.is_empty() {
            "#raw(block: true"
        } else {
            "raw(block: true"
        });
        if let Some(language) = Self::source_language(traversal, attributes) {
            self.writer.raw(", lang: ");
            self.writer.string_literal(language);
        }
        self.writer.raw(", ");
        self.writer.string_literal(source);
        self.writer.raw(")\n");
        self.writer
            .raw(if annotations.is_empty() { "\n" } else { "}\n" });
    }

    fn write_source_block_with_line_options(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        source: &str,
        source_lines: &[usize],
        attributes: Option<AttributesRef<'_>>,
        options: &SourceLineOptions,
        annotations: &CodeAnnotations,
    ) {
        self.writer.raw("#{\n");
        if !annotations.is_empty() {
            self.write_code_annotations(annotations);
        }
        self.writer.raw("  let numbers = ");
        self.write_source_line_numbers(source_lines, options.line_number_start);
        self.writer.raw("\n  let highlighted = (");
        for source_line in source_lines {
            let highlighted = options.highlighted_lines.binary_search(source_line).is_ok();
            let _ = write!(self.writer, "{highlighted}, ");
        }
        self.writer.raw(")\n");

        let gutter_tenths = source_gutter_tenths(source, options);
        let _ = writeln!(
            self.writer,
            "  let gutter = {}.{}em",
            gutter_tenths / 10,
            gutter_tenths % 10
        );
        self.writer.raw("  show raw.line: line => {\n");
        self.writer.raw("    let index = line.number - 1\n");
        self.writer
            .raw("    let marked = highlighted.at(index, default: false)\n");
        self.writer.raw(
            "    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }\n",
        );
        self.writer.raw("    let code = ");
        if !annotations.anchors.is_empty() {
            self.writer.raw("index-anchors.at(index, default: []) + ");
        }
        self.writer
            .raw("box(width: code-width, fill: if marked { rgb(");
        self.writer.string_literal(&self.palette.accent);
        self.writer.raw(") } else { none }, ");
        self.writer.raw(if annotations.links.is_empty() {
            "line.body)\n"
        } else {
            "code-links(line))\n"
        });
        self.writer
            .raw("    if numbers == none {\n      code\n    } else {\n");
        self.writer
            .raw("      let number = numbers.at(index, default: none)\n");
        self.writer.raw(
            "      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb(",
        );
        self.writer.string_literal(&self.palette.counter);
        self.writer
            .raw("), str(number)) })) + h(0.8em) + code\n    }\n  }\n  raw(block: true");
        if let Some(language) = Self::source_language(traversal, attributes) {
            self.writer.raw(", lang: ");
            self.writer.string_literal(language);
        }
        self.writer.raw(", ");
        self.writer.string_literal(source);
        self.writer.raw(")\n}\n\n");
    }

    fn write_code_index_anchors(&mut self, lines: &[Vec<usize>]) {
        self.writer.raw("  let index-anchors = (\n");
        for anchors in lines {
            self.writer.raw("    [");
            for anchor in anchors {
                let _ = write!(self.writer, "#metadata(none)<__indexterm-{anchor}>");
            }
            self.writer.raw("],\n");
        }
        self.writer.raw("  )\n");
    }

    fn write_source_line_numbers(&mut self, source_lines: &[usize], start: Option<usize>) {
        let Some(start) = start else {
            self.writer.raw("none");
            return;
        };

        self.writer.raw("(");
        let mut previous_source_line = None;
        for source_line in source_lines {
            if previous_source_line == Some(*source_line) {
                self.writer.raw("none, ");
            } else {
                let number = start.saturating_add(source_line.saturating_sub(1));
                let _ = write!(self.writer, "{number}, ");
                previous_source_line = Some(*source_line);
            }
        }
        self.writer.raw(")");
    }

    fn code_tab_size(
        traversal: &TraversalContext<'a>,
        attributes: Option<AttributesRef<'_>>,
    ) -> usize {
        attributes
            .and_then(|attributes| attribute_text(attributes, "tabsize"))
            .and_then(|value| value.parse().ok())
            .or_else(|| {
                traversal.get("tabsize").and_then(|value| {
                    if let Some(value) = value.as_str() {
                        value.parse().ok()
                    } else if let Some(value) = value.as_integer() {
                        usize::try_from(value).ok()
                    } else {
                        None
                    }
                })
            })
            .filter(|size| *size > 0)
            .unwrap_or(4)
    }

    fn source_language<'doc>(
        traversal: &TraversalContext<'a>,
        attributes: Option<AttributesRef<'doc>>,
    ) -> Option<&'doc str> {
        Self::source_highlighting_enabled(traversal)
            .then(|| attributes.and_then(acdc_converters_core::code::source_language))
            .flatten()
            .filter(|language| acdc_pdf_render::supports_raw_language(language))
    }

    fn source_highlighting_enabled(traversal: &TraversalContext<'a>) -> bool {
        traversal.get("source-highlighter").is_some()
    }

    /// Write passthrough content as escaped, unframed monospace text.
    pub(crate) fn write_passthrough_block(&mut self, content: ContentRef<'_>) {
        self.writer.raw("#block(width: 100%)[#raw(block: false, ");
        self.write_verbatim_string(content);
        self.writer.raw(")]\n\n");
    }

    fn write_verbatim_string(&mut self, content: ContentRef<'_>) {
        let text = InlineTextTransform::default()
            .decode_char_refs(true)
            .line_break("\n")
            .rendered()
            .content_to_string(content);
        self.writer.string_literal(&text);
    }

    fn code_text<'inline>(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: impl DoubleEndedIterator<Item = InlineRef<'inline>> + ExactSizeIterator + Clone,
        attributes: Option<AttributesRef<'_>>,
        tab_size: usize,
    ) -> Result<CodeText, Error> {
        let extension = code_output_extension(traversal).to_owned();
        let line_comment =
            attributes.and_then(|attributes| attribute_text(attributes, "line-comment"));
        let CodeText {
            source: mut text,
            indexes: mut positions,
            mut links,
        } = self.collect_code_text(
            traversal,
            nodes,
            &extension,
            line_comment,
            false,
            TextBoundaries::BOTH,
        )?;
        // Keep a final anchor-only line through raw code's trailing-line trimming.
        // Its placeholder is replaced with empty content, so it adds no glyph or space.
        let end = text.len();
        if (text.is_empty() || text.ends_with('\n'))
            && links.iter().any(|link| {
                link.range == (end..end)
                    && (link.anchor.is_some() || link.replacement.as_deref() == Some(""))
            })
        {
            text.push('\u{fffc}');
            for link in &mut links {
                if link.range == (end..end)
                    && (link.anchor.is_some() || link.replacement.as_deref() == Some(""))
                {
                    link.range.end = text.len();
                    link.replacement.get_or_insert_with(String::new);
                }
            }
        }
        for link in &mut links {
            link.anchor = link.anchor.as_deref().and_then(|id| self.anchors.claim(id));
        }
        let index_count = positions.len();
        for link in &links {
            positions.extend([(link.range.start, 0), (link.range.end, 0)]);
        }
        transform_code_text(&mut text, &mut positions, |text| {
            expand_code_tabs(text, tab_size)
        });
        for (link, [(start, _), (end, _)]) in links
            .iter_mut()
            .zip(positions.split_off(index_count).as_chunks::<2>().0)
        {
            link.range = *start..*end;
        }
        Ok(CodeText {
            source: text,
            indexes: positions,
            links,
        })
    }

    fn collect_code_text<'inline>(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: impl DoubleEndedIterator<Item = InlineRef<'inline>> + ExactSizeIterator + Clone,
        extension: &str,
        line_comment: Option<&str>,
        suppress_indexes: bool,
        boundaries: TextBoundaries,
    ) -> Result<CodeText, Error> {
        let mut code = CodeText::default();

        let siblings = nodes.clone();
        for (index, node) in nodes.enumerate() {
            let boundaries = boundaries.for_inline_refs(
                Some(node),
                siblings.clone().take(index).rev(),
                siblings.clone().skip(index + 1),
            );
            if !suppress_indexes && let InlineKind::IndexTerm(term) = node.kind() {
                let entry = self.render_index_entry(traversal, term)?;
                if let Some(anchor) = self.index_catalog.add(entry) {
                    code.indexes.push((code.source.len(), anchor));
                }
            }
            if let InlineKind::Anchor(anchor) = node.kind() {
                code.links.push(CodeSpan {
                    range: code.source.len()..code.source.len(),
                    target: None,
                    anchor: Some(anchor.id().to_owned()),
                    replacement: Some(String::new()),
                    ..CodeSpan::default()
                });
            } else if let InlineKind::Footnote(footnote) = node.kind() {
                self.collect_code_footnote(traversal, footnote, &mut code)?;
            } else if let Some(children) = code_inline_content(node) {
                let child = self.collect_code_text(
                    traversal,
                    children.inlines(),
                    extension,
                    line_comment,
                    suppress_indexes || matches!(node.kind(), InlineKind::IndexTerm(_)),
                    boundaries,
                )?;
                code.append(self.format_code_text(node, child));
            } else if let Some(link) =
                resolve_code_link(node, &self.processor.references, extension)
            {
                let mut child = if let Some(children) =
                    code_link_content(node).filter(|nodes| !nodes.is_empty())
                {
                    self.collect_code_text(
                        traversal,
                        children.inlines(),
                        extension,
                        line_comment,
                        suppress_indexes,
                        boundaries,
                    )?
                } else {
                    CodeText {
                        source: link.text,
                        ..CodeText::default()
                    }
                };
                child.wrap_span(CodeSpan {
                    target: link.target,
                    anchor: link.anchor,
                    ..CodeSpan::default()
                });
                code.append(child);
            } else if let InlineKind::Verbatim(verbatim) = node.kind() {
                let mut content = verbatim.text();
                if index
                    .checked_sub(1)
                    .is_some_and(|previous| is_xml_callout(siblings.clone(), previous))
                {
                    content = content.strip_prefix("-->").unwrap_or(content);
                }
                if index.checked_add(1).is_some_and(|next| {
                    matches!(
                        siblings.clone().nth(next).map(InlineRef::kind),
                        Some(InlineKind::Callout(_))
                    )
                }) {
                    if index
                        .checked_add(1)
                        .is_some_and(|next| is_xml_callout(siblings.clone(), next))
                    {
                        content = content.strip_suffix("<!--").unwrap_or(content);
                    }
                    content = strip_pdf_callout_guard(content, line_comment);
                }
                code.source.push_str(content);
            } else if let InlineKind::Callout(callout) = node.kind() {
                let _ = write!(code.source, "({})", callout.number());
            } else {
                code.push_inline_text(node, boundaries);
            }
        }

        Ok(code)
    }

    fn format_code_text(&mut self, node: InlineRef<'_>, mut text: CodeText) -> CodeText {
        let quotes = if matches!(node.kind(), InlineKind::CurvedQuotation(_)) {
            Some(("“", "”"))
        } else if matches!(node.kind(), InlineKind::CurvedApostrophe(_)) {
            Some(("‘", "’"))
        } else {
            None
        };
        if let Some((open, close)) = quotes {
            let mut wrapped = CodeText {
                source: open.to_owned(),
                ..CodeText::default()
            };
            wrapped.append(text);
            wrapped.source.push_str(close);
            text = wrapped;
        }
        if let Some(span) = self.code_formatting(node) {
            text.wrap_span(span);
        }
        text
    }

    fn code_formatting(&mut self, node: InlineRef<'_>) -> Option<CodeSpan> {
        let (id, role, prefix, suffix) = match node.kind() {
            InlineKind::Bold(text) => (
                text.id().map(acdc_parser::ast::ElementIdRef::as_str),
                text.role(),
                "#strong[",
                "]",
            ),
            InlineKind::Italic(text) => (
                text.id().map(acdc_parser::ast::ElementIdRef::as_str),
                text.role(),
                "#emph[",
                "]",
            ),
            InlineKind::Monospace(text) => (
                text.id().map(acdc_parser::ast::ElementIdRef::as_str),
                text.role(),
                "",
                "",
            ),
            InlineKind::Highlight(text) => {
                let (prefix, suffix) = if text.id().is_none() && text.role().is_none() {
                    ("#highlight[", "]")
                } else {
                    ("", "")
                };
                (
                    text.id().map(acdc_parser::ast::ElementIdRef::as_str),
                    text.role(),
                    prefix,
                    suffix,
                )
            }
            InlineKind::Superscript(text) => (
                text.id().map(acdc_parser::ast::ElementIdRef::as_str),
                text.role(),
                "#super[",
                "]",
            ),
            InlineKind::Subscript(text) => (
                text.id().map(acdc_parser::ast::ElementIdRef::as_str),
                text.role(),
                "#sub[",
                "]",
            ),
            InlineKind::CurvedQuotation(text) => (
                text.id().map(acdc_parser::ast::ElementIdRef::as_str),
                text.role(),
                "",
                "",
            ),
            InlineKind::CurvedApostrophe(text) => (
                text.id().map(acdc_parser::ast::ElementIdRef::as_str),
                text.role(),
                "",
                "",
            ),
            InlineKind::Plain(_)
            | InlineKind::Raw(_)
            | InlineKind::Verbatim(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::CharacterReference(_)
            | InlineKind::Symbol(_)
            | InlineKind::LineBreak(_)
            | InlineKind::Anchor(_)
            | InlineKind::Callout(_)
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
            | _ => return None,
        };
        // Reuse prose role handling, but attach the ID once when the code span
        // is split across lines or around nested links and footnotes.
        let output = replace(&mut self.writer, Writer::new());
        let state = self.write_inline_span_start(None, role);
        self.writer.raw(prefix);
        let prefix = replace(&mut self.writer, Writer::new()).into_string();
        self.writer.raw(suffix);
        self.write_inline_span_end(state);
        let suffix = replace(&mut self.writer, output).into_string();
        Some(CodeSpan {
            anchor: id.map(str::to_owned),
            prefix,
            suffix,
            ..CodeSpan::default()
        })
    }

    fn code_link_lines(
        &mut self,
        source: &str,
        links: &[CodeSpan],
        autofit: bool,
        tab_size: usize,
    ) -> Vec<Vec<CodeSpan>> {
        if links.is_empty() {
            return Vec::new();
        }
        if source.is_empty() {
            for link in links {
                if let Some(anchor) = &link.anchor {
                    let _ = write!(self.writer, "#metadata(none)<{anchor}>");
                }
            }
            return Vec::new();
        }
        let wrapped = if autofit {
            Cow::Borrowed(source)
        } else {
            wrap_code_text(
                source,
                self.table_cell_code_wrap_columns()
                    .unwrap_or(self.code_wrap_columns),
                tab_size,
            )
        };
        let mut offset = 0;
        wrapped
            .split_inclusive('\n')
            .take(source_line_count(&wrapped))
            .map(|line| {
                let end = offset + line.trim_end_matches('\n').len();
                let spans = links
                    .iter()
                    .filter_map(|link| {
                        let start = link.range.start.max(offset);
                        let stop = link.range.end.min(end);
                        let empty_anchor = link.range.is_empty()
                            && (link.anchor.is_some() || link.replacement.is_some())
                            && link.range.start >= offset
                            && (link.range.start < end
                                || (link.range.start == end
                                    && (end == source.len()
                                        || source.as_bytes().get(end) == Some(&b'\n'))));
                        (start < stop || empty_anchor).then(|| CodeSpan {
                            range: start - offset..stop - offset,
                            target: link.target.clone(),
                            replacement: link.replacement.clone(),
                            anchor: (start == link.range.start)
                                .then(|| link.anchor.clone())
                                .flatten(),
                            prefix: link.prefix.clone(),
                            suffix: link.suffix.clone(),
                        })
                    })
                    .collect();
                offset = end + usize::from(source.as_bytes().get(end) == Some(&b'\n'));
                spans
            })
            .collect()
    }

    fn write_code_annotations(&mut self, annotations: &CodeAnnotations) {
        if !annotations.anchors.is_empty() {
            self.write_code_index_anchors(&annotations.anchors);
        }
        if annotations.links.is_empty() {
            return;
        }
        self.writer.raw(include_str!("code_links.typ"));
        self.writer.raw("  let links = (\n");
        for line in &annotations.links {
            self.writer.raw("    (");
            for span in line {
                let _ = write!(
                    self.writer,
                    "({}, {}, body => ",
                    span.range.start, span.range.end
                );
                if let Some(anchor) = &span.anchor {
                    let _ = write!(self.writer, "[#metadata(none)<{anchor}>] + ");
                }
                if !span.prefix.is_empty() {
                    self.writer.raw("[");
                    self.writer.raw(&span.prefix);
                    self.writer.raw("#");
                }
                if let Some(replacement) = &span.replacement {
                    self.writer.raw("[");
                    self.writer.raw(replacement);
                    self.writer.raw("]");
                } else {
                    match &span.target {
                        Some(CodeLinkTarget::External(target)) => {
                            self.writer.raw("link(");
                            self.writer.string_literal(target);
                            self.writer.raw(", body)");
                        }
                        Some(CodeLinkTarget::Internal(target)) => {
                            let _ = write!(
                                self.writer,
                                "context link(query(<{}>).first().location(), body)",
                                encode_label(target)
                            );
                        }
                        Some(CodeLinkTarget::DocumentTop) => {
                            self.writer.raw("link((page: 1, x: 0pt, y: 0pt), body)");
                        }
                        None => self.writer.raw("body"),
                    }
                }
                if !span.prefix.is_empty() {
                    self.writer.raw(&span.suffix);
                    self.writer.raw("]");
                }
                self.writer.raw("), ");
            }
            self.writer.raw("),\n");
        }
        self.writer.raw("  )\n  let code-links(line) = {\n    let start = 0\n    let body = []\n    for (from, to, make-link) in links.at(line.number - 1, default: ()) {\n      body += code-slice(line.body, start, from).first()\n      body += make-link(code-slice(line.body, from, to).first())\n      start = to\n    }\n    body + code-slice(line.body, start, line.text.len()).first()\n  }\n");
    }

    fn code_index_anchor_lines(
        &mut self,
        source: &str,
        positions: Vec<(usize, usize)>,
        autofit: bool,
        tab_size: usize,
    ) -> Vec<Vec<usize>> {
        if positions.is_empty() {
            return Vec::new();
        }
        let wrapped = if autofit {
            Cow::Borrowed(source)
        } else {
            wrap_code_text(
                source,
                self.table_cell_code_wrap_columns()
                    .unwrap_or(self.code_wrap_columns),
                tab_size,
            )
        };
        let mut positions = positions.into_iter().peekable();
        let mut lines = Vec::new();
        let mut offset = 0;
        for line in wrapped
            .split_inclusive('\n')
            .take(source_line_count(&wrapped))
        {
            let end = offset + line.trim_end_matches('\n').len();
            let original_newline = source.as_bytes().get(end) == Some(&b'\n');
            let include_end = original_newline || end == source.len();
            let mut anchors = Vec::new();
            while positions
                .peek()
                .is_some_and(|(position, _)| *position < end || (include_end && *position == end))
            {
                if let Some((_, anchor)) = positions.next() {
                    anchors.push(anchor);
                }
            }
            lines.push(anchors);
            offset = end + usize::from(original_newline);
        }
        if let Some(last_line) = lines.last_mut() {
            last_line.extend(positions.map(|(_, anchor)| anchor));
        } else {
            // A concealed term can leave no raw line to carry its locator.
            for (_, anchor) in positions {
                let _ = write!(self.writer, "#metadata(none)<__indexterm-{anchor}>");
            }
        }
        lines
    }

    pub(crate) fn write_page_layout_change(&mut self, br: PageBreakRef<'_>) {
        let layout = if let Some(value) =
            attribute_text(br.attributes(), "page-layout").filter(|value| !value.is_empty())
        {
            match value {
                "portrait" => Some(PageLayout::Portrait),
                "landscape" => Some(PageLayout::Landscape),
                _ => {
                    warn_with_advice_at(
                        &mut self.diagnostics,
                        Some(&Location::from(
                            br.attributes().location().unwrap_or(br.location()),
                        )),
                        format!(
                            "unsupported page-break layout '{value}', keeping the current page layout"
                        ),
                        "Set `page-layout` to `portrait` or `landscape`, or remove it to keep the current layout.",
                    );
                    None
                }
            }
        } else {
            // Asciidoctor ignores repeated role names before selecting a layout.
            let portrait = br.roles().iter().position(|role| role == "portrait");
            let landscape = br.roles().iter().position(|role| role == "landscape");
            match (portrait, landscape) {
                (Some(portrait), Some(landscape)) if portrait > landscape => {
                    Some(PageLayout::Portrait)
                }
                (_, Some(_)) => Some(PageLayout::Landscape),
                (Some(_), None) => Some(PageLayout::Portrait),
                (None, None) => None,
            }
        };
        let Some(layout) = layout.filter(|layout| *layout != self.page_layout) else {
            return;
        };

        let page = self.config.emit_options.page;
        if let Some(margin) = self.config.emit_options.page_margin
            && PageSetup::with_margin(page, layout, margin).is_none()
        {
            warn_with_advice_at(
                &mut self.diagnostics,
                Some(&Location::from(
                    br.attributes().location().unwrap_or(br.location()),
                )),
                "page-break layout leaves no room inside the configured margins, keeping the current page layout",
                "Reduce `pdf-page-margin` or choose a larger page size before switching the layout.",
            );
            return;
        }
        if let PageSize::Custom(_) = page {
            // The preamble writes custom sizes with the initial orientation applied.
            let dimensions = page.dimensions(layout);
            let _ = writeln!(
                self.writer,
                "#set page(width: {}pt, height: {}pt, flipped: false)\n",
                dimensions.width_points(),
                dimensions.height_points(),
            );
        } else {
            let _ = writeln!(
                self.writer,
                "#set page(flipped: {})\n",
                layout == PageLayout::Landscape,
            );
        }
        self.page_layout = layout;
        self.page_width_pt = page.width_points(layout);
        self.code_wrap_columns = code_wrap_columns(
            self.config.theme,
            page,
            layout,
            self.config.emit_options.page_margin,
        );
    }

    pub(crate) fn write_stem_fallback(
        &mut self,
        content: &str,
        block: bool,
        location: Option<&Location>,
    ) {
        self.warn_unsupported("stem content", "rendering it as literal text", location);
        if block {
            self.writer.raw("#block[");
        }
        self.write_text_expr(content);
        if block {
            self.writer.raw("]\n\n");
        }
    }

    /// Write blocks inside the frame named by `frame`, or unframed when `None`.
    ///
    /// Example, sidebar, and open blocks each read differently in print, so they
    /// do not share one frame: an open block is a transparent container and
    /// takes none.
    pub(crate) fn write_framed_blocks(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        frame: Option<&str>,
        label: Option<&str>,
        blocks: impl Iterator<Item = BlockRef<'a>>,
    ) -> Result<(), Error> {
        if let Some(frame) = frame {
            let _ = writeln!(self.writer, "#{frame}[");
        }
        if let Some(label) = label {
            self.writer.raw("#text(weight: \"bold\")[");
            self.write_text_expr(label);
            self.writer.raw("]#linebreak()\n");
        }
        self.write_blocks(traversal, blocks)?;
        if frame.is_some() {
            self.writer.raw("]\n\n");
        }
        Ok(())
    }

    pub(crate) fn warn_unsupported(
        &mut self,
        feature: &str,
        fallback: &str,
        location: Option<&Location>,
    ) {
        warn_with_advice_at(
            &mut self.diagnostics,
            location,
            format!("{feature} is not yet supported by the PDF backend, {fallback}"),
            "Use the HTML backend or Asciidoctor PDF for this feature until PDF backend support is added.",
        );
    }

    pub(crate) fn warn_unsupported_once(
        &mut self,
        key: &'static str,
        message: impl Into<Cow<'static, str>>,
        advice: impl Into<Cow<'static, str>>,
        location: Option<&Location>,
    ) {
        if self.unsupported_metadata_warnings.insert(key) {
            warn_with_advice_at(&mut self.diagnostics, location, message, advice);
        }
    }

    pub(crate) fn warn_unsupported_parser_variant(
        &mut self,
        node_kind: &str,
        location: Option<&Location>,
    ) {
        warn_with_advice_at(
            &mut self.diagnostics,
            location,
            format!("an unsupported parser {node_kind} variant was omitted from PDF output"),
            "Use the HTML backend or Asciidoctor PDF for this document and report the unsupported construct.",
        );
    }

    pub(crate) fn warn_static_media_fallback(&mut self, location: &Location) {
        if self.static_media_warning == StaticMediaWarningState::Emitted {
            return;
        }
        self.static_media_warning = StaticMediaWarningState::Emitted;
        warn_with_advice_at(
            &mut self.diagnostics,
            Some(location),
            "interactive media playback is unavailable in static PDF output; rendering clickable source links",
            "Use the HTML backend when in-document playback is required.",
        );
    }

    pub(crate) fn write_static_media_link(&mut self, target: &str, kind: &str) {
        self.write_text_expr("►\u{a0}");
        self.writer.raw("#link(");
        self.writer.string_literal(target);
        self.writer.raw(")[");
        self.write_text_expr(target);
        self.writer.raw("]");
        self.write_text_expr(" ");
        self.writer.raw("#emph[");
        self.write_text_expr(&format!("({kind})"));
        self.writer.raw("]");
    }

    pub(crate) fn static_media_source_target(
        traversal: &TraversalContext<'a>,
        source: SourceRef<'_>,
    ) -> String {
        Self::static_media_target(traversal, source.to_string().as_str())
    }

    pub(crate) fn static_media_target(traversal: &TraversalContext<'a>, target: &str) -> String {
        acdc_converters_core::media::resolve_target(target, traversal)
    }

    pub(crate) fn has_asset(&self, target: &str) -> bool {
        self.assets.get(target).is_some()
    }

    pub(crate) fn asset_virtual_path(&self, target: &str) -> Option<String> {
        self.assets
            .get(target)
            .map(|asset| asset.virtual_path.clone())
    }

    pub(crate) fn write_static_media_caption(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
    ) -> Result<(), Error> {
        let Some(title) = title else {
            return Ok(());
        };
        self.writer.raw("#imagecaption[");
        self.write_title(traversal, Some(title))?;
        self.writer.raw("]\n");
        Ok(())
    }

    pub(crate) fn write_list_item(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        marker: &str,
        item: ListItemRef<'a>,
    ) -> Result<(), Error> {
        let indent = "  ".repeat(self.list_depth);
        let _ = write!(self.writer, "{indent}{marker} ");
        let has_blocks = item.blocks().next().is_some();
        if has_blocks {
            self.writer.raw("#block(width: 100%)[");
        }
        if let Some(checked) = item.checked() {
            match checked {
                acdc_parser::ListItemCheckedStatus::Checked => self.writer.raw("#checkbox(true) "),
                acdc_parser::ListItemCheckedStatus::Unchecked => {
                    self.writer.raw("#checkbox(false) ");
                }
                _ => {}
            }
        }
        self.write_content(traversal, item.principal())?;
        self.writer.raw("\n");

        if has_blocks {
            self.writer.raw("\n");
            self.list_depth += 1;
            for block in item.blocks() {
                traversal.visit_block(self, block)?;
            }
            self.list_depth -= 1;
            let _ = writeln!(self.writer, "{indent}]");
        }
        Ok(())
    }

    pub(crate) fn write_bibliography_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: UnorderedListRef<'a>,
    ) -> Result<(), Error> {
        let indent = "  ".repeat(self.list_depth);
        let _ = writeln!(self.writer, "{indent}#[");
        let _ = write!(
            self.writer,
            "{indent}#set list(marker: box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("
        );
        self.writer.string_literal(&self.palette.bullet);
        self.writer.raw("))))\n");

        self.list_depth += 1;
        for item in list.items() {
            let item_indent = "  ".repeat(self.list_depth);
            let _ = write!(self.writer, "{item_indent}- #block(width: 100%)[");
            self.write_paragraph_alignment(Some(list.attributes()), |visitor| {
                visitor.write_bibliography_principal(traversal, item)
            })?;
            if item.blocks().next().is_some() {
                self.writer.raw("\n\n");
                self.list_depth += 1;
                for block in item.blocks() {
                    traversal.visit_block(self, block)?;
                }
                self.list_depth -= 1;
            }
            self.writer.raw("]\n");
        }
        self.list_depth -= 1;
        let _ = writeln!(self.writer, "{indent}]\n");
        Ok(())
    }

    fn write_bibliography_principal(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        item: ListItemRef<'a>,
    ) -> Result<(), Error> {
        let mut content = item.principal().inlines();
        let Some(first) = content.next() else {
            return Ok(());
        };
        let InlineKind::Anchor(anchor) = first.kind() else {
            return self.write_content(traversal, item.principal());
        };
        if !anchor.is_bibliography() {
            return self.write_content(traversal, item.principal());
        }
        self.write_inline_anchor(anchor.id());
        let references = Rc::clone(&self.processor.references);
        let backlink = references
            .get(anchor.id())
            .map(ReferenceRef::from)
            .is_some_and(ReferenceRef::has_automatic_citation);
        if backlink {
            let label = encode_bibliography_reference_label(anchor.id());
            let _ = write!(self.writer, "#link(<{label}>)[");
        }

        if let Some(label) = anchor.bibliography_label() {
            self.write_text_expr("[");
            self.write_content(traversal, label)?;
            self.write_text_expr("]");
        } else {
            self.write_text_expr(&format!("[{}]", anchor.id()));
        }

        if backlink {
            self.writer.raw("]");
        }
        self.write_inline_refs(traversal, content)
    }

    pub(crate) fn write_horizontal_description_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: DescriptionListRef<'a>,
    ) -> Result<(), Error> {
        if list.items().next().is_none() {
            return Ok(());
        }

        self.writer
            .raw("#layout(size => {\nlet term-width = calc.min(calc.max(0pt,\n");
        for row in description_list_rows(list.items()) {
            self.writer.raw("measure([");
            // Measurement content is not displayed, so its anchors must not
            // consume destinations or index entries from the visible term.
            let previous = self.index_catalog.set_suspended(true);
            let previous_anchors = replace(&mut self.anchors.suspended, true);
            let result = self.write_horizontal_description_terms(traversal, row.clone(), false);
            self.anchors.suspended = previous_anchors;
            self.index_catalog.set_suspended(previous);
            result?;
            self.writer.raw("]).width,\n");
        }
        self.writer.raw(
            "), size.width * 50%)\ngrid(columns: (term-width, 1fr), column-gutter: 20pt, row-gutter: 0.5em, align: top,\n",
        );

        for row in description_list_rows(list.items()) {
            let Some(item) = row.clone().last() else {
                continue;
            };
            self.writer.raw("[");
            self.write_horizontal_description_terms(traversal, row.clone(), true)?;
            self.writer.raw("], [");
            self.write_description_list_item(traversal, item)?;
            self.writer.raw("],\n");
        }

        self.writer.raw(")\n})\n\n");
        Ok(())
    }

    pub(crate) fn write_description_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: DescriptionListRef<'a>,
    ) -> Result<(), Error> {
        let indent = "  ".repeat(self.list_depth);

        for row in description_list_rows(list.items()) {
            let Some(description) = row.clone().last() else {
                continue;
            };
            let _ = writeln!(
                self.writer,
                "{indent}#block(width: 100%, above: 0pt, below: 0.5em)["
            );
            for (index, item) in row.clone().enumerate() {
                if index > 0 {
                    self.writer.raw("#linebreak()");
                }
                for anchor in item.anchors() {
                    self.write_anchor_target(anchor);
                }
                self.writer.raw("#text(weight: \"bold\")[");
                self.write_content(traversal, item.term())?;
                self.writer.raw("]");
            }
            if !description.principal().is_empty() || description.blocks().next().is_some() {
                self.writer
                    .raw("\n#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[");
                self.list_depth += 1;
                self.write_description_list_item(traversal, description)?;
                self.list_depth -= 1;
                self.writer.raw("]");
            }
            let _ = writeln!(self.writer, "\n{indent}]");
        }
        self.writer.raw("\n");
        Ok(())
    }

    pub(crate) fn write_qanda_description_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: DescriptionListRef<'a>,
    ) -> Result<(), Error> {
        if list.items().next().is_none() {
            return Ok(());
        }

        let indent = "  ".repeat(self.list_depth);
        let _ = writeln!(self.writer, "{indent}#[");
        let _ = write!(self.writer, "{indent}#set enum(numbering: ");
        self.write_ordered_list_numbering(OrderedListNumbering::Arabic);
        self.writer.raw(", spacing: 1em)\n");

        for row in description_list_rows(list.items()) {
            let Some(answer) = row.clone().last() else {
                continue;
            };
            let _ = write!(self.writer, "{indent}  + #block(width: 100%)[");
            self.writer.raw("#block(width: 100%, breakable: false)[");
            self.write_paragraph_alignment(Some(list.attributes()), |visitor| {
                visitor.write_qanda_description_terms(traversal, row.clone())?;
                if !answer.principal().is_empty() {
                    visitor.writer.raw("#linebreak()\n");
                    visitor.write_content(traversal, answer.principal())?;
                }
                Ok(())
            })?;
            self.writer.raw("]");
            if answer.blocks().next().is_some() {
                self.writer.raw("\n\n");
                self.write_blocks(traversal, answer.blocks())?;
            }
            self.writer.raw("]\n");
        }

        let _ = writeln!(self.writer, "{indent}]\n");
        Ok(())
    }

    pub(crate) fn write_marker_description_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: DescriptionListRef<'a>,
    ) -> Result<(), Error> {
        if list.items().next().is_none() {
            return Ok(());
        }

        let node = list;
        let ordered = node.style() == Some("ordered");
        let stacked = node.roles().contains("stack");
        let subject_stop = attribute_text(node.attributes(), "subject-stop");
        let indent = "  ".repeat(self.list_depth);
        let _ = writeln!(self.writer, "{indent}#[");
        if ordered {
            let _ = write!(self.writer, "{indent}#set enum(numbering: ");
            self.write_ordered_list_numbering(OrderedListNumbering::Arabic);
            self.writer.raw(")\n");
        } else {
            let _ = write!(
                self.writer,
                "{indent}#set list(marker: box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("
            );
            self.writer.string_literal(&self.palette.bullet);
            self.writer.raw("))))\n");
        }

        for row in description_list_rows(list.items()) {
            let Some(subject) = row.clone().next() else {
                continue;
            };
            let Some(description) = row.clone().last() else {
                continue;
            };
            let marker = if ordered { "+" } else { "-" };
            let _ = write!(self.writer, "{indent}  {marker} #block(width: 100%)[");
            for anchor in subject.anchors() {
                self.write_anchor_target(anchor);
            }
            self.writer.raw("#strong[");
            self.write_content(traversal, subject.term())?;
            let subject_text = InlineTextTransform::default()
                .decode_char_refs(true)
                .content_to_string(subject.term());
            let has_stop = subject_text
                .trim_end()
                .chars()
                .next_back()
                .is_some_and(|last| matches!(last, '.' | '!' | '?' | ';' | ':'));
            if !has_stop && let Some(stop) = subject_stop.or_else(|| (!stacked).then_some(":")) {
                self.write_text_expr(stop);
            }
            self.writer.raw("]");

            if !description.principal().is_empty() {
                self.writer
                    .raw(if stacked { "#linebreak()\n" } else { " " });
                self.write_content(traversal, description.principal())?;
            }
            if description.blocks().next().is_some() {
                self.writer.raw("\n\n");
                self.write_blocks(traversal, description.blocks())?;
            }
            self.writer.raw("]\n");
        }

        let _ = writeln!(self.writer, "{indent}]\n");
        Ok(())
    }

    fn write_horizontal_description_terms<'doc>(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        items: impl Iterator<Item = DescriptionListItemRef<'doc>>,
        write_anchors: bool,
    ) -> Result<(), Error> {
        for (index, item) in items.enumerate() {
            if index > 0 {
                self.writer.raw("#linebreak()");
            }
            if write_anchors {
                for anchor in item.anchors() {
                    self.write_anchor_target(anchor);
                }
            }
            self.writer.raw("#text(weight: \"bold\")[");
            self.write_content(traversal, item.term())?;
            self.writer.raw("]");
        }
        Ok(())
    }

    fn write_qanda_description_terms<'doc>(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        items: impl Iterator<Item = DescriptionListItemRef<'doc>>,
    ) -> Result<(), Error> {
        for (index, item) in items.enumerate() {
            if index > 0 {
                self.writer.raw("#linebreak()");
            }
            for anchor in item.anchors() {
                self.write_anchor_target(anchor);
            }
            self.writer.raw("#emph[");
            self.write_content(traversal, item.term())?;
            self.writer.raw("]");
        }
        Ok(())
    }

    fn write_description_list_item(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        item: DescriptionListItemRef<'a>,
    ) -> Result<(), Error> {
        if !item.principal().is_empty() {
            self.write_content(traversal, item.principal())?;
            if item.blocks().next().is_some() {
                self.writer.raw("\n\n");
            }
        }
        self.write_blocks(traversal, item.blocks())
    }

    pub(crate) fn write_ordered_list_start(
        &mut self,
        attributes: Option<AttributesRef<'_>>,
        marker: &str,
    ) {
        let indent = "  ".repeat(self.list_depth);
        let _ = writeln!(self.writer, "{indent}#[");
        let _ = write!(self.writer, "{indent}#set enum(numbering: ");
        let markerless = matches!(
            attribute_style(attributes),
            Some("none" | "no-bullet" | "unnumbered" | "unstyled")
        );
        if attribute_style(attributes) == Some("none") {
            self.writer.raw("(..numbers) => box(width: 0.5em)[]");
        } else if markerless {
            self.writer.raw("(..numbers) => none");
        } else {
            let numbering = match attributes
                .and_then(|attributes| attribute_text(attributes, "style"))
            {
                Some(style) => OrderedListNumbering::from_explicit_style(style).unwrap_or_default(),
                None => match marker.matches('.').count() {
                    2 => OrderedListNumbering::LowerAlpha,
                    3 => OrderedListNumbering::LowerRoman,
                    4 => OrderedListNumbering::UpperAlpha,
                    5 => OrderedListNumbering::UpperRoman,
                    _ => OrderedListNumbering::Arabic,
                },
            };
            self.write_ordered_list_numbering(numbering);
        }
        if attribute_style(attributes) == Some("unstyled") {
            self.writer.raw(", body-indent: 0pt");
            self.ordered_unstyled_scope_depth += 1;
        } else if self.ordered_unstyled_scope_depth > 0 {
            self.writer.raw(", body-indent: 0.5em");
        }
        if let Some(start) = attributes
            .and_then(|attributes| attribute_text(attributes, "start"))
            .and_then(|start| start.parse::<i64>().ok())
            .filter(|start| *start > 0)
        {
            let _ = write!(self.writer, ", start: {start}");
        }
        if has_attribute_token(attributes, "options", "reversed") {
            self.writer.raw(", reversed: true");
        }
        self.writer.raw(")\n");
    }

    pub(crate) fn write_ordered_list_end(&mut self, attributes: Option<AttributesRef<'_>>) {
        if attribute_style(attributes) == Some("unstyled") {
            self.ordered_unstyled_scope_depth -= 1;
        }
    }

    pub(crate) fn write_unordered_list_start(
        &mut self,
        attributes: Option<AttributesRef<'_>>,
    ) -> bool {
        if has_attribute_token(attributes, "options", "checklist") {
            return false;
        }
        let style = attributes
            .and_then(|attributes| attribute_text(attributes, "style"))
            .and_then(UnorderedMarker::from_style);
        let resets_parent_style = style.is_none() && self.unordered_style_scope_depth > 0;
        if style.is_none() && !resets_parent_style {
            return false;
        }

        let indent = "  ".repeat(self.list_depth);
        let _ = writeln!(self.writer, "{indent}#[");
        let _ = write!(self.writer, "{indent}#set list(marker: ");
        if let Some(style) = style {
            let _ = write!(
                self.writer,
                "depth => if depth == {} {{ ",
                self.unordered_list_depth
            );
            self.write_unordered_marker(style);
            self.writer.raw(" } else { let markers = ");
            self.write_default_unordered_markers();
            self.writer
                .raw("; markers.at(calc.rem(depth, markers.len())) }");
        } else {
            self.write_default_unordered_markers();
        }
        if style == Some(UnorderedMarker::Unstyled) {
            self.writer.raw(", body-indent: 0pt");
        } else if self.unordered_style_scope_depth > 0 {
            self.writer.raw(", body-indent: 0.5em");
        }
        self.writer.raw(")\n");
        self.unordered_style_scope_depth += 1;
        true
    }

    pub(crate) fn write_unordered_list_end(&mut self) {
        self.unordered_style_scope_depth -= 1;
    }

    fn write_default_unordered_markers(&mut self) {
        self.writer.raw("(");
        for (index, style) in [
            UnorderedMarker::Disc,
            UnorderedMarker::Circle,
            UnorderedMarker::Square,
        ]
        .into_iter()
        .enumerate()
        {
            if index > 0 {
                self.writer.raw(", ");
            }
            self.write_unordered_marker(style);
        }
        self.writer.raw(")");
    }

    fn write_unordered_marker(&mut self, style: UnorderedMarker) {
        match style {
            UnorderedMarker::Disc => {
                self.writer
                    .raw("box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb(");
                self.writer.string_literal(&self.palette.bullet);
                self.writer.raw(")))");
            }
            UnorderedMarker::Circle => {
                self.writer
                    .raw("box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb(");
                self.writer.string_literal(&self.palette.bullet);
                self.writer.raw(")))");
            }
            UnorderedMarker::Square => {
                self.writer
                    .raw("box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb(");
                self.writer.string_literal(&self.palette.bullet);
                self.writer.raw(")))");
            }
            UnorderedMarker::None => self.writer.raw("box(width: 0.28em)[]"),
            UnorderedMarker::NoBullet | UnorderedMarker::Unstyled => self.writer.raw("none"),
        }
    }

    fn write_ordered_list_numbering(&mut self, numbering: OrderedListNumbering) {
        self.writer.raw("(..numbers) => text(fill: rgb(");
        self.writer.string_literal(&self.palette.counter);
        self.writer.raw("), ");
        let pattern = match numbering {
            // Typst treats `0` as literal text in a numbering pattern, so `01.`
            // would produce `010.` for item 10 instead of Asciidoctor's `10.`.
            OrderedListNumbering::Decimal => {
                self.writer.raw(
                    "{ let number = numbers.pos().last(); (if number < 10 { \"0\" } else { \"\" }) + str(number) + \".\" }",
                );
                None
            }
            // Asciidoctor PDF advances through lowercase Greek Unicode code points,
            // including final sigma, instead of using Typst's Greek numerals.
            OrderedListNumbering::LowerGreek => {
                self.writer.raw(
                    "{ let number = numbers.pos().last(); str.from-unicode(944 + number) + \".\" }",
                );
                None
            }
            OrderedListNumbering::Arabic => Some("1."),
            OrderedListNumbering::LowerAlpha => Some("a."),
            OrderedListNumbering::UpperAlpha => Some("A."),
            OrderedListNumbering::LowerRoman => Some("i."),
            OrderedListNumbering::UpperRoman => Some("I."),
        };
        if let Some(pattern) = pattern {
            self.writer.raw("numbering(");
            self.writer.string_literal(pattern);
            self.writer.raw(", ..numbers.pos())");
        }
        self.writer.raw(")");
    }

    pub(crate) fn write_table(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        table: TableRef<'a>,
        attributes: Option<AttributesRef<'_>>,
    ) -> Result<(), Error> {
        let column_count = column_count(table);
        let width = table_width(attributes);
        let autowidth = has_attribute_token(attributes, "options", "autowidth");
        let constrained = width.is_some() || has_attribute_token(attributes, "role", "stretch");
        let columns = table.columns().collect::<Vec<_>>();
        let tracks = (!autowidth || constrained)
            .then(|| table_column_tracks(columns.iter().copied(), column_count));
        if let Some(tracks) = &tracks {
            let _ = write!(
                self.writer,
                "#table(columns: {}",
                typst_table_column_tracks(tracks)
            );
        } else {
            let _ = write!(self.writer, "#table(columns: {column_count}");
        }
        let alignments = table_column_alignments(columns.iter().copied(), column_count);
        let _ = write!(self.writer, ", align: {alignments}, stroke: none");

        let grid = grid_rows(table, column_count);
        let presentation = table.presentation().unwrap_or_else(|| {
            TablePresentation::resolve_with(attributes, |name| traversal.get(name))
        });
        let header_rows = usize::from(grid.first().is_some_and(|row| row.is_header));
        let row_count = grid.len();
        let available_columns = self
            .table_cell_code_wrap_columns()
            .unwrap_or(self.code_wrap_columns)
            .saturating_mul(usize::from(width.unwrap_or(100)))
            / 100;
        let context = TableRenderContext {
            columns,
            presentation,
            column_count,
            row_count,
            header_rows,
            column_width_columns: tracks.as_ref().map_or_else(
                || equal_table_column_widths(column_count, available_columns),
                |tracks| table_column_width_columns(tracks, available_columns),
            ),
        };
        if let Some(header) = grid.first().filter(|row| row.is_header) {
            self.writer.raw(", table.header(repeat: true, ");
            self.write_table_row_cells(traversal, &context, header, 0, "")?;
            self.writer.raw(")");
        }

        // Typst owns the merged geometry. Emit each real cell at its logical
        // position and omit the grid's horizontal/vertical span placeholders.
        for (y, row) in grid
            .iter()
            .enumerate()
            .filter(|(_, row)| !row.is_header && !row.is_footer)
        {
            self.write_table_row_cells(traversal, &context, row, y, ", ")?;
        }

        if let Some(footer) = grid.last().filter(|row| row.is_footer) {
            self.writer.raw(", table.footer(repeat: false, ");
            self.write_table_row_cells(traversal, &context, footer, grid.len() - 1, "")?;
            self.writer.raw(")");
        }

        self.writer.raw(")\n\n");
        Ok(())
    }

    pub(crate) fn write_table_width_start(
        &mut self,
        attributes: Option<AttributesRef<'_>>,
    ) -> bool {
        let Some(width) = table_width(attributes).filter(|width| *width < 100) else {
            return false;
        };
        let _ = writeln!(self.writer, "#block(width: {width}%)[");
        true
    }

    pub(crate) fn write_table_alignment_start(
        &mut self,
        attributes: Option<AttributesRef<'_>>,
    ) -> bool {
        let alignment = table_alignment(attributes, self.table_theme.align);
        let has_reduced_width = table_width(attributes).is_some_and(|width| width < 100);
        if alignment == TableAlignment::Left
            || !(has_reduced_width || Self::table_has_intrinsic_width(attributes))
        {
            return false;
        }
        let _ = writeln!(self.writer, "#align({})[", typst_table_alignment(alignment));
        true
    }

    pub(crate) fn write_table_wrappers_start(
        &mut self,
        attributes: Option<AttributesRef<'_>>,
        enabled: bool,
    ) -> (bool, bool) {
        let aligned = enabled && self.write_table_alignment_start(attributes);
        let sized = enabled && self.write_table_width_start(attributes);
        (sized, aligned)
    }

    pub(crate) fn table_has_intrinsic_width(attributes: Option<AttributesRef<'_>>) -> bool {
        has_attribute_token(attributes, "options", "autowidth")
            && table_width(attributes).is_none()
            && !has_attribute_token(attributes, "role", "stretch")
    }

    pub(crate) fn write_intrinsic_table_start(&mut self) {
        self.writer.raw("#context {\nlet acdc-table-body = [\n");
    }

    pub(crate) fn write_intrinsic_table_end(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: BlockRef<'_>,
        fallback: Option<CaptionKind>,
    ) -> Result<(), Error> {
        let attributes = block.attributes();
        let alignment = table_alignment(attributes, self.table_theme.align);
        let _ = writeln!(
            self.writer,
            "]\nalign({}, [\n#context block(width: measure(acdc-table-body).width)[",
            typst_table_alignment(alignment)
        );
        let sticky_title =
            !has_attribute_token(attributes, "options", "unbreakable") && block.title().is_some();
        if sticky_title {
            self.writer
                .raw("#block(sticky: true, above: 0pt, below: 0pt)[\n");
        }
        self.write_captioned_title(traversal, block.title(), attributes, fallback)?;
        if sticky_title {
            self.writer.raw("]\n");
        }
        self.writer.raw("]\n#acdc-table-body\n])\n}\n\n");
        Ok(())
    }

    pub(crate) fn write_table_wrappers_end(&mut self, sized: bool, aligned: bool) {
        if sized {
            self.writer.raw("]\n");
        }
        if aligned {
            self.writer.raw("]\n");
        }
        if sized || aligned {
            self.writer.raw("\n");
        }
    }

    pub(crate) fn omit_zero_width_table(&mut self, attributes: Option<AttributesRef<'_>>) -> bool {
        if table_width(attributes) != Some(0) {
            return false;
        }
        let location = attributes
            .and_then(AttributesRef::location)
            .map(Location::from);
        warn_with_advice_at(
            &mut self.diagnostics,
            location.as_ref(),
            "cannot fit contents of table cell into a table with a width of 0%; omitting the table",
            "Set the table width to a value greater than 0% to include it in the PDF.",
        );
        true
    }

    fn write_table_row_cells(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        context: &TableRenderContext<'_>,
        row: &GridRow<TableRowRef<'a>>,
        y: usize,
        first_separator: &str,
    ) -> Result<(), Error> {
        let mut separator = first_separator;
        let mut source_cells = row.ast_row.cells().enumerate();
        for (x, cell) in row.cells.iter().enumerate() {
            let CellKind::Content { cell_index } = cell else {
                continue;
            };
            if let Some((_, ast_cell)) = source_cells.find(|(index, _)| index == cell_index) {
                self.writer.raw(separator);
                self.write_table_cell(
                    traversal,
                    context,
                    ast_cell,
                    *cell_index,
                    TableCellPosition {
                        x,
                        y,
                        is_header: row.is_header,
                        is_footer: row.is_footer,
                    },
                )?;
                separator = ", ";
            }
        }
        Ok(())
    }

    fn write_table_cell(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        context: &TableRenderContext<'_>,
        cell: TableCellRef<'a>,
        source_column_index: usize,
        position: TableCellPosition,
    ) -> Result<(), Error> {
        let cell_ref = cell;
        let TableCellPosition { x, y, .. } = position;
        let _ = write!(self.writer, "table.cell(x: {x}, y: {y}");
        if cell_ref.colspan() > 1 {
            let _ = write!(self.writer, ", colspan: {}", cell_ref.colspan());
        }
        if cell_ref.rowspan() > 1 {
            let _ = write!(self.writer, ", rowspan: {}", cell_ref.rowspan());
        }
        let style = if position.is_header {
            ColumnStyle::Header
        } else {
            cell_ref.style().unwrap_or_else(|| {
                context
                    .columns
                    .get(source_column_index)
                    .map_or(ColumnStyle::Default, |column| column.style())
            })
        };
        let default_alignment = || (HorizontalAlignment::default(), VerticalAlignment::default());
        let source_column_alignment = context
            .columns
            .get(source_column_index)
            .map_or_else(default_alignment, |column| {
                (column.halign(), column.valign())
            });
        let physical_column_alignment = context
            .columns
            .get(x)
            .map_or_else(default_alignment, |column| {
                (column.halign(), column.valign())
            });
        if let Some(alignment) = table_cell_alignment(
            cell_ref,
            style,
            source_column_alignment,
            physical_column_alignment,
        ) {
            let _ = write!(self.writer, ", align: {alignment}");
        }
        self.write_table_cell_stroke(context, cell_ref, position);
        self.write_table_cell_fill(context, position);
        self.writer.raw(")[");
        let outer_cell_state = replace(
            &mut self.table_cell_section_state,
            TableCellSectionState::Inside {
                width_columns: table_cell_width_columns(context, x, cell_ref.colspan()),
            },
        );
        let result = self.write_table_cell_content(traversal, cell, style);
        self.table_cell_section_state = outer_cell_state;
        result?;
        self.writer.raw("]");
        Ok(())
    }

    fn write_table_cell_stroke(
        &mut self,
        context: &TableRenderContext<'_>,
        cell_ref: TableCellRef<'_>,
        position: TableCellPosition,
    ) {
        let presentation = context.presentation;
        let column_count = context.column_count;
        let row_count = context.row_count;
        let header_rows = context.header_rows;
        let TableCellPosition { x, y, .. } = position;
        let row_rule = matches!(presentation.grid(), TableGrid::All | TableGrid::Rows);
        let column_rule = matches!(presentation.grid(), TableGrid::All | TableGrid::Columns);
        let vertical_frame = matches!(presentation.frame(), TableFrame::All | TableFrame::Sides);
        let horizontal_frame = matches!(presentation.frame(), TableFrame::All | TableFrame::Ends);
        let right = x.saturating_add(cell_ref.colspan()).min(column_count);
        let bottom = y.saturating_add(cell_ref.rowspan()).min(row_count);

        self.writer.raw(", stroke: (");
        self.write_table_stroke_side("left", if x == 0 { vertical_frame } else { column_rule });
        self.write_table_stroke_side(
            "right",
            if right == column_count {
                vertical_frame
            } else {
                column_rule
            },
        );
        if header_rows > 0 && y == header_rows {
            let _ = write!(
                self.writer,
                "top: {}pt + rgb(\"{}\"), ",
                self.table_theme.header_divider_width_pt, self.table_theme.border_color,
            );
        } else {
            self.write_table_stroke_side("top", if y == 0 { horizontal_frame } else { row_rule });
        }
        if header_rows > 0 && bottom == header_rows {
            let _ = write!(
                self.writer,
                "bottom: {}pt + rgb(\"{}\"), ",
                self.table_theme.header_divider_width_pt, self.table_theme.border_color,
            );
        } else {
            self.write_table_stroke_side(
                "bottom",
                if bottom == row_count {
                    horizontal_frame
                } else {
                    row_rule
                },
            );
        }
        self.writer.raw(")");
    }

    fn write_table_stroke_side(&mut self, side: &str, enabled: bool) {
        if enabled {
            let _ = write!(
                self.writer,
                "{side}: {}pt + rgb(\"{}\"), ",
                self.table_theme.border_width_pt, self.table_theme.border_color,
            );
        } else {
            let _ = write!(self.writer, "{side}: none, ");
        }
    }

    fn write_table_cell_fill(
        &mut self,
        context: &TableRenderContext<'_>,
        position: TableCellPosition,
    ) {
        let fill = if position.is_header {
            self.table_theme.header_background.as_deref()
        } else if position.is_footer {
            self.table_theme.footer_background.as_deref()
        } else {
            let body_row = position.y.saturating_sub(context.header_rows);
            let striped = match context.presentation.stripes() {
                TableStripes::All => true,
                TableStripes::Odd => body_row & 1 == 0,
                TableStripes::Even => body_row & 1 == 1,
                TableStripes::None | _ => false,
            };
            striped.then_some(self.table_theme.stripe_background.as_str())
        };
        if let Some(fill) = fill {
            let _ = write!(self.writer, ", fill: rgb(\"{fill}\")");
        }
    }

    fn write_table_cell_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        cell: TableCellRef<'a>,
        style: ColumnStyle,
    ) -> Result<(), Error> {
        if style == ColumnStyle::Literal
            && let Some(text) = literal_table_cell_text(cell.blocks())
        {
            self.write_table_literal(&text);
            return Ok(());
        }

        let wrapper = match style {
            ColumnStyle::Emphasis => Some("#tableemphasis["),
            ColumnStyle::Header => Some("#tableheader["),
            ColumnStyle::Monospace | ColumnStyle::Literal => Some("#tablemonospace["),
            ColumnStyle::Strong => Some("#tablestrong["),
            ColumnStyle::AsciiDoc | ColumnStyle::Default | _ => None,
        };
        if let Some(wrapper) = wrapper {
            self.writer.raw(wrapper);
        }
        if style == ColumnStyle::AsciiDoc {
            traversal.with_table_cell(cell, |traversal| {
                self.write_blocks(traversal, cell.blocks())
            })?;
        } else {
            self.write_blocks(traversal, cell.blocks())?;
        }
        if cell.blocks().next().is_none() {
            self.write_text_expr("");
        }
        if wrapper.is_some() {
            self.writer.raw("]");
        }
        Ok(())
    }

    pub(super) fn in_asciidoc_table_cell(&self) -> bool {
        self.in_table_cell()
    }

    fn in_table_cell(&self) -> bool {
        matches!(
            self.table_cell_section_state,
            TableCellSectionState::Inside { .. }
        )
    }

    fn table_cell_code_wrap_columns(&self) -> Option<usize> {
        self.table_cell_width_columns()
            .map(|columns| columns.saturating_sub(4).max(1))
    }

    fn table_cell_text_break_columns(&self) -> Option<usize> {
        self.table_cell_width_columns()
            .map(|columns| columns.saturating_sub(2).max(4))
    }

    fn table_cell_width_columns(&self) -> Option<usize> {
        match self.table_cell_section_state {
            TableCellSectionState::Inside { width_columns } => Some(width_columns),
            TableCellSectionState::Outside => None,
        }
    }

    pub(crate) fn write_block_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        image: ImageRef<'_>,
    ) -> Result<(), Error> {
        if block_image_float(Some(image.attributes())).is_some() {
            self.warn_unsupported(
                "block image side wrapping",
                "rendering the image on the requested side with following content below it",
                Some(&Location::from(image.location())),
            );
        }
        let has_caption = image.title().is_some();
        if has_caption {
            self.writer.raw("#block(width: 100%, breakable: false)[\n");
        }
        let source = Self::static_media_source_target(traversal, image.source());
        let alt = block_image_alt(image);
        let link = attribute_text(image.attributes(), "link").filter(|target| !target.is_empty());
        if let Some(asset) = self.assets.get(&source) {
            let width = image_width(image.attributes(), true);
            let alignment = block_image_alignment(Some(image.attributes()));
            let align_to_page = image.options().contains("align-to-page");
            let uses_doc_image = !align_to_page
                && alignment == BlockImageAlignment::Left
                && matches!(
                    width,
                    None | Some(
                        ImageWidth::Points {
                            constrain_to_bounds: true,
                            ..
                        } | ImageWidth::ContainerRatio {
                            constrain_to_bounds: true,
                            ..
                        }
                    )
                );
            if uses_doc_image {
                self.writer.raw("#docimage(");
                self.writer.string_literal(&asset.virtual_path);
                self.writer.raw(", alt: ");
                self.writer.string_literal(&alt);
                match width {
                    Some(ImageWidth::Points { value, .. }) => {
                        let _ = write!(self.writer, ", width: {value}pt");
                    }
                    Some(ImageWidth::ContainerRatio { value, .. }) => {
                        let _ = write!(self.writer, ", ratio: {value}");
                    }
                    Some(ImageWidth::IntrinsicRatio(_) | ImageWidth::ViewportRatio(_)) | None => {}
                }
                if let Some(target) = &link {
                    self.writer.raw(", destination: ");
                    self.writer.string_literal(target);
                }
                self.writer.raw(")\n");
            } else {
                self.write_positioned_block_image(
                    &asset.virtual_path,
                    &alt,
                    width,
                    alignment,
                    link,
                    align_to_page,
                );
            }
        } else {
            self.write_block_image_fallback(traversal, image, &alt, link)?;
            self.writer.raw("\n");
        }
        if has_caption {
            self.write_captioned_title_with(
                traversal,
                "imagecaption",
                image.title(),
                Some(image.attributes()),
                Some(CaptionKind::Figure),
            )?;
            self.writer.raw("]\n\n");
        } else {
            self.writer.raw("\n");
        }
        Ok(())
    }

    fn write_block_image_fallback(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        image: ImageRef<'_>,
        alt: &str,
        link: Option<&str>,
    ) -> Result<(), Error> {
        if let Some(target) = link {
            self.writer.raw("#link(");
            self.writer.string_literal(target);
            self.writer.raw(")[");
        }
        self.write_text_expr("[");
        if let Some(content) = image.alt() {
            self.write_content(traversal, content)?;
        } else {
            self.write_text_expr(alt);
        }
        self.write_text_expr("]");
        if link.is_some() {
            self.writer.raw("]");
        }
        self.write_text_expr(&format!(" | {}", image.source()));
        Ok(())
    }

    fn write_positioned_block_image(
        &mut self,
        path: &str,
        alt: &str,
        width: Option<ImageWidth>,
        alignment: BlockImageAlignment,
        link: Option<&str>,
        align_to_page: bool,
    ) {
        let clip = !align_to_page
            && matches!(
                width,
                None | Some(
                    ImageWidth::Points {
                        constrain_to_bounds: true,
                        ..
                    } | ImageWidth::ContainerRatio {
                        constrain_to_bounds: true,
                        ..
                    }
                )
            );
        if align_to_page {
            self.writer
                .raw("#context layout(available => move(dx: -here().position().x, ");
        }
        let block_prefix = if align_to_page { "" } else { "#" };
        let _ = write!(
            self.writer,
            "{block_prefix}block(width: {}{}, radius: {}pt, clip: {clip})[",
            if align_to_page {
                self.page_width_pt
            } else {
                100.0
            },
            if align_to_page { "pt" } else { "%" },
            self.image_radius_pt
        );
        if alignment != BlockImageAlignment::Left {
            let _ = write!(self.writer, "#align({})[", alignment.typst());
        }
        if let Some(target) = link {
            self.writer.raw("#link(");
            self.writer.string_literal(target);
            self.writer.raw(")[");
        }
        match width {
            Some(ImageWidth::Points { value, .. }) => {
                self.writer.raw("#image(");
                self.writer.string_literal(path);
                self.writer.raw(", alt: ");
                self.writer.string_literal(alt);
                let _ = write!(self.writer, ", width: {value}pt)");
            }
            Some(ImageWidth::ContainerRatio { value, .. }) => {
                self.writer.raw("#image(");
                self.writer.string_literal(path);
                self.writer.raw(", alt: ");
                self.writer.string_literal(alt);
                if align_to_page {
                    let _ = write!(self.writer, ", width: {value} * available.width)");
                } else {
                    let _ = write!(self.writer, ", width: {}%)", value * 100.0);
                }
            }
            Some(ImageWidth::IntrinsicRatio(ratio)) => {
                self.write_intrinsically_scaled_image(
                    path,
                    alt,
                    ratio,
                    true,
                    align_to_page.then_some("available.width"),
                );
            }
            Some(ImageWidth::ViewportRatio(ratio)) => {
                self.writer.raw("#image(");
                self.writer.string_literal(path);
                self.writer.raw(", alt: ");
                self.writer.string_literal(alt);
                let _ = write!(self.writer, ", width: {}pt)", ratio * self.page_width_pt);
            }
            None => {
                self.writer.raw("#image(");
                self.writer.string_literal(path);
                self.writer.raw(", alt: ");
                self.writer.string_literal(alt);
                self.writer.raw(")");
            }
        }
        if link.is_some() {
            self.writer.raw("]");
        }
        if alignment != BlockImageAlignment::Left {
            self.writer.raw("]");
        }
        self.writer.raw("]\n");
        if align_to_page {
            self.writer.raw("))\n");
        }
    }

    pub(crate) fn write_inline_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        image: InlineImageRef<'_>,
    ) -> Result<(), Error> {
        let source = Self::static_media_target(traversal, &image.source().to_string());
        let alt = acdc_converters_core::media::image_alt_from_content(image.source(), image.alt());
        let link = attribute_text(image.attributes(), "link").filter(|target| !target.is_empty());
        if let Some(asset) = self.assets.get(&source) {
            let fit = attribute_text(image.attributes(), "fit").unwrap_or_default();
            let fit_line = fit == "line";
            if fit == "none" {
                self.warn_unsupported_once(
                    "inline-image-fit-none",
                    "inline image `fit=none` page-height sizing is not supported by the PDF backend; rendering with normal intrinsic sizing",
                    "Use `fit=line` to constrain the image to the text line, or use Asciidoctor PDF when the image must use the full page height.",
                    Some(&image.location().into()),
                );
            }
            self.writer.raw("#box(");
            if fit_line {
                self.writer.raw("context layout(size => { let body = ");
            }
            if let Some(target) = &link {
                self.writer.raw("link(");
                self.writer.string_literal(target);
                self.writer.raw(")[#");
            }
            match image_width(image.attributes(), false) {
                Some(ImageWidth::IntrinsicRatio(ratio)) => {
                    self.write_intrinsically_scaled_image(
                        &asset.virtual_path,
                        &alt,
                        ratio,
                        false,
                        None,
                    );
                }
                width => {
                    self.writer.raw("image(");
                    self.writer.string_literal(&asset.virtual_path);
                    self.writer.raw(", alt: ");
                    self.writer.string_literal(&alt);
                    match width {
                        Some(ImageWidth::Points { value, .. }) => {
                            let _ = write!(self.writer, ", width: {value}pt");
                        }
                        Some(ImageWidth::ContainerRatio { value, .. }) => {
                            let _ = write!(self.writer, ", width: {}%", value.min(1.0) * 100.0);
                        }
                        Some(ImageWidth::ViewportRatio(ratio)) => {
                            let _ =
                                write!(self.writer, ", width: {}pt", ratio * self.page_width_pt);
                        }
                        Some(ImageWidth::IntrinsicRatio(_)) | None => {}
                    }
                    self.writer.raw(")");
                }
            }
            if link.is_some() {
                self.writer.raw("]");
            }
            if fit_line {
                self.writer.raw(
                    "; let body-height = measure(body).height; let line-height = measure(box(height: 1em)).height; let target-height = calc.min(size.height, line-height); if body-height > target-height { let factor = target-height / body-height * 100%; scale(x: factor, y: factor, reflow: true, body) } else { body } })",
                );
            }
            self.writer.raw(")");
        } else {
            if let Some(target) = &link {
                self.writer.raw("#link(");
                self.writer.string_literal(target);
                self.writer.raw(")[");
            }
            if let Some(content) = image.alt() {
                self.write_content(traversal, content)?;
            } else {
                self.write_text_expr(&inline_image_fallback_text(image));
            }
            if link.is_some() {
                self.writer.raw("]");
            }
        }
        Ok(())
    }

    fn write_intrinsically_scaled_image(
        &mut self,
        path: &str,
        alt: &str,
        ratio: f64,
        markup: bool,
        maximum_width: Option<&str>,
    ) {
        let prefix = if markup { "#" } else { "" };
        let percentage = ratio * 100.0;
        let maximum_width = maximum_width.unwrap_or("size.width");
        let _ = write!(
            self.writer,
            "{prefix}context layout(size => {{ let body = scale(x: {percentage}%, y: {percentage}%, reflow: true, image("
        );
        self.writer.string_literal(path);
        self.writer.raw(", alt: ");
        self.writer.string_literal(alt);
        let _ = write!(
            self.writer,
            ")); let body-width = measure(body).width; if body-width > {maximum_width} {{ let factor = {maximum_width} / body-width * 100%; scale(x: factor, y: factor, reflow: true, body) }} else {{ body }} }})"
        );
    }

    fn write_icon(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        icon: IconRef<'_>,
    ) -> Result<(), Error> {
        let alt = acdc_converters_core::icon::alt_from_content(icon.target(), icon.alt());
        let link = attribute_text(icon.attributes(), "link");
        if let Some(target) = &link {
            self.writer.raw("#link(");
            self.writer.string_literal(target);
            self.writer.raw(")[");
        }
        match IconMode::from_attributes(traversal) {
            IconMode::Font => {
                if let Some(glyph) = builtin_icon_glyph(&icon.target().to_string()) {
                    match attribute_text(icon.attributes(), "size") {
                        Some("2x") => self.write_sized_icon_glyph(glyph, "2em"),
                        Some("3x") => self.write_sized_icon_glyph(glyph, "3em"),
                        Some("4x") => self.write_sized_icon_glyph(glyph, "4em"),
                        Some("5x") => self.write_sized_icon_glyph(glyph, "5em"),
                        Some("lg") => self.write_sized_icon_glyph(glyph, "1.333em"),
                        Some("fw") => {
                            self.writer.raw("#box(width: 1em)[#align(center)[");
                            self.write_text_expr(glyph);
                            self.writer.raw("]]");
                        }
                        _ => self.write_text_expr(glyph),
                    }
                } else {
                    self.write_icon_alternative(traversal, icon, &alt)?;
                }
            }
            IconMode::Image => {
                let source = attribute_text(icon.attributes(), "icon").map_or_else(
                    || icon_image_source(traversal, icon.target()),
                    |value| acdc_converters_core::icon::custom_image_source(traversal, value),
                );
                if let Some(asset) = self.assets.get(&source) {
                    self.writer.raw("#box(image(");
                    self.writer.string_literal(&asset.virtual_path);
                    self.writer.raw(", alt: ");
                    self.writer.string_literal(&alt);
                    self.writer.raw(", height: 1em))");
                } else {
                    self.write_icon_alternative(traversal, icon, &alt)?;
                }
            }
            IconMode::Text | _ => self.write_icon_alternative(traversal, icon, &alt)?,
        }
        if link.is_some() {
            self.writer.raw("]");
        }
        Ok(())
    }

    fn write_icon_alternative(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        icon: IconRef<'_>,
        fallback: &str,
    ) -> Result<(), Error> {
        self.write_text_expr("[");
        if let Some(content) = icon.alt() {
            self.write_content(traversal, content)?;
        } else {
            self.write_text_expr(fallback);
        }
        self.write_text_expr("]");
        Ok(())
    }

    fn write_sized_icon_glyph(&mut self, glyph: &str, size: &str) {
        self.writer.raw("#text(size: ");
        self.writer.raw(size);
        self.writer.raw(")[");
        self.write_text_expr(glyph);
        self.writer.raw("]");
    }

    fn write_footnote(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        footnote: FootnoteRef<'_>,
    ) -> Result<(), Error> {
        if self.anchors.suspended {
            // Copied titles display the parsed marker without defining
            // another native footnote or duplicating the original label.
            let _ = write!(self.writer, "#super[{}]", footnote.number());
        } else if let Some(id) = footnote.id().filter(|_| footnote.content().is_empty()) {
            // Typst label references reuse the definition's counter value
            // without advancing the footnote counter.
            let _ = write!(self.writer, "#footnote(<{}>)", encode_footnote_label(id));
        } else {
            let _ = write!(
                self.writer,
                "#counter(footnote).update({})",
                footnote.number().saturating_sub(1)
            );
            self.writer.raw("#footnote[");
            self.write_content(traversal, footnote.content())?;
            self.writer.raw("]");
            if let Some(id) = footnote.id() {
                let _ = write!(self.writer, "<{}>", encode_footnote_label(id));
            }
        }

        Ok(())
    }

    pub(crate) fn write_inline_macro(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        match node.kind() {
            InlineKind::Footnote(footnote) => self.write_footnote(traversal, footnote)?,
            InlineKind::Icon(icon) => self.write_icon(traversal, icon)?,
            InlineKind::Image(image) => self.write_inline_image(traversal, image)?,
            InlineKind::Keyboard(keyboard) => {
                for (index, key) in keyboard.keys().enumerate() {
                    if index > 0 {
                        self.write_text_expr("\u{202f}+\u{202f}");
                    }
                    self.writer.raw("#box(baseline: 15%, fill: rgb(");
                    self.writer.string_literal(&self.palette.callout_bg);
                    self.writer.raw("), stroke: 0.5pt + rgb(");
                    self.writer.string_literal(&self.palette.border);
                    self.writer
                        .raw("), radius: 2pt, inset: (x: 2pt, y: 0.5pt))[");
                    self.write_content(traversal, key)?;
                    self.writer.raw("]");
                }
            }
            InlineKind::Button(button) => {
                self.writer.raw("#strong[\\[\u{2009}");
                self.write_content(traversal, button.label())?;
                self.writer.raw("\u{2009}\\]]");
            }
            InlineKind::Menu(menu) => self.write_menu(traversal, menu)?,
            InlineKind::Url(url) => {
                let target = url.target().to_string();
                let fallback = link_fallback(&target, url.hides_uri_scheme());
                self.write_link(
                    traversal,
                    &target,
                    Some(url.text()),
                    Some(url.attributes()),
                    Some(url.location()),
                    fallback,
                )?;
            }
            InlineKind::Link(link) => {
                let target = link.target().to_string();
                let fallback = link_fallback(&target, link.hides_uri_scheme());
                self.write_link(
                    traversal,
                    &target,
                    Some(link.text()),
                    Some(link.attributes()),
                    Some(link.location()),
                    fallback,
                )?;
            }
            InlineKind::Mailto(mailto) => {
                let target = mailto.target().to_string();
                let fallback = mailto_fallback(&target);
                self.write_link(
                    traversal,
                    &mailto_target(mailto),
                    Some(mailto.text()),
                    Some(mailto.attributes()),
                    Some(mailto.location()),
                    fallback,
                )?;
            }
            InlineKind::Autolink(autolink) => self.write_autolink(traversal, autolink)?,
            InlineKind::CrossReference(xref) => self.write_cross_reference(traversal, xref)?,
            InlineKind::Pass(pass) => {
                if let Some(text) = pass.text() {
                    self.write_text_expr(text);
                }
            }
            InlineKind::Stem(stem) => {
                self.write_stem_fallback(stem.text(), false, Some(&stem.location().into()));
            }
            InlineKind::IndexTerm(term) => {
                self.write_index_term(traversal, term)?;
            }
            InlineKind::Plain(_)
            | InlineKind::Raw(_)
            | InlineKind::Verbatim(_)
            | InlineKind::Bold(_)
            | InlineKind::Italic(_)
            | InlineKind::Monospace(_)
            | InlineKind::Highlight(_)
            | InlineKind::Subscript(_)
            | InlineKind::Superscript(_)
            | InlineKind::CurvedQuotation(_)
            | InlineKind::CurvedApostrophe(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::CharacterReference(_)
            | InlineKind::Symbol(_)
            | InlineKind::LineBreak(_)
            | InlineKind::Anchor(_)
            | InlineKind::Callout(_)
            | _ => {
                self.warn_unsupported_parser_variant("inline macro", Some(&node.location().into()));
            }
        }
        Ok(())
    }

    fn write_index_term(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        term: IndexTermRef<'_>,
    ) -> Result<(), Error> {
        if !self.index_catalog.is_suspended() {
            let entry = self.render_index_entry(traversal, term)?;
            if let Some(anchor) = self.index_catalog.add(entry) {
                // Whitespace before the label is visible in inline content.
                let _ = write!(self.writer, "#metadata(none)<__indexterm-{anchor}>");
            }
        }
        if term.is_visible() {
            self.write_content(traversal, term.term())?;
        }
        Ok(())
    }

    pub(crate) fn register_heading_index_term(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        term: IndexTermRef<'_>,
    ) -> Result<(), Error> {
        let entry = self.render_index_entry(traversal, term)?;
        self.index_catalog.register(entry);
        Ok(())
    }

    fn render_index_entry(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        term: IndexTermRef<'_>,
    ) -> Result<CatalogEntry, Error> {
        let term = term.catalog();
        let primary = self.render_index_catalog_term(traversal, term.term())?;
        let secondary = term
            .secondary()
            .map(|inlines| self.render_index_catalog_term(traversal, inlines))
            .transpose()?;
        let tertiary = term
            .tertiary()
            .map(|inlines| self.render_index_catalog_term(traversal, inlines))
            .transpose()?;
        let relationship = match term.relationship() {
            Some(IndexTermRelationshipRef::See { target }) => {
                CatalogRelationship::See(self.render_index_catalog_term(traversal, target)?)
            }
            Some(IndexTermRelationshipRef::SeeAlso { targets }) => CatalogRelationship::SeeAlso(
                targets
                    .iter()
                    .map(|target| self.render_index_catalog_term(traversal, target))
                    .collect::<Result<_, _>>()?,
            ),
            None | Some(_) => CatalogRelationship::None,
        };
        Ok(CatalogEntry {
            primary,
            secondary,
            tertiary,
            relationship,
        })
    }

    fn render_index_catalog_term(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: ContentRef<'_>,
    ) -> Result<CatalogTerm, Error> {
        let plain = InlineTextTransform::default()
            .decode_char_refs(true)
            .content_to_string(inlines);
        let output = replace(&mut self.writer, Writer::new());
        let previous = self.index_catalog.set_suspended(true);
        let boundaries = replace(&mut self.text_boundaries, TextBoundaries::BOTH);
        let in_span = replace(&mut self.in_inline_span, false);
        let result = self.write_content(traversal, inlines);
        self.text_boundaries = boundaries;
        self.in_inline_span = in_span;
        self.index_catalog.set_suspended(previous);
        let markup = replace(&mut self.writer, output).into_string();
        result?;
        Ok(CatalogTerm { plain, markup })
    }

    fn write_menu(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        menu: MenuRef<'_>,
    ) -> Result<(), Error> {
        self.writer.raw("#strong[");
        for (index, part) in std::iter::once(menu.target())
            .chain(menu.items())
            .enumerate()
        {
            if index > 0 {
                self.write_text_expr(" ");
                self.writer.raw("#text(size: 1.15em, fill: rgb(");
                self.writer.string_literal(&self.palette.accent);
                self.writer.raw("))[\u{203a}]");
                self.write_text_expr(" ");
            }
            self.write_content(traversal, part)?;
        }
        self.writer.raw("]");
        Ok(())
    }

    fn write_autolink(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        autolink: AutolinkRef<'_>,
    ) -> Result<(), Error> {
        let target = autolink.target().to_string();
        let (fallback, angle_brackets) = autolink_fallback(
            &target,
            autolink.is_bracketed(),
            autolink.hides_uri_scheme(),
        );
        if angle_brackets {
            self.write_text_expr("<");
        }
        self.write_link(traversal, &target, None, None, None, fallback)?;
        if angle_brackets {
            self.write_text_expr(">");
        }
        Ok(())
    }

    /// Write a cross-reference as a Typst link to the target's label.
    ///
    /// Typst fails the whole compilation on a link to a label that no element
    /// defines, so a target that is absent from the reference catalog — or a
    /// reference nested inside another one's text — renders as `[id]` text
    /// alone. Every cataloged target gets a label from `write_block_anchor`,
    /// `write_anchor_target` or the section heading.
    fn write_cross_reference(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        xref: CrossReferenceRef<'_>,
    ) -> Result<(), Error> {
        // Clone the handles so the borrowed reference text and the resolution
        // guard both outlive the `&mut self` render calls.
        let references = Rc::clone(&self.processor.references);
        let guard = self.processor.xref_guard.clone();
        let target = xref.target();

        if xref.target_is_local() && target.is_empty() {
            return self.write_document_top_reference(traversal, xref.text());
        }

        if xref.text().is_empty()
            && references
                .get(target)
                .map(ReferenceRef::from)
                .is_some_and(ReferenceRef::is_bibliography)
            && self
                .bibliography_backlinks_written
                .insert(target.to_string())
        {
            let label = encode_bibliography_reference_label(target);
            let _ = write!(self.writer, "#metadata(none) <{label}>");
        }

        if !xref.text().is_empty() {
            if !xref.target_is_local()
                && let Some((external_target, _)) = Self::interdocument_xref(traversal, target)
            {
                self.write_external_link(&external_target, |visitor| {
                    visitor.write_content(traversal, xref.text())
                })?;
            } else if references.contains_key(target) {
                self.write_labelled_link(target, |visitor| {
                    visitor.write_content(traversal, xref.text())
                })?;
            } else {
                self.write_content(traversal, xref.text())?;
            }
            return Ok(());
        }

        let previous = self.index_catalog.set_suspended(true);
        let previous_anchors = replace(&mut self.anchors.suspended, true);
        let reference = references.get(target).map(ReferenceRef::from);
        let result = match resolve_reference(reference, xref, &guard) {
            XrefDisplay::Title(inlines, _scope) | XrefDisplay::Label(inlines, _scope) => self
                .write_labelled_link(target, |visitor| visitor.write_content(traversal, inlines)),
            XrefDisplay::ShortCaption(prefix) => self.write_labelled_link(target, |visitor| {
                visitor.write_text_expr(&prefix);
                Ok(())
            }),
            XrefDisplay::FullCaption(prefix, inlines, _scope) => {
                self.write_labelled_link(target, |visitor| {
                    visitor.write_text_expr(&prefix);
                    visitor.write_text_expr(", “");
                    visitor.write_content(traversal, inlines)?;
                    visitor.write_text_expr("”");
                    Ok(())
                })
            }
            XrefDisplay::Emphasized(prefix, inlines, _scope) => {
                self.write_labelled_link(target, |visitor| {
                    if let Some(prefix) = prefix {
                        visitor.write_text_expr(&prefix);
                        visitor.write_text_expr(", ");
                    }
                    visitor.write_quoted_span(
                        traversal,
                        None,
                        None,
                        "#emph[",
                        inlines.inlines(),
                        "]",
                    )?;
                    Ok(())
                })
            }
            XrefDisplay::Fallback(text) => self.write_labelled_link(target, |visitor| {
                visitor.write_text_expr(&text);
                Ok(())
            }),
            XrefDisplay::Unresolved(text) | XrefDisplay::Nested(text) => {
                self.write_text_expr(&text);
                Ok(())
            }
            XrefDisplay::External(target) => {
                if let Some((target, text)) = Self::interdocument_xref(traversal, &target) {
                    self.write_external_link(&target, |visitor| {
                        visitor.write_text_expr(&text);
                        Ok(())
                    })
                } else {
                    self.write_text_expr(&target);
                    Ok(())
                }
            }
        };
        self.anchors.suspended = previous_anchors;
        self.index_catalog.set_suspended(previous);
        result
    }

    fn write_document_top_reference(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        text: ContentRef<'_>,
    ) -> Result<(), Error> {
        if self.processor.xref_guard.is_resolving() {
            self.write_text_expr("[^top]");
            return Ok(());
        }
        self.writer.raw("#link((page: 1, x: 0pt, y: 0pt))[");
        if text.is_empty() {
            self.write_text_expr("[^top]");
        } else {
            self.write_content(traversal, text)?;
        }
        self.writer.raw("]");
        Ok(())
    }

    fn interdocument_xref(
        traversal: &TraversalContext<'a>,
        target: &str,
    ) -> Option<(String, String)> {
        let attributes = &traversal;
        let extension = attributes
            .get("relfilesuffix")
            .and_then(|value| value.text())
            .or_else(|| {
                attributes
                    .get("outfilesuffix")
                    .and_then(|value| value.text())
            })
            .unwrap_or("pdf");
        interdocument_xref(target, extension.strip_prefix('.').unwrap_or(extension))
    }

    fn write_external_link(
        &mut self,
        target: &str,
        content: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.writer.raw("#link(");
        self.writer.string_literal(target);
        self.writer.raw(")[");
        content(self)?;
        self.writer.raw("]");
        Ok(())
    }

    /// Write `content` inside a Typst link to `target`'s label.
    fn write_labelled_link(
        &mut self,
        target: &str,
        content: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let label = encode_label(target);
        let _ = write!(
            self.writer,
            "#context link(query(<{label}>).first().location())["
        );
        content(self)?;
        self.writer.raw("]");
        Ok(())
    }

    fn write_link(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        target: &str,
        text: Option<ContentRef<'_>>,
        metadata: Option<AttributesRef<'_>>,
        location: Option<LocationRef<'_>>,
        fallback: &str,
    ) -> Result<(), Error> {
        let id = metadata
            .and_then(|metadata| metadata.get("id"))
            .and_then(|value| {
                if let AttributeValueRef::Text(id) = value {
                    Some(id)
                } else {
                    None
                }
            })
            .filter(|id| {
                location.is_some_and(|location| {
                    self.processor
                        .references
                        .get(*id)
                        .map(ReferenceRef::from)
                        .is_some_and(|reference| reference.location() == location)
                })
            });
        let mut role = String::new();
        if let Some(AttributeValueRef::Tokens(roles)) =
            metadata.and_then(|metadata| metadata.get("role"))
        {
            for (index, token) in roles.iter().enumerate() {
                if index > 0 {
                    role.push(' ');
                }
                role.push_str(token);
            }
        }
        let state = self.write_inline_span_start(id, (!role.is_empty()).then_some(role.as_str()));
        if let Some(text) = text {
            let mut nodes = text.inlines();
            if let Some(node) = nodes.next()
                && nodes.next().is_none()
                && let InlineKind::Image(image) = node.kind()
                && attribute_text(image.attributes(), "link").is_some()
            {
                self.write_inline_image(traversal, image)?;
                self.write_inline_span_end(state);
                return Ok(());
            }
        }

        self.writer.raw("#link(");
        self.writer.string_literal(target);
        self.writer.raw(")[");
        if let Some(text) = text.filter(|text| !text.is_empty()) {
            self.write_content(traversal, text)?;
        } else {
            self.write_text_expr(fallback);
        }
        self.writer.raw("]");
        self.write_inline_span_end(state);
        Ok(())
    }
}

fn description_list_rows<'doc>(
    mut items: impl Iterator<Item = DescriptionListItemRef<'doc>> + Clone,
) -> impl Iterator<Item = impl Iterator<Item = DescriptionListItemRef<'doc>> + Clone> {
    std::iter::from_fn(move || {
        let row = items.clone();
        let mut count = 0;
        for item in items.by_ref() {
            count += 1;
            if !item.principal().is_empty() || item.blocks().next().is_some() {
                break;
            }
        }
        (count > 0).then(|| row.take(count))
    })
}

fn table_column_tracks<'doc>(
    columns: impl Iterator<Item = ColumnFormatRef<'doc>> + Clone,
    column_count: usize,
) -> Vec<TableColumnTrack> {
    if columns.clone().next().is_none() {
        return vec![TableColumnTrack::Fraction(1); column_count];
    }

    let has_automatic_width = columns
        .clone()
        .any(|column| column.width() == ColumnWidth::Auto);
    let fixed_width_total: u64 = columns
        .clone()
        .filter_map(|column| match column.width() {
            ColumnWidth::Proportional(width) | ColumnWidth::Percentage(width) => {
                Some(u64::from(width))
            }
            ColumnWidth::Auto => None,
            _ => Some(1),
        })
        .sum();
    let fixed_widths_fit = fixed_width_total <= 100;
    let all_fixed_widths_are_zero = !has_automatic_width && fixed_width_total == 0;

    columns
        .map(|column| match column.width() {
            ColumnWidth::Proportional(_) | ColumnWidth::Percentage(_)
                if all_fixed_widths_are_zero =>
            {
                TableColumnTrack::Fraction(1)
            }
            ColumnWidth::Proportional(width) | ColumnWidth::Percentage(width)
                if has_automatic_width && fixed_widths_fit =>
            {
                TableColumnTrack::Percentage(width)
            }
            ColumnWidth::Proportional(width) | ColumnWidth::Percentage(width) => {
                TableColumnTrack::Fraction(width)
            }
            ColumnWidth::Auto if fixed_widths_fit => TableColumnTrack::Fraction(1),
            ColumnWidth::Auto => TableColumnTrack::Fraction(0),
            _ => TableColumnTrack::Fraction(1),
        })
        .collect()
}

fn typst_table_column_tracks(tracks: &[TableColumnTrack]) -> String {
    let tracks = tracks
        .iter()
        .map(|track| match track {
            TableColumnTrack::Fraction(width) => format!("{width}fr"),
            TableColumnTrack::Percentage(width) => format!("{width}%"),
        })
        .collect::<Vec<_>>();
    format!("({})", tracks.join(", "))
}

fn table_cell_width_columns(
    context: &TableRenderContext<'_>,
    column: usize,
    colspan: usize,
) -> usize {
    let span_end = column.saturating_add(colspan).min(context.column_count);
    context
        .column_width_columns
        .get(column..span_end)
        .unwrap_or_default()
        .iter()
        .copied()
        .sum::<usize>()
        .max(1)
}

fn equal_table_column_widths(column_count: usize, table_columns: usize) -> Vec<usize> {
    let width = table_columns
        .checked_div(column_count)
        .unwrap_or(table_columns);
    vec![width; column_count]
}

fn table_column_width_columns(tracks: &[TableColumnTrack], table_columns: usize) -> Vec<usize> {
    let percentage_total = tracks
        .iter()
        .filter_map(|track| match track {
            TableColumnTrack::Percentage(width) => Some(usize::try_from(*width).unwrap_or(100)),
            TableColumnTrack::Fraction(_) => None,
        })
        .sum::<usize>()
        .min(100);
    let fraction_total = tracks
        .iter()
        .filter_map(|track| match track {
            TableColumnTrack::Fraction(width) => {
                Some(usize::try_from(*width).unwrap_or(usize::MAX))
            }
            TableColumnTrack::Percentage(_) => None,
        })
        .sum::<usize>();
    let fraction_columns = table_columns.saturating_mul(100 - percentage_total) / 100;

    tracks
        .iter()
        .map(|track| match track {
            TableColumnTrack::Percentage(width) => {
                table_columns.saturating_mul(usize::try_from(*width).unwrap_or(100)) / 100
            }
            TableColumnTrack::Fraction(width) => fraction_columns
                .saturating_mul(usize::try_from(*width).unwrap_or(usize::MAX))
                .checked_div(fraction_total)
                .unwrap_or(0),
        })
        .collect()
}

fn table_width(attributes: Option<AttributesRef<'_>>) -> Option<u8> {
    let AttributeValueRef::Text(value) = attributes?.get("width")? else {
        return Some(100);
    };
    if matches!(value, "0" | "0%") {
        return Some(0);
    }
    let value = value.trim_start();
    let sign_len = usize::from(value.starts_with(['+', '-']));
    let digit_len = value[sign_len..]
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    let integer = value[..sign_len + digit_len].parse::<i64>().unwrap_or(0);
    Some(
        u8::try_from(integer)
            .ok()
            .filter(|width| (1..=100).contains(width))
            .unwrap_or(100),
    )
}

fn table_alignment(
    attributes: Option<AttributesRef<'_>>,
    theme_default: TableAlignment,
) -> TableAlignment {
    if let Some(AttributeValueRef::Text(value)) =
        attributes.and_then(|attributes| attributes.get("align"))
        && let Some(alignment) = source_table_alignment(value)
    {
        return alignment;
    }
    if let Some(AttributeValueRef::Tokens(roles)) =
        attributes.and_then(|attributes| attributes.get("role"))
    {
        roles
            .iter()
            .filter_map(source_table_alignment)
            .last()
            .unwrap_or(theme_default)
    } else {
        theme_default
    }
}

fn source_table_alignment(value: &str) -> Option<TableAlignment> {
    match value {
        "left" => Some(TableAlignment::Left),
        "center" => Some(TableAlignment::Center),
        "right" => Some(TableAlignment::Right),
        _ => None,
    }
}

const fn typst_table_alignment(alignment: TableAlignment) -> &'static str {
    match alignment {
        TableAlignment::Left => "left",
        TableAlignment::Center => "center",
        TableAlignment::Right => "right",
    }
}

fn table_column_alignments<'doc>(
    columns: impl Iterator<Item = ColumnFormatRef<'doc>> + Clone,
    column_count: usize,
) -> String {
    if columns.clone().next().is_none() {
        return format!("({})", vec!["left + top"; column_count].join(", "));
    }

    let alignments = columns
        .map(|column| {
            let horizontal = typst_horizontal_alignment(column.halign());
            let vertical = typst_vertical_alignment(column.valign());
            format!("{horizontal} + {vertical}")
        })
        .collect::<Vec<_>>();

    format!("({})", alignments.join(", "))
}

fn table_cell_alignment(
    cell: TableCellRef<'_>,
    style: ColumnStyle,
    source_column_alignment: (HorizontalAlignment, VerticalAlignment),
    physical_column_alignment: (HorizontalAlignment, VerticalAlignment),
) -> Option<String> {
    let (column_horizontal, column_vertical) = source_column_alignment;
    let horizontal = cell.halign().unwrap_or(column_horizontal);
    let vertical = cell.valign().unwrap_or(column_vertical);

    if style == ColumnStyle::AsciiDoc {
        let effective = (HorizontalAlignment::Left, vertical);
        return (effective != physical_column_alignment)
            .then(|| format!("left + {}", typst_vertical_alignment(vertical)));
    }

    if source_column_alignment != physical_column_alignment {
        return Some(format!(
            "{} + {}",
            typst_horizontal_alignment(horizontal),
            typst_vertical_alignment(vertical)
        ));
    }

    match (cell.halign(), cell.valign()) {
        (Some(horizontal), Some(vertical)) => Some(format!(
            "{} + {}",
            typst_horizontal_alignment(horizontal),
            typst_vertical_alignment(vertical)
        )),
        (Some(horizontal), None) => Some(typst_horizontal_alignment(horizontal).to_string()),
        (None, Some(vertical)) => Some(typst_vertical_alignment(vertical).to_string()),
        (None, None) => None,
    }
}

const fn typst_horizontal_alignment(alignment: HorizontalAlignment) -> &'static str {
    match alignment {
        HorizontalAlignment::Left => "left",
        HorizontalAlignment::Center => "center",
        HorizontalAlignment::Right => "right",
    }
}

const fn typst_vertical_alignment(alignment: VerticalAlignment) -> &'static str {
    match alignment {
        VerticalAlignment::Top => "top",
        VerticalAlignment::Middle => "horizon",
        VerticalAlignment::Bottom => "bottom",
    }
}

pub(crate) fn is_collapsible_example(
    attributes: Option<AttributesRef<'_>>,
    kind: Option<CaptionKind>,
) -> bool {
    kind == Some(CaptionKind::Example) && has_attribute_token(attributes, "options", "collapsible")
}

pub(crate) fn collapse_source_whitespace(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let needs_collapsing = bytes
        .iter()
        .any(|byte| matches!(byte, b'\t' | b'\n' | b'\r'))
        || bytes.windows(2).any(|pair| pair == b"  ");
    if !needs_collapsing {
        return Cow::Borrowed(text);
    }

    let mut collapsed = String::with_capacity(text.len());
    let mut previous_was_whitespace = false;
    for character in text.chars() {
        if matches!(character, ' ' | '\t' | '\n' | '\r') {
            if !previous_was_whitespace {
                collapsed.push(' ');
                previous_was_whitespace = true;
            }
        } else {
            collapsed.push(character);
            previous_was_whitespace = false;
        }
    }
    Cow::Owned(collapsed)
}

fn code_link_content(node: InlineRef<'_>) -> Option<ContentRef<'_>> {
    if let Some(link) = node.as_hyperlink() {
        Some(link.text())
    } else if let InlineKind::CrossReference(xref) = node.kind() {
        Some(xref.text())
    } else {
        None
    }
}

fn code_output_extension<'ctx>(traversal: &'ctx TraversalContext<'_>) -> &'ctx str {
    traversal
        .get("relfilesuffix")
        .and_then(|value| value.text())
        .or_else(|| {
            traversal
                .get("outfilesuffix")
                .and_then(|value| value.text())
        })
        .unwrap_or("pdf")
        .trim_start_matches('.')
}

fn prose_whitespace(text: &str, pre_wrap: bool) -> Cow<'_, str> {
    if !pre_wrap || !text.contains("  ") {
        return collapse_source_whitespace(text);
    }

    let mut protected = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        protected.push(character);
        if character == ' ' && characters.peek() == Some(&' ') {
            protected.push('\u{200b}');
        }
    }
    Cow::Owned(collapse_source_whitespace(&protected).into_owned())
}

fn long_unbreakable_ranges(text: &str, max_columns: usize) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut run_start = None;
    let mut run_columns = 0;

    for (index, character) in text.char_indices() {
        if character.is_alphanumeric()
            || character == '_'
            || UnicodeWidthChar::width(character) == Some(0)
        {
            run_start.get_or_insert(index);
            run_columns += UnicodeWidthChar::width(character).unwrap_or(0);
        } else {
            if run_columns > max_columns {
                ranges.push(run_start.unwrap_or(index)..index);
            }
            run_start = None;
            run_columns = 0;
        }
    }
    if run_columns > max_columns {
        ranges.push(run_start.unwrap_or(text.len())..text.len());
    }
    ranges
}

fn wrap_code_text(text: &str, max_columns: usize, tab_size: usize) -> Cow<'_, str> {
    if text
        .split('\n')
        .all(|line| code_line_width(line, tab_size) <= max_columns)
    {
        return Cow::Borrowed(text);
    }

    let mut wrapped = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let (line, newline) = line
            .strip_suffix('\n')
            .map_or((line, false), |line| (line, true));
        wrap_code_line(&mut wrapped, line, max_columns, tab_size);
        if newline {
            wrapped.push('\n');
        }
    }
    Cow::Owned(wrapped)
}

fn source_gutter_tenths(source: &str, options: &SourceLineOptions) -> usize {
    options.line_number_start.map_or(0, |start| {
        let last = start.saturating_add(source_line_count(source).saturating_sub(1));
        last.to_string().len().saturating_mul(6)
    })
}

fn wrap_code_text_with_line_origins(
    text: &str,
    max_columns: usize,
    tab_size: usize,
) -> (Cow<'_, str>, Vec<usize>) {
    if text
        .split('\n')
        .all(|line| code_line_width(line, tab_size) <= max_columns)
    {
        let source_lines = (1..=source_line_count(text)).collect();
        return (Cow::Borrowed(text), source_lines);
    }

    let mut wrapped = String::with_capacity(text.len());
    let mut source_lines = Vec::new();
    let line_count = source_line_count(text);
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            wrapped.push('\n');
        }
        let visual_lines = wrap_code_line(&mut wrapped, line, max_columns, tab_size);
        if index < line_count {
            source_lines.extend(std::iter::repeat_n(index + 1, visual_lines));
        }
    }
    (Cow::Owned(wrapped), source_lines)
}

fn expand_code_tabs(text: &str, tab_size: usize) -> Cow<'_, str> {
    if !text.contains('\t') {
        return Cow::Borrowed(text);
    }

    let mut expanded = String::with_capacity(text.len());
    let mut column = 0;
    for character in text.chars() {
        match character {
            '\t' => {
                let spaces = tab_size - column % tab_size;
                expanded.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\n' => {
                expanded.push(character);
                column = 0;
            }
            _ => {
                expanded.push(character);
                column += UnicodeWidthChar::width(character).unwrap_or(0);
            }
        }
    }
    Cow::Owned(expanded)
}

fn wrap_code_line(
    output: &mut String,
    mut line: &str,
    max_columns: usize,
    tab_size: usize,
) -> usize {
    let mut visual_lines = 1;
    while !line.is_empty() {
        let mut columns = 0;
        let mut hard_break = line.len();
        let mut whitespace_break = None;
        let mut overflowed = false;
        for (index, character) in line.char_indices() {
            let width = code_character_width(character, columns, tab_size);
            if columns + width > max_columns {
                hard_break = index;
                overflowed = true;
                break;
            }
            columns += width;
            if character.is_whitespace() && columns >= max_columns / 2 {
                whitespace_break = Some(index + character.len_utf8());
            }
        }

        if !overflowed {
            output.push_str(line);
            return visual_lines;
        }

        let split = whitespace_break.unwrap_or(hard_break);
        let split = if split == 0 {
            line.chars().next().map_or(0, char::len_utf8)
        } else {
            split
        };
        output.push_str(&line[..split]);
        output.push('\n');
        visual_lines += 1;
        line = &line[split..];
    }
    visual_lines
}

fn code_line_width(line: &str, tab_size: usize) -> usize {
    line.chars().fold(0, |columns, character| {
        columns + code_character_width(character, columns, tab_size)
    })
}

fn code_character_width(character: char, column: usize, tab_size: usize) -> usize {
    if character == '\t' {
        tab_size - column % tab_size
    } else {
        UnicodeWidthChar::width(character).unwrap_or(0)
    }
}

fn transform_code_text(
    text: &mut String,
    positions: &mut [(usize, usize)],
    transform: impl Fn(&str) -> Cow<'_, str>,
) {
    if let Cow::Owned(transformed) = transform(text) {
        for (offset, _) in positions {
            *offset = transform(&text[..*offset]).len();
        }
        *text = transformed;
    }
}

fn is_xml_callout<'inline>(
    mut nodes: impl Iterator<Item = InlineRef<'inline>> + Clone,
    index: usize,
) -> bool {
    matches!(nodes.clone().nth(index).map(InlineRef::kind), Some(InlineKind::Callout(_)))
        && index.checked_sub(1).is_some_and(|previous| matches!(nodes.clone().nth(previous).map(InlineRef::kind), Some(InlineKind::Verbatim(text)) if text.text().ends_with("<!--")))
        && index.checked_add(1).is_some_and(|next| matches!(nodes.nth(next).map(InlineRef::kind), Some(InlineKind::Verbatim(text)) if text.text().starts_with("-->")))
}

fn strip_pdf_callout_guard<'text>(text: &'text str, prefix: Option<&str>) -> &'text str {
    let before_guard = text.strip_suffix(' ').unwrap_or(text);
    match prefix {
        Some("") => text,
        // A trailing space may belong to the prefix rather than its optional separator.
        Some(prefix) => before_guard
            .strip_suffix(prefix)
            .or_else(|| text.strip_suffix(prefix))
            .unwrap_or(text),
        None => ["//", "#", "--", ";;"]
            .iter()
            .find_map(|guard| before_guard.strip_suffix(*guard))
            .unwrap_or(text),
    }
}

fn asciidoctor_foreground_colour(role: &str) -> Option<&'static str> {
    match role {
        "aqua" => Some("#00bfbf"),
        "black" => Some("#000000"),
        "blue" => Some("#0000bf"),
        "fuchsia" => Some("#bf00bf"),
        "gray" => Some("#606060"),
        "green" => Some("#006000"),
        "lime" => Some("#00bf00"),
        "maroon" => Some("#600000"),
        "navy" => Some("#000060"),
        "olive" => Some("#606000"),
        "purple" => Some("#600060"),
        "red" => Some("#bf0000"),
        "silver" => Some("#909090"),
        "teal" => Some("#006060"),
        "white" => Some("#bfbfbf"),
        "yellow" => Some("#bfbf00"),
        _ => None,
    }
}

fn asciidoctor_background_colour(role: &str) -> Option<&'static str> {
    match role {
        "aqua-background" => Some("#00fafa"),
        "black-background" => Some("#000000"),
        "blue-background" => Some("#0000fa"),
        "fuchsia-background" => Some("#fa00fa"),
        "gray-background" => Some("#7d7d7d"),
        "green-background" => Some("#007d00"),
        "lime-background" => Some("#00fa00"),
        "maroon-background" => Some("#7d0000"),
        "navy-background" => Some("#00007d"),
        "olive-background" => Some("#7d7d00"),
        "purple-background" => Some("#7d007d"),
        "red-background" => Some("#fa0000"),
        "silver-background" => Some("#bcbcbc"),
        "teal-background" => Some("#007d7d"),
        "white-background" => Some("#fafafa"),
        "yellow-background" => Some("#fafa00"),
        _ => None,
    }
}

fn attribute_tokens<'doc>(
    attributes: Option<AttributesRef<'doc>>,
    name: &str,
) -> impl Iterator<Item = &'doc str> {
    attributes
        .and_then(|attributes| match attributes.get(name) {
            Some(AttributeValueRef::Tokens(tokens)) => Some(tokens),
            _ => None,
        })
        .into_iter()
        .flat_map(TokensRef::iter)
}

fn attribute_content<'doc>(
    attributes: Option<AttributesRef<'doc>>,
    name: &str,
) -> Option<ContentRef<'doc>> {
    match attributes.and_then(|attributes| attributes.get(name)) {
        Some(AttributeValueRef::Content(content)) => Some(content),
        _ => None,
    }
}

fn has_attribute_token(attributes: Option<AttributesRef<'_>>, name: &str, token: &str) -> bool {
    matches!(attributes.and_then(|attributes| attributes.get(name)), Some(AttributeValueRef::Tokens(tokens)) if tokens.contains(token))
}

fn attribute_style(attributes: Option<AttributesRef<'_>>) -> Option<&str> {
    attributes.and_then(|attributes| attribute_text(attributes, "style"))
}

pub(super) fn attribute_text<'doc>(
    attributes: AttributesRef<'doc>,
    name: &str,
) -> Option<&'doc str> {
    if let Some(AttributeValueRef::Text(value)) = attributes.get(name) {
        Some(value)
    } else {
        None
    }
}

fn inline_image_fallback_text(image: InlineImageRef<'_>) -> String {
    image.alt().map_or_else(
        || format!("[image: {}]", image.source()),
        |content| {
            InlineTextTransform::default()
                .decode_char_refs(true)
                .content_to_string(content)
        },
    )
}

pub(crate) fn builtin_icon_glyph(name: &str) -> Option<&'static str> {
    match name {
        "arrow-down" => Some("↓"),
        "arrow-left" => Some("←"),
        "arrow-right" => Some("→"),
        "arrow-up" => Some("↑"),
        "check" => Some("✓"),
        "circle-info" | "info-circle" | "info" => Some("ⓘ"),
        "exclamation-triangle" | "triangle-exclamation" | "warning" => Some("⚠"),
        "fire" => Some("🔥"),
        "heart" => Some("♥"),
        "lightbulb" | "lightbulb-o" => Some("💡"),
        "minus" => Some("−"),
        "plus" => Some("+"),
        "question" | "circle-question" | "question-circle" => Some("?"),
        "star" => Some("★"),
        "times" | "xmark" => Some("×"),
        _ => None,
    }
}

fn block_image_alt(image: ImageRef<'_>) -> String {
    acdc_converters_core::media::image_alt_from_content(image.source(), image.alt())
}

fn block_image_alignment(attributes: Option<AttributesRef<'_>>) -> BlockImageAlignment {
    if let Some(side) = block_image_float(attributes) {
        return side;
    }
    if attributes.is_some_and(|attributes| attributes.contains("align")) {
        return attributes
            .and_then(|attributes| attribute_text(attributes, "align"))
            .and_then(BlockImageAlignment::from_name)
            .unwrap_or_default();
    }
    attribute_tokens(attributes, "role")
        .filter_map(BlockImageAlignment::from_name)
        .last()
        .unwrap_or_default()
}

fn block_image_float(attributes: Option<AttributesRef<'_>>) -> Option<BlockImageAlignment> {
    attributes
        .and_then(|attributes| attribute_text(attributes, "float"))
        .and_then(BlockImageAlignment::from_name)
        .filter(|alignment| *alignment != BlockImageAlignment::Center)
}

fn image_width(attributes: AttributesRef<'_>, supports_viewport_width: bool) -> Option<ImageWidth> {
    if let Some(value) = attributes.get("pdfwidth") {
        let value = if let AttributeValueRef::Text(value) = value {
            value
        } else {
            ""
        };
        return Some(pdf_image_width(value, supports_viewport_width));
    }

    if let Some(scale) = attribute_text(attributes, "scale") {
        return Some(ImageWidth::IntrinsicRatio(leading_number(scale) / 100.0));
    }

    if let Some(scaledwidth) = attribute_text(attributes, "scaledwidth") {
        if let Some(percentage) = scaledwidth.strip_suffix('%') {
            return Some(ImageWidth::ContainerRatio {
                value: leading_number(percentage) / 100.0,
                constrain_to_bounds: false,
            });
        }
        if scaledwidth.bytes().all(|byte| byte.is_ascii_digit()) {
            return Some(ImageWidth::ContainerRatio {
                value: leading_number(scaledwidth) / 100.0,
                constrain_to_bounds: false,
            });
        }
        return Some(ImageWidth::Points {
            value: measurement_points(scaledwidth),
            constrain_to_bounds: false,
        });
    }

    let width = attribute_text(attributes, "width")?;
    if let Some(percentage) = width.strip_suffix('%') {
        let percentage = percentage.parse::<f64>().ok()?;
        return (percentage.is_finite() && percentage >= 0.0).then(|| ImageWidth::ContainerRatio {
            value: (percentage / 100.0).min(1.0),
            constrain_to_bounds: true,
        });
    }
    if width.is_empty() || !width.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let pixels = width.parse::<f64>().ok()?;
    pixels.is_finite().then_some(ImageWidth::Points {
        value: pixels * 0.75,
        constrain_to_bounds: true,
    })
}

fn pdf_image_width(width: &str, supports_viewport_width: bool) -> ImageWidth {
    if let Some(percentage) = width.strip_suffix('%') {
        return ImageWidth::ContainerRatio {
            value: leading_number(percentage) / 100.0,
            constrain_to_bounds: false,
        };
    }
    if let Some(percentage) = width.strip_suffix("iw") {
        return ImageWidth::IntrinsicRatio(leading_number(percentage) / 100.0);
    }
    if supports_viewport_width && let Some(percentage) = width.strip_suffix("vw") {
        return ImageWidth::ViewportRatio(leading_number(percentage) / 100.0);
    }

    ImageWidth::Points {
        value: measurement_points(width),
        constrain_to_bounds: false,
    }
}

fn measurement_points(value: &str) -> f64 {
    const POINTS_PER_INCH: f64 = 72.0;
    let units = [
        ("pt", 1.0),
        ("px", 0.75),
        ("in", POINTS_PER_INCH),
        ("cm", POINTS_PER_INCH / 2.54),
        ("mm", POINTS_PER_INCH / 25.4),
        ("pc", 12.0),
    ];
    for (suffix, factor) in units {
        if let Some(number) = value.strip_suffix(suffix) {
            return leading_number(number) * factor;
        }
    }
    leading_number(value)
}

fn leading_number(value: &str) -> f64 {
    let bytes = value.as_bytes();
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let mut saw_digit = false;
    let mut saw_dot = false;

    while let Some(byte) = bytes.get(end) {
        match byte {
            b'0'..=b'9' => saw_digit = true,
            b'.' if !saw_dot => saw_dot = true,
            _ => break,
        }
        end += 1;
    }

    if !saw_digit {
        return 0.0;
    }
    value[..end]
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
        .unwrap_or(0.0)
}

fn literal_table_cell_text<'doc>(blocks: impl Iterator<Item = BlockRef<'doc>>) -> Option<String> {
    let mut text = String::new();
    for (index, block) in blocks.enumerate() {
        let BlockKind::Paragraph(paragraph) = block.kind() else {
            return None;
        };
        if index > 0 {
            text.push_str("\n\n");
        }
        let transform = InlineTextTransform::default()
            .decode_char_refs(true)
            .line_break("\n");
        let _ = transform.write_inlines(&mut text, paragraph.content().inlines());
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::{
        BlockImageAlignment, ImageWidth, block_image_alignment, expand_code_tabs, image_width,
        long_unbreakable_ranges, measurement_points, prose_whitespace, wrap_code_text,
        wrap_code_text_with_line_origins,
    };
    use acdc_parser::{AttributeValue, Image, Location, Source, ast::ImageRef};

    fn image_with_width(width: &'static str) -> acdc_parser::Image<'static> {
        let mut image = acdc_parser::Image::new(
            acdc_parser::Source::Name("bird"),
            acdc_parser::Location::default(),
        );
        image
            .metadata
            .attributes
            .set("width".into(), AttributeValue::String(width.into()));
        image
    }

    fn assert_measurement(input: &str, expected: f64) {
        let actual = measurement_points(input);
        assert!(
            (actual - expected).abs() < 1e-9,
            "measurement {input:?} resolved to {actual}, expected {expected}"
        );
    }

    #[test]
    fn image_width_matches_asciidoctor_pdf_units() {
        assert_eq!(
            image_width(ImageRef::from(&image_with_width("120")).attributes(), true),
            Some(ImageWidth::Points {
                value: 90.0,
                constrain_to_bounds: true,
            })
        );
        assert_eq!(
            image_width(ImageRef::from(&image_with_width("40%")).attributes(), true),
            Some(ImageWidth::ContainerRatio {
                value: 0.4,
                constrain_to_bounds: true,
            })
        );
        assert_eq!(
            image_width(ImageRef::from(&image_with_width("150%")).attributes(), true),
            Some(ImageWidth::ContainerRatio {
                value: 1.0,
                constrain_to_bounds: true,
            })
        );
        for ignored in ["120px", "12.5", "-1", "invalid%"] {
            assert_eq!(
                image_width(
                    ImageRef::from(&image_with_width(ignored)).attributes(),
                    true
                ),
                None
            );
        }
    }

    #[test]
    fn pre_wrap_preserves_each_repeated_space() {
        assert_eq!(
            prose_whitespace("two  spaces   together", true),
            "two \u{200b} spaces \u{200b} \u{200b} together"
        );
        assert_eq!(
            prose_whitespace("two  spaces   together", false),
            "two spaces together"
        );
    }

    #[test]
    fn pdfwidth_overrides_width_and_uses_pdf_measurements() {
        let mut image = image_with_width("40");
        image
            .metadata
            .attributes
            .set("pdfwidth".into(), AttributeValue::String("60".into()));
        assert_eq!(
            image_width(ImageRef::from(&image).attributes(), true),
            Some(ImageWidth::Points {
                value: 60.0,
                constrain_to_bounds: false,
            })
        );

        for (input, expected) in [
            ("40pt", 40.0),
            ("40px", 30.0),
            ("1in", 72.0),
            ("2.54cm", 72.0),
            ("25.4mm", 72.0),
            ("6pc", 72.0),
            ("12.5", 12.5),
            ("40unknown", 40.0),
            ("invalid", 0.0),
        ] {
            assert_measurement(input, expected);
        }
    }

    #[test]
    fn pdf_image_scale_attributes_follow_reference_precedence() {
        let mut image = image_with_width("120");
        image
            .metadata
            .attributes
            .set("scaledwidth".into(), AttributeValue::String("40%".into()));
        image
            .metadata
            .attributes
            .set("scale".into(), AttributeValue::String("25".into()));
        image
            .metadata
            .attributes
            .set("pdfwidth".into(), AttributeValue::String("72pt".into()));

        assert_eq!(
            image_width(ImageRef::from(&image).attributes(), true),
            Some(ImageWidth::Points {
                value: 72.0,
                constrain_to_bounds: false,
            })
        );
        image.metadata.attributes.remove("pdfwidth");
        assert_eq!(
            image_width(ImageRef::from(&image).attributes(), true),
            Some(ImageWidth::IntrinsicRatio(0.25))
        );
        image.metadata.attributes.remove("scale");
        assert_eq!(
            image_width(ImageRef::from(&image).attributes(), true),
            Some(ImageWidth::ContainerRatio {
                value: 0.4,
                constrain_to_bounds: false,
            })
        );
        image
            .metadata
            .attributes
            .set("scaledwidth".into(), AttributeValue::String("1in".into()));
        assert_eq!(
            image_width(ImageRef::from(&image).attributes(), true),
            Some(ImageWidth::Points {
                value: 72.0,
                constrain_to_bounds: false,
            })
        );
    }

    #[test]
    fn pdfwidth_supports_container_intrinsic_and_block_viewport_units() {
        let image_with_pdfwidth = |width: &'static str| {
            let mut image = acdc_parser::Image::new(
                acdc_parser::Source::Name("bird"),
                acdc_parser::Location::default(),
            );
            image
                .metadata
                .attributes
                .set("pdfwidth".into(), AttributeValue::String(width.into()));
            image
        };

        assert_eq!(
            image_width(
                ImageRef::from(&image_with_pdfwidth("200%")).attributes(),
                true
            ),
            Some(ImageWidth::ContainerRatio {
                value: 2.0,
                constrain_to_bounds: false,
            })
        );
        assert_eq!(
            image_width(
                ImageRef::from(&image_with_pdfwidth("50iw")).attributes(),
                true
            ),
            Some(ImageWidth::IntrinsicRatio(0.5))
        );
        assert_eq!(
            image_width(
                ImageRef::from(&image_with_pdfwidth("50vw")).attributes(),
                true
            ),
            Some(ImageWidth::ViewportRatio(0.5))
        );
        assert_eq!(
            image_width(
                ImageRef::from(&image_with_pdfwidth("50vw")).attributes(),
                false
            ),
            Some(ImageWidth::Points {
                value: 50.0,
                constrain_to_bounds: false,
            })
        );
    }

    #[test]
    fn block_image_alignment_follows_float_attribute_and_role_precedence() {
        let mut image = Image::new(Source::Name("example"), Location::default());
        assert_eq!(
            block_image_alignment(Some(ImageRef::from(&image).attributes())),
            BlockImageAlignment::Left
        );

        image.metadata.roles = vec!["left", "unrelated", "right"];
        assert_eq!(
            block_image_alignment(Some(ImageRef::from(&image).attributes())),
            BlockImageAlignment::Right
        );

        image
            .metadata
            .attributes
            .set("align".into(), AttributeValue::String("center".into()));
        assert_eq!(
            block_image_alignment(Some(ImageRef::from(&image).attributes())),
            BlockImageAlignment::Center
        );

        image
            .metadata
            .attributes
            .set("align".into(), AttributeValue::String("invalid".into()));
        assert_eq!(
            block_image_alignment(Some(ImageRef::from(&image).attributes())),
            BlockImageAlignment::Left
        );

        image
            .metadata
            .attributes
            .set("float".into(), AttributeValue::String("right".into()));
        assert_eq!(
            block_image_alignment(Some(ImageRef::from(&image).attributes())),
            BlockImageAlignment::Right
        );

        image
            .metadata
            .attributes
            .set("float".into(), AttributeValue::String("invalid".into()));
        image
            .metadata
            .attributes
            .set("align".into(), AttributeValue::String("center".into()));
        assert_eq!(
            block_image_alignment(Some(ImageRef::from(&image).attributes())),
            BlockImageAlignment::Center
        );
    }

    #[test]
    fn table_break_ranges_select_only_long_uninterrupted_runs() {
        let ranges = long_unbreakable_ranges("short abcdefghij after", 8);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges.first(), Some(&(6..16)));
        assert_eq!(
            long_unbreakable_ranges("one two three", 8),
            [] as [std::ops::Range<usize>; 0]
        );
    }

    #[test]
    fn code_wrapping_preserves_whitespace_and_terminal_newlines() {
        assert_eq!(
            wrap_code_text("alpha    beta\n\n", 10, 4),
            "alpha    \nbeta\n\n"
        );
    }

    #[test]
    fn code_wrapping_breaks_long_tokens_at_character_boundaries() {
        assert_eq!(wrap_code_text("abcdefghijk", 5, 4), "abcde\nfghij\nk");
        assert_eq!(wrap_code_text("abcd界ef", 5, 4), "abcd\n界ef");
    }

    #[test]
    fn code_tabs_expand_to_configured_tab_stops() {
        assert_eq!(expand_code_tabs("a\tb", 4), "a   b");
        assert_eq!(expand_code_tabs("TABX\tvalue", 4), "TABX    value");
        assert_eq!(expand_code_tabs("a\tb", 8), "a       b");
    }

    #[test]
    fn code_wrapping_tracks_the_source_line_for_each_visual_line() {
        let (text, source_lines) = wrap_code_text_with_line_origins("abcdefgh\nxy\n", 5, 4);

        assert_eq!(text, "abcde\nfgh\nxy\n");
        assert_eq!(source_lines, [1, 1, 2]);
    }
}
