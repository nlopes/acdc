use std::{
    borrow::Cow, collections::HashSet, fmt::Write as _, num::NonZeroU32, ops::Range, rc::Rc,
};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::{SubsFlags, apply_replacements, effective_subs_flags};
use acdc_converters_core::{
    Diagnostics, Doctype, InlineTextTransform,
    code::{SourceLineOptions, detect_language, source_line_count},
    icon::{IconMode, alt as icon_alt, image_source as icon_image_source},
    inlines_to_string,
    link::{autolink_fallback, link_fallback, mailto_fallback},
    list::OrderedListNumbering,
    section::effective_section_level,
    shows_block_title,
    substitutions::{Replacements, TextBoundaries},
    table::{CellKind, GridRow, build_grid, determine_column_count},
    toc::{Config as TocConfig, NumberingConfig, effective_level, has_real_parts, section_numbers},
    visitor::Visitor,
    xref::{XrefDisplay, interdocument_xref, resolve_xref},
};
use acdc_parser::{
    Anchor, AttributeValue, Autolink, Block, BlockMetadata, Caption, CaptionKind, ColumnStyle,
    ColumnWidth, CrossReference, DelimitedBlock, DelimitedBlockType, DescriptionList,
    DescriptionListItem, ElementAttributes, HorizontalAlignment, Icon, Image, IndexTerm,
    InlineMacro, InlineNode, ListItem, Menu, Paragraph, Raw, Section, SectionKind, Source,
    Substitution, Table, TableColumn, TableFrame, TableGrid, TableOfContents, TablePresentation,
    TableStripes, Title, TocEntry, VerticalAlignment,
};
use acdc_pdf_images::ImageMap;
use acdc_pdf_theme::{
    Heading, PageBreakBefore, Palette, PartBreakAfter, Table as TableTheme, TableAlignment, Theme,
};
use acdc_pdf_typst::Writer;
use unicode_width::UnicodeWidthChar;

use crate::{
    Error, Processor, encode_bibliography_reference_label, encode_footnote_label, encode_label,
    has_autofit_option,
    index::{CatalogTerm, IndexCatalog, PageSequenceStyle},
};

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
    assets: &'m ImageMap,
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
    pub(crate) in_article_abstract: bool,
    pub(crate) automatic_preamble_lead_state: AutomaticPreambleLeadState,
    table_cell_section_state: TableCellSectionState,
    pub(crate) doctype: Doctype,
    book_page_break_state: BookPageBreakState,
    pub(crate) explicit_page_break_state: ExplicitPageBreakState,
    text_boundaries: TextBoundaries,
    toc_entries: Vec<TocEntry<'a>>,
    toc_written: bool,
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

struct TableRenderContext<'table, 'source> {
    table: &'table Table<'source>,
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
    fn from_metadata(metadata: &BlockMetadata<'_>) -> Option<Self> {
        metadata.roles.iter().rev().find_map(|role| match *role {
            "text-left" => Some(Self::Left),
            "text-center" => Some(Self::Center),
            "text-right" => Some(Self::Right),
            "text-justify" => Some(Self::Justify),
            _ => None,
        })
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
        theme: &'m Theme,
        page_width_pt: f64,
        code_wrap_columns: usize,
        toc_entries: Vec<TocEntry<'a>>,
        diagnostics: Diagnostics<'d>,
    ) -> Self {
        let doctype = processor
            .document_attributes()
            .get_string("doctype")
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| processor.options().doctype());
        let is_book = doctype == Doctype::Book;
        let chapter_signifier = if is_book {
            match processor.document_attributes().get("chapter-signifier") {
                Some(AttributeValue::String(signifier)) if !signifier.is_empty() => {
                    Some(signifier.clone().into_owned())
                }
                Some(_) => None,
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
        Self {
            writer: Writer::new(),
            processor,
            assets,
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
            in_article_abstract: false,
            automatic_preamble_lead_state: AutomaticPreambleLeadState::Inactive,
            table_cell_section_state: TableCellSectionState::Outside,
            doctype,
            book_page_break_state,
            explicit_page_break_state: ExplicitPageBreakState::Inactive,
            text_boundaries: TextBoundaries::BOTH,
            toc_entries,
            toc_written: false,
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

    pub(crate) fn write_index_catalog(&mut self) {
        let sequence_style = PageSequenceStyle::from_attributes(
            self.processor
                .document_attributes()
                .get_string("index-pagenum-sequence-style")
                .as_deref(),
            self.processor
                .document_attributes()
                .get_string("media")
                .as_deref(),
        );
        self.index_catalog.write(
            &mut self.writer,
            sequence_style,
            self.index_columns,
            self.index_column_gap_pt,
        );
    }

    pub(crate) fn section_break_before(&mut self, section: &Section<'_>) -> bool {
        if self.book_page_break_state == BookPageBreakState::Disabled {
            return false;
        }

        if section.level == 0 && section.kind == SectionKind::Normal {
            self.book_page_break_state = BookPageBreakState::AfterPart;
            return self.heading.part.break_before == PageBreakBefore::Always;
        }

        if effective_section_level(section.level, section.kind) == 1 {
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

        if section.level == 0 {
            self.book_page_break_state = BookPageBreakState::Enabled;
        }
        false
    }

    pub(crate) fn render_toc(
        &mut self,
        toc_macro: Option<&TableOfContents<'_>>,
        placement: &str,
    ) -> Result<(), Error> {
        if self.toc_written || self.toc_entries.is_empty() {
            return Ok(());
        }

        let config = TocConfig::from_attributes(toc_macro, self.processor.document_attributes());
        let configured_placement =
            if config.placement() == "none" && self.processor.pdf_options().toc {
                "auto"
            } else {
                config.placement()
            };
        let should_render = match placement {
            "auto" => matches!(
                configured_placement,
                "auto" | "left" | "right" | "top" | "bottom"
            ),
            other => configured_placement == other,
        };
        if !should_render {
            return Ok(());
        }

        self.toc_written = true;
        let suppress_header =
            toc_macro.is_some_and(|toc| toc.metadata.options.contains(&"noheader"));
        let suppress_footer =
            toc_macro.is_some_and(|toc| toc.metadata.options.contains(&"nofooter"));
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
            "#let _acdc_toc_entry(target, depth, body) = context {\n  link(\n    target,\n    pad(\n      left: depth * 1.25em,\n      grid(\n        columns: (auto, 1fr, auto),\n        column-gutter: 0.5em,\n        body,\n        repeat[.],\n        str(counter(page).at(target).first()),\n      ),\n    ),\n  )\n}\n",
        );

        let entries = self.toc_entries.clone();
        let numbers = section_numbers(
            &entries,
            &NumberingConfig::new(self.processor.document_attributes(), None),
        );
        let has_parts = has_real_parts(&entries);
        let mut root = TocRoot::None;
        let mut hidden_article_abstract_level = None;
        for (entry, number) in entries.iter().zip(numbers) {
            if let Some(abstract_level) = hidden_article_abstract_level {
                if entry.level > abstract_level {
                    continue;
                }
                hidden_article_abstract_level = None;
            }
            if self.doctype == Doctype::Article && entry.kind == SectionKind::Abstract {
                hidden_article_abstract_level = Some(entry.level);
                continue;
            }
            if entry.kind == SectionKind::Index && !self.populated_index_sections.contains(entry.id)
            {
                continue;
            }
            if entry.level > config.levels() {
                continue;
            }
            let depth = match entry.level {
                0 => {
                    root = if entry.kind == SectionKind::Normal {
                        TocRoot::Part
                    } else {
                        TocRoot::Special
                    };
                    0
                }
                _ if !has_parts || root != TocRoot::Part => {
                    effective_level(entry, has_parts).saturating_sub(1)
                }
                level => level,
            };
            let _ = write!(
                self.writer,
                "#_acdc_toc_entry(<{}>, {depth}, [",
                crate::encode_label(entry.id)
            );
            if let Some(number) = number {
                self.write_text_expr(&number);
            }
            self.write_title_without_recording_index_terms(&entry.title)?;
            self.writer.raw("])\n");
        }
        self.writer.raw("#pagebreak()\n\n");
        if suppress_header || suppress_footer {
            self.writer.raw("]\n\n");
        }
        Ok(())
    }

    pub(crate) fn write_blocks(&mut self, blocks: &[Block<'_>]) -> Result<(), Error> {
        for block in blocks {
            self.visit_block(block)?;
        }
        Ok(())
    }

    pub(crate) fn write_delimited_block_content(
        &mut self,
        block: &DelimitedBlock<'_>,
    ) -> Result<(), Error> {
        let fallback = CaptionKind::for_delimited(&block.inner, block.metadata.style);
        let collapsible_example = is_collapsible_example(&block.metadata, fallback);
        let zero_width_table = matches!(block.inner, DelimitedBlockType::DelimitedTable(_))
            && self.omit_zero_width_table(&block.metadata);
        if zero_width_table {
            return Ok(());
        }
        let table = matches!(block.inner, DelimitedBlockType::DelimitedTable(_));
        let intrinsic_table =
            table && !block.title.is_empty() && Self::table_has_intrinsic_width(&block.metadata);
        if intrinsic_table {
            self.write_intrinsic_table_start();
        }
        let (sized_table, aligned_table) =
            self.write_table_wrappers_start(&block.metadata, table && !intrinsic_table);
        let writes_own_title = matches!(block.inner, DelimitedBlockType::DelimitedSidebar(_))
            || matches!(block.inner, DelimitedBlockType::DelimitedOpen(_))
                && block.metadata.style == Some("abstract")
            || collapsible_example;
        // A block built through the API carries no resolved caption, so classify it with
        // the same rules the parser used.
        let captioned = block.metadata.caption.is_some() || fallback.is_some();
        if shows_block_title(&block.inner) && !writes_own_title && !intrinsic_table {
            let sticky_title = table
                && block.metadata.options.contains(&"breakable")
                && !block.metadata.options.contains(&"unbreakable")
                && !block.title.is_empty();
            if sticky_title {
                self.writer
                    .raw("#block(sticky: true, above: 0pt, below: 0pt)[\n");
            }
            if captioned {
                self.write_captioned_title(&block.title, &block.metadata, fallback)?;
            } else {
                self.write_block_title(&block.title)?;
            }
            if sticky_title {
                self.writer.raw("]\n");
            }
        }
        let result = match &block.inner {
            DelimitedBlockType::DelimitedExample(blocks)
            | DelimitedBlockType::DelimitedOpen(blocks)
                if collapsible_example =>
            {
                self.write_disclosure(&block.title, |visitor| visitor.write_blocks(blocks))
            }
            // Each container reads differently in print: an example takes a
            // light frame, a sidebar a shaded box, and an open block is a
            // transparent container that takes neither.
            DelimitedBlockType::DelimitedExample(blocks) => self.write_example(blocks),
            DelimitedBlockType::DelimitedSidebar(blocks) => {
                self.write_sidebar(&block.title, blocks)
            }
            DelimitedBlockType::DelimitedOpen(blocks)
                if block.metadata.style == Some("abstract") =>
            {
                self.write_abstract(Some(&block.title), &block.metadata, |visitor| {
                    visitor.write_blocks(blocks)
                })
            }
            DelimitedBlockType::DelimitedOpen(blocks) => {
                self.write_framed_blocks(None, None, blocks)
            }
            DelimitedBlockType::DelimitedQuote(blocks) => {
                self.write_quote_block(&block.metadata, |visitor| visitor.write_blocks(blocks))
            }
            DelimitedBlockType::DelimitedVerse(nodes) => {
                self.write_verse_block(nodes, &block.metadata)
            }
            DelimitedBlockType::DelimitedListing(nodes)
            | DelimitedBlockType::DelimitedLiteral(nodes) => {
                self.write_verbatim_block(nodes, &block.metadata);
                Ok(())
            }
            DelimitedBlockType::DelimitedPass(nodes) => {
                self.write_passthrough_block(nodes);
                Ok(())
            }
            DelimitedBlockType::DelimitedTable(table) => self.write_table(table, &block.metadata),
            DelimitedBlockType::DelimitedStem(stem) => {
                self.write_stem_fallback(stem.content, true);
                Ok(())
            }
            DelimitedBlockType::DelimitedComment(_) => Ok(()),
            _ => {
                self.warn_unsupported_parser_variant("delimited block");
                Ok(())
            }
        };
        if intrinsic_table {
            self.write_intrinsic_table_end(&block.title, &block.metadata, fallback)?;
        }
        self.write_table_wrappers_end(sized_table, aligned_table);
        result
    }

    pub(crate) fn write_title(&mut self, title: &Title<'_>) -> Result<(), Error> {
        if !title.is_empty() {
            self.write_inlines(title.as_ref())?;
        }
        Ok(())
    }

    fn write_title_without_recording_index_terms(
        &mut self,
        title: &Title<'_>,
    ) -> Result<(), Error> {
        let previous = self.index_catalog.set_suspended(true);
        let result = self.write_title(title);
        self.index_catalog.set_suspended(previous);
        result
    }

    pub(crate) fn write_inlines(&mut self, nodes: &[InlineNode<'_>]) -> Result<(), Error> {
        let previous_boundaries = self.text_boundaries;
        let last = nodes.len().saturating_sub(1);
        let result = (|| {
            for (index, node) in nodes.iter().enumerate() {
                let follows_break =
                    index > 0 && matches!(nodes.get(index - 1), Some(InlineNode::LineBreak(_)));
                let precedes_break = matches!(nodes.get(index + 1), Some(InlineNode::LineBreak(_)));
                self.text_boundaries = TextBoundaries::new(
                    follows_break
                        || (!self.in_inline_span
                            && previous_boundaries.at_paragraph_start()
                            && index == 0),
                    precedes_break
                        || (!self.in_inline_span
                            && previous_boundaries.at_paragraph_end()
                            && index == last),
                );
                self.visit_inline_node(node)?;
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

    pub(crate) fn write_inline_verbatim(&mut self, text: &str) {
        if let Some(max_columns) = self.table_cell_text_break_columns() {
            self.write_table_text_expr(text, max_columns, TableTextKind::InlineVerbatim);
        } else {
            self.write_text_or_raw_expr(text, TableTextKind::InlineVerbatim);
        }
    }

    fn write_table_literal(&mut self, text: &str) {
        let max_columns = self.table_cell_text_break_columns().unwrap_or(usize::MAX);
        self.write_table_text_expr(text, max_columns, TableTextKind::Literal);
    }

    fn write_table_text_expr(&mut self, text: &str, max_columns: usize, kind: TableTextKind) {
        let ranges = long_unbreakable_ranges(text, max_columns);
        if ranges.is_empty() {
            self.write_text_or_raw_expr(text, kind);
            return;
        }

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
        if previous_end < text.len() {
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

    pub(crate) fn write_block_anchor(&mut self, metadata: &BlockMetadata<'_>) {
        if let Some(anchor) = metadata.id.as_ref().or_else(|| metadata.anchors.first()) {
            self.write_anchor_target(anchor);
        }
    }

    pub(crate) fn write_anchor_target(&mut self, anchor: &Anchor<'_>) {
        let _ = writeln!(
            self.writer,
            "#metadata(none) <{}>",
            crate::encode_label(anchor.id)
        );
    }

    pub(crate) fn write_inline_span_start(
        &mut self,
        id: Option<&str>,
        role: Option<&str>,
    ) -> InlineSpanState {
        if let Some(id) = id {
            let _ = write!(self.writer, "#metadata(none) <{}>", crate::encode_label(id));
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
        prose_whitespace(text, self.pre_wrap_depth > 0)
    }

    pub(crate) fn write_plain(&mut self, text: &str) {
        #[cfg(feature = "pre-spec-subs")]
        let text = apply_replacements(
            text,
            self.processor.current_subs.get(),
            &Replacements::unicode(),
            self.text_boundaries,
        );
        #[cfg(not(feature = "pre-spec-subs"))]
        let text = Cow::Owned(Replacements::unicode().transform(text, self.text_boundaries));
        let text = prose_whitespace(&text, self.pre_wrap_depth > 0);
        self.write_text_expr(&acdc_converters_core::decode_numeric_char_refs(&text));
    }

    pub(crate) fn write_raw(&mut self, raw: &Raw<'_>) {
        let mut replacements = Replacements::unicode();
        replacements.double_arrow_right = "=>";
        replacements.double_arrow_left = "<=";
        replacements.arrow_right = "->";
        replacements.arrow_left = "<-";

        let mut text = raw.content.to_string();
        let mut applied_special_chars = false;
        for substitution in &raw.subs {
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
                    text = replacements.apply(&text, self.text_boundaries);
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

        let text = prose_whitespace(&text, self.pre_wrap_depth > 0);
        if applied_special_chars {
            self.write_text_expr(&text);
        } else {
            self.write_text_expr(&acdc_converters_core::decode_numeric_char_refs(&text));
        }
    }

    pub(crate) fn write_quoted_span(
        &mut self,
        id: Option<&str>,
        role: Option<&str>,
        prefix: &str,
        nodes: &[InlineNode<'_>],
        suffix: &str,
    ) -> Result<(), Error> {
        let state = self.write_inline_span_start(id, role);
        self.writer.raw(prefix);
        self.write_inlines(nodes)?;
        self.writer.raw(suffix);
        self.write_inline_span_end(state);
        Ok(())
    }

    /// Write a block's title above the block.
    ///
    /// The title is its own layout block, so the content that follows starts on
    /// a new line, and sits close to it rather than a paragraph apart.
    pub(crate) fn write_block_title(&mut self, title: &Title<'_>) -> Result<(), Error> {
        if title.is_empty() {
            return Ok(());
        }
        self.writer.raw("#blocktitle[");
        self.write_title(title)?;
        self.writer.raw("]\n");
        Ok(())
    }

    /// Write a collapsible example as an expanded disclosure for print.
    pub(crate) fn write_disclosure(
        &mut self,
        title: &Title<'_>,
        write_body: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.writer.raw("#block(width: 100%, below: 0.8em)[\n");
        self.writer
            .raw("#grid(columns: (0.8em, 1fr), column-gutter: 0.2em, align: top, ");
        self.writer
            .raw("[#box(width: 0.8em, height: 0.8em, baseline: 0.1em, align(center + horizon, ");
        self.writer
            .raw("rotate(90deg, origin: center, text(weight: \"bold\", size: 0.8em, \">\"))))], ");
        self.writer.raw("[#captiontext[");
        if title.is_empty() {
            self.write_text_expr("Details");
        } else {
            self.write_title(title)?;
        }
        self.writer
            .raw("]])\n#block(inset: (left: 1em), above: 0.3em)[\n");
        write_body(self)?;
        self.writer.raw("\n]\n]\n\n");
        Ok(())
    }

    /// Write a paragraph's title and content under the paragraph's effective
    /// substitutions.
    ///
    /// `write_title` is false for the synthetic paragraph inside a simple
    /// admonition, whose title the admonition wrapper already wrote.
    pub(crate) fn write_paragraph_content(
        &mut self,
        para: &Paragraph<'_>,
        write_title: bool,
        automatic_lead: bool,
    ) -> Result<(), Error> {
        #[cfg(feature = "pre-spec-subs")]
        let previous_subs = self.processor.current_subs.replace(effective_subs_flags(
            para.metadata.substitutions.as_ref(),
            matches!(
                para.metadata.style,
                Some("verse" | "literal" | "listing" | "source")
            ),
        ));

        let result = (|| {
            let fallback = CaptionKind::for_style(para.metadata.style);
            if is_collapsible_example(&para.metadata, fallback) {
                return self.write_disclosure(&para.title, |visitor| {
                    visitor.write_paragraph_alignment(&para.metadata, |visitor| {
                        visitor.write_inlines(&para.content)
                    })
                });
            }
            if para.metadata.style == Some("abstract") {
                let title = write_title.then_some(&para.title);
                return self.write_abstract(title, &para.metadata, |visitor| {
                    visitor.write_inlines(&para.content)
                });
            }
            if write_title {
                // An `[example]`, `[listing]` or `[source]` paragraph takes a caption; every
                // other paragraph takes an ordinary title. A paragraph built through the API
                // carries no resolved caption, so its style is classified here instead.
                if para.metadata.caption.is_some() || fallback.is_some() {
                    self.write_captioned_title(&para.title, &para.metadata, fallback)?;
                } else {
                    self.write_block_title(&para.title)?;
                }
            }
            self.write_paragraph_body(para, automatic_lead)
        })();

        #[cfg(feature = "pre-spec-subs")]
        self.processor.current_subs.set(previous_subs);
        result
    }

    pub(crate) fn write_abstract_title(&mut self, title: &Title<'_>) -> Result<(), Error> {
        if title.is_empty() {
            return Ok(());
        }
        self.writer.raw("#abstracttitle[");
        self.write_title(title)?;
        self.writer.raw("]");
        Ok(())
    }

    pub(crate) fn write_abstract(
        &mut self,
        title: Option<&Title<'_>>,
        metadata: &BlockMetadata<'_>,
        write_body: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if let Some(title) = title {
            self.write_abstract_title(title)?;
            self.writer.raw("\n");
        }
        self.writer.raw("#abstract[\n");
        self.write_paragraph_alignment(metadata, write_body)?;
        self.writer.raw("\n]\n\n");
        Ok(())
    }

    /// Write the attribution line a quote or verse carries, if any.
    ///
    /// The cited work is only displayed when an author is present, matching
    /// asciidoctor-pdf. Both values keep their inline formatting.
    pub(crate) fn write_attribution(&mut self, metadata: &BlockMetadata<'_>) -> Result<(), Error> {
        let Some(attribution) = metadata.attribution.as_ref() else {
            return Ok(());
        };
        let citetitle = metadata.citetitle.as_ref();
        self.writer.raw("#attribution[");
        self.write_inlines(attribution)?;
        if let Some(citetitle) = citetitle {
            self.write_text_expr(", ");
            self.write_inlines(citetitle)?;
        }
        self.writer.raw("]\n\n");
        Ok(())
    }

    fn write_paragraph_alignment(
        &mut self,
        metadata: &BlockMetadata<'_>,
        write_body: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let alignment = ParagraphAlignment::from_metadata(metadata);
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
        metadata: &BlockMetadata<'_>,
        write_body: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.write_paragraph_alignment(metadata, write_body)?;
        self.writer.raw("\n\n");
        Ok(())
    }

    fn write_paragraph_roles_start(
        &mut self,
        metadata: &BlockMetadata<'_>,
        automatic_lead: bool,
    ) -> usize {
        let mut wrappers = 0;
        if automatic_lead {
            self.writer.raw("#text(size: 1.25em)[");
            wrappers += 1;
        }
        for role in &metadata.roles {
            let prefix = match *role {
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
        metadata: &BlockMetadata<'_>,
        write_body: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.writer.raw("#blockquote[\n");
        write_body(self)?;
        if metadata.attribution.is_some() {
            self.writer.raw("\n#text(style: \"normal\")[\n");
            self.write_attribution(metadata)?;
            self.writer.raw("]\n");
        }
        self.writer.raw("]\n\n");
        Ok(())
    }

    fn write_verse_content(&mut self, nodes: &[InlineNode<'_>]) {
        let text = InlineTextTransform::default()
            .line_break("\n")
            .to_string(nodes);
        self.writer.raw("#verse[");
        self.write_text_expr(&text);
        self.writer.raw("]");
    }

    /// Write verse content, which keeps its line breaks and stays proportional.
    pub(crate) fn write_verse_block(
        &mut self,
        nodes: &[InlineNode<'_>],
        metadata: &BlockMetadata<'_>,
    ) -> Result<(), Error> {
        self.write_verse_content(nodes);
        self.writer.raw("\n\n");
        self.write_attribution(metadata)
    }

    /// Write a paragraph's content in the shape its style asks for.
    ///
    /// A `[quote]`, `[verse]`, `[literal]`, `[listing]`, `[source]`, or
    /// `[example]` paragraph reads as its delimited counterpart, matching
    /// `asciidoctor`.
    fn write_paragraph_body(
        &mut self,
        para: &Paragraph<'_>,
        automatic_lead: bool,
    ) -> Result<(), Error> {
        match para.metadata.style {
            Some("quote") => self.write_quote_block(&para.metadata, |visitor| {
                visitor.write_paragraph_alignment(&para.metadata, |visitor| {
                    visitor.write_inlines(&para.content)
                })
            }),
            Some("verse") => {
                self.write_aligned_paragraph_body(&para.metadata, |visitor| {
                    visitor.write_verse_content(&para.content);
                    Ok(())
                })?;
                self.write_attribution(&para.metadata)
            }
            Some("literal" | "listing" | "source") => {
                self.write_verbatim_block(&para.content, &para.metadata);
                Ok(())
            }
            Some("example") => self.write_aligned_paragraph_body(&para.metadata, |visitor| {
                visitor.writer.raw("#examplebox[\n");
                visitor.write_inlines(&para.content)?;
                visitor.writer.raw("\n]");
                Ok(())
            }),
            _ => {
                let wrappers = self.write_paragraph_roles_start(&para.metadata, automatic_lead);
                self.write_paragraph_alignment(&para.metadata, |visitor| {
                    visitor.write_inlines(&para.content)
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
        title: &Title<'_>,
        metadata: &BlockMetadata<'_>,
        fallback: Option<CaptionKind>,
    ) -> Result<(), Error> {
        self.write_captioned_title_with("blocktitle", title, metadata, fallback)
    }

    fn write_captioned_title_with(
        &mut self,
        wrapper: &str,
        title: &Title<'_>,
        metadata: &BlockMetadata<'_>,
        fallback: Option<CaptionKind>,
    ) -> Result<(), Error> {
        if title.is_empty() {
            return Ok(());
        }
        let prefix = self.caption_prefix(metadata, fallback);
        let _ = write!(self.writer, "#{wrapper}[");
        if let Some(prefix) = prefix {
            self.write_text_expr(&prefix);
        }
        self.write_title(title)?;
        self.writer.raw("]\n");
        Ok(())
    }

    /// The caption prefix for a titled block, e.g. `Example 2. `, or `None` when the caption
    /// carries no prefix.
    ///
    /// The parser resolves the label from the document attributes in effect at the block's
    /// source position and assigns the ordinal, so a caption change part-way through a document
    /// applies from that point on, and nested blocks number inner-first.
    fn caption_prefix(
        &self,
        metadata: &BlockMetadata<'_>,
        fallback: Option<CaptionKind>,
    ) -> Option<String> {
        // Caller-built metadata never had a source position, so it falls back to the document's
        // final attributes. `Caption::resolve_owned` keeps the precedence chain in the parser
        // rather than duplicating it here.
        let resolved = match (&metadata.caption, fallback) {
            (Some(caption), _) => caption.clone(),
            (None, Some(kind)) => {
                Caption::resolve_owned(metadata, self.processor.document_attributes(), kind)
            }
            (None, None) => return None,
        };
        match resolved {
            Caption::Numbered {
                label,
                number,
                kind,
            } => Some(Self::numbered_caption_prefix(
                &label,
                // A block the parser numbered keeps that ordinal. One it could not — a
                // caller-built block, or a parsed block that gained its title afterwards —
                // draws from this converter's counter, which starts past every parsed ordinal.
                number.map_or_else(|| self.next_caption_number(kind), NonZeroU32::get),
            )),
            Caption::Custom(prefix) => Some(prefix.into_owned()),
            Caption::Unnumbered | _ => None,
        }
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
    pub(crate) fn write_example(&mut self, blocks: &[Block<'_>]) -> Result<(), Error> {
        self.write_framed_blocks(Some("examplebox"), None, blocks)
    }

    /// Write a sidebar, which centres its title inside the shaded box rather
    /// than setting it above, matching Asciidoctor PDF.
    pub(crate) fn write_sidebar(
        &mut self,
        title: &Title<'_>,
        blocks: &[Block<'_>],
    ) -> Result<(), Error> {
        self.writer.raw("#sidebarbox[\n");
        if !title.is_empty() {
            self.writer.raw("#sidebartitle[");
            self.write_title(title)?;
            self.writer.raw("]\n");
        }
        self.write_blocks(blocks)?;
        self.writer.raw("]\n\n");
        Ok(())
    }

    pub(crate) fn write_verbatim_block(
        &mut self,
        nodes: &[InlineNode<'_>],
        metadata: &BlockMetadata<'_>,
    ) {
        if metadata.options.contains(&"mixed") && self.source_language(metadata) == Some("php") {
            self.warn_unsupported_once(
                "php-mixed-highlighting",
                "PHP source block mixed-mode highlighting is not supported by the PDF backend; rendering with Typst's normal PHP highlighter",
                "Use the `html+php` source language when it gives acceptable highlighting, or use Asciidoctor PDF for explicit `%mixed` highlighting.",
            );
        }
        let tab_size = self.code_tab_size(metadata);
        let source = self.code_text(nodes, tab_size);
        let autofit = has_autofit_option(metadata, self.processor.document_attributes());
        let options = if self.source_highlighting_enabled() {
            SourceLineOptions::resolve(metadata, &source)
        } else {
            SourceLineOptions::default()
        };
        if autofit {
            if options.is_empty() {
                self.write_autofit_open(&source, metadata, 0);
                self.write_raw_block(&source, metadata);
                self.writer.raw("]\n\n");
                return;
            }

            let source_lines = (1..=source_line_count(&source)).collect::<Vec<_>>();
            if source_lines.is_empty() {
                self.write_autofit_open(&source, metadata, 0);
                self.write_raw_block(&source, metadata);
                self.writer.raw("]\n\n");
                return;
            }
            let extra_width_tenths = if options.line_number_start.is_some() {
                source_gutter_tenths(&source, &options).saturating_add(8)
            } else {
                0
            };
            self.write_autofit_open(&source, metadata, extra_width_tenths);
            self.write_source_block_with_line_options(&source, &source_lines, metadata, &options);
            self.writer.raw("]\n\n");
            return;
        }

        let wrap_columns = self
            .table_cell_code_wrap_columns()
            .unwrap_or(self.code_wrap_columns);
        if options.is_empty() {
            let source = wrap_code_text(&source, wrap_columns, tab_size);
            self.write_raw_block(&source, metadata);
            return;
        }

        let (source, source_lines) =
            wrap_code_text_with_line_origins(&source, wrap_columns, tab_size);
        if source_lines.is_empty() {
            self.write_raw_block(&source, metadata);
            return;
        }
        self.write_source_block_with_line_options(&source, &source_lines, metadata, &options);
    }

    fn write_autofit_open(
        &mut self,
        source: &str,
        metadata: &BlockMetadata<'_>,
        extra_width_tenths: usize,
    ) {
        self.writer.raw("#_acdc_autofit_code(");
        self.writer.string_literal(source);
        if let Some(language) = self.source_language(metadata) {
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

    fn write_raw_block(&mut self, source: &str, metadata: &BlockMetadata<'_>) {
        self.writer.raw("#raw(block: true");
        if let Some(language) = self.source_language(metadata) {
            self.writer.raw(", lang: ");
            self.writer.string_literal(language);
        }
        self.writer.raw(", ");
        self.writer.string_literal(source);
        self.writer.raw(")\n\n");
    }

    fn write_source_block_with_line_options(
        &mut self,
        source: &str,
        source_lines: &[usize],
        metadata: &BlockMetadata<'_>,
        options: &SourceLineOptions,
    ) {
        self.writer.raw("#{\n  let numbers = ");
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
        self.writer
            .raw("    let code = box(width: code-width, fill: if marked { rgb(");
        self.writer.string_literal(&self.palette.accent);
        self.writer.raw(") } else { none }, line.body)\n");
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
        if let Some(language) = self.source_language(metadata) {
            self.writer.raw(", lang: ");
            self.writer.string_literal(language);
        }
        self.writer.raw(", ");
        self.writer.string_literal(source);
        self.writer.raw(")\n}\n\n");
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

    fn code_tab_size(&self, metadata: &BlockMetadata<'_>) -> usize {
        metadata
            .attributes
            .get("tabsize")
            .or_else(|| self.processor.document_attributes().get("tabsize"))
            .and_then(|value| {
                if let AttributeValue::String(value) = value {
                    value.parse().ok()
                } else {
                    None
                }
            })
            .filter(|size| *size > 0)
            .unwrap_or(4)
    }

    fn source_language<'metadata>(
        &self,
        metadata: &'metadata BlockMetadata<'_>,
    ) -> Option<&'metadata str> {
        self.source_highlighting_enabled()
            .then(|| detect_language(metadata))
            .flatten()
            .filter(|language| acdc_pdf_render::supports_raw_language(language))
    }

    fn source_highlighting_enabled(&self) -> bool {
        self.processor
            .document_attributes()
            .get("source-highlighter")
            .is_some_and(|value| !matches!(value, AttributeValue::Bool(false)))
    }

    /// Write passthrough content as escaped, unframed monospace text.
    pub(crate) fn write_passthrough_block(&mut self, nodes: &[InlineNode<'_>]) {
        self.writer.raw("#block(width: 100%)[#raw(block: false, ");
        self.write_verbatim_string(nodes);
        self.writer.raw(")]\n\n");
    }

    fn write_verbatim_string(&mut self, nodes: &[InlineNode<'_>]) {
        let text = InlineTextTransform::default()
            .line_break("\n")
            .to_string(nodes);
        self.write_verbatim_text(&text);
    }

    fn code_text(&self, nodes: &[InlineNode<'_>], tab_size: usize) -> String {
        let mut text = code_text_without_callout_guards(nodes);
        #[cfg(feature = "pre-spec-subs")]
        let subs = self.processor.current_subs.get();
        #[cfg(feature = "pre-spec-subs")]
        if subs.contains(SubsFlags::ATTRIBUTES)
            && let Cow::Owned(expanded) = acdc_parser::substitute(
                &text,
                &[acdc_parser::Substitution::Attributes],
                self.processor.document_attributes(),
            )
        {
            text = expanded;
        }
        #[cfg(feature = "pre-spec-subs")]
        if let Cow::Owned(replaced) =
            apply_replacements(&text, subs, &Replacements::unicode(), TextBoundaries::BOTH)
        {
            text = replaced;
        }
        if let Cow::Owned(expanded) = expand_code_tabs(&text, tab_size) {
            text = expanded;
        }
        text
    }

    fn write_verbatim_text(&mut self, text: &str) {
        #[cfg(feature = "pre-spec-subs")]
        {
            let text = apply_replacements(
                text,
                self.processor.current_subs.get(),
                &Replacements::unicode(),
                TextBoundaries::BOTH,
            );
            self.writer.string_literal(&text);
        }
        #[cfg(not(feature = "pre-spec-subs"))]
        self.writer.string_literal(text);
    }

    pub(crate) fn write_stem_fallback(&mut self, content: &str, block: bool) {
        self.warn_unsupported("stem content", "rendering it as literal text");
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
        frame: Option<&str>,
        label: Option<&str>,
        blocks: &[Block<'_>],
    ) -> Result<(), Error> {
        if let Some(frame) = frame {
            let _ = writeln!(self.writer, "#{frame}[");
        }
        if let Some(label) = label {
            self.writer.raw("#text(weight: \"bold\")[");
            self.write_text_expr(label);
            self.writer.raw("]#linebreak()\n");
        }
        self.write_blocks(blocks)?;
        if frame.is_some() {
            self.writer.raw("]\n\n");
        }
        Ok(())
    }

    pub(crate) fn warn_unsupported(&mut self, feature: &str, fallback: &str) {
        self.diagnostics.warn_with_advice(
            format!("{feature} is not yet supported by the PDF backend, {fallback}"),
            "Use the HTML backend or Asciidoctor PDF for this feature until PDF backend support is added.",
        );
    }

    pub(crate) fn warn_unsupported_once(
        &mut self,
        key: &'static str,
        message: impl Into<Cow<'static, str>>,
        advice: impl Into<Cow<'static, str>>,
    ) {
        if self.unsupported_metadata_warnings.insert(key) {
            self.diagnostics.warn_with_advice(message, advice);
        }
    }

    pub(crate) fn warn_unsupported_parser_variant(&mut self, node_kind: &str) {
        self.diagnostics.warn_with_advice(
            format!(
                "an unsupported parser {node_kind} variant was omitted from PDF output"
            ),
            "Use the HTML backend or Asciidoctor PDF for this document and report the unsupported construct.",
        );
    }

    pub(crate) fn warn_static_media_fallback(&mut self) {
        if self.static_media_warning == StaticMediaWarningState::Emitted {
            return;
        }
        self.static_media_warning = StaticMediaWarningState::Emitted;
        self.diagnostics.warn_with_advice(
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

    pub(crate) fn static_media_source_target(&self, source: &Source<'_>) -> String {
        self.static_media_target(source.to_string().as_str())
    }

    pub(crate) fn static_media_target(&self, target: &str) -> String {
        acdc_converters_core::media::resolve_target(target, self.processor.document_attributes())
    }

    pub(crate) fn has_asset(&self, target: &str) -> bool {
        self.assets.get(target).is_some()
    }

    pub(crate) fn asset_virtual_path(&self, target: &str) -> Option<String> {
        self.assets
            .get(target)
            .map(|asset| asset.virtual_path.clone())
    }

    pub(crate) fn write_static_media_caption(&mut self, title: &Title<'_>) -> Result<(), Error> {
        if title.is_empty() {
            return Ok(());
        }
        self.writer.raw("#imagecaption[");
        self.write_title(title)?;
        self.writer.raw("]\n");
        Ok(())
    }

    pub(crate) fn write_list_item(
        &mut self,
        marker: &str,
        item: &ListItem<'_>,
    ) -> Result<(), Error> {
        let indent = "  ".repeat(self.list_depth);
        let _ = write!(self.writer, "{indent}{marker} ");
        let has_blocks = !item.blocks.is_empty();
        if has_blocks {
            self.writer.raw("#block(width: 100%)[");
        }
        if let Some(checked) = &item.checked {
            match checked {
                acdc_parser::ListItemCheckedStatus::Checked => self.writer.raw("#checkbox(true) "),
                acdc_parser::ListItemCheckedStatus::Unchecked => {
                    self.writer.raw("#checkbox(false) ");
                }
                _ => {}
            }
        }
        self.write_inlines(&item.principal)?;
        self.writer.raw("\n");

        if has_blocks {
            self.writer.raw("\n");
            self.list_depth += 1;
            for block in &item.blocks {
                self.visit_block(block)?;
            }
            self.list_depth -= 1;
            let _ = writeln!(self.writer, "{indent}]");
        }
        Ok(())
    }

    pub(crate) fn write_bibliography_list(
        &mut self,
        list: &acdc_parser::UnorderedList<'_>,
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
        for item in &list.items {
            let item_indent = "  ".repeat(self.list_depth);
            let _ = write!(self.writer, "{item_indent}- #block(width: 100%)[");
            self.write_paragraph_alignment(&list.metadata, |visitor| {
                visitor.write_bibliography_principal(item)
            })?;
            if !item.blocks.is_empty() {
                self.writer.raw("\n\n");
                self.list_depth += 1;
                for block in &item.blocks {
                    self.visit_block(block)?;
                }
                self.list_depth -= 1;
            }
            self.writer.raw("]\n");
        }
        self.list_depth -= 1;
        let _ = writeln!(self.writer, "{indent}]\n");
        Ok(())
    }

    fn write_bibliography_principal(&mut self, item: &ListItem<'_>) -> Result<(), Error> {
        let Some((InlineNode::InlineAnchor(anchor), content)) = item.principal.split_first() else {
            return self.write_inlines(&item.principal);
        };
        if !anchor.is_bibliography() {
            return self.write_inlines(&item.principal);
        }

        let _ = write!(self.writer, "#metadata(none) <{}>", encode_label(anchor.id));
        let references = Rc::clone(&self.processor.references);
        let backlink = references
            .get(anchor.id)
            .is_some_and(acdc_parser::Reference::has_automatic_citation);
        if backlink {
            let label = encode_bibliography_reference_label(anchor.id);
            let _ = write!(self.writer, "#link(<{label}>)[");
        }

        if let Some(label) = references
            .get(anchor.id)
            .and_then(|reference| reference.xreflabel.as_deref())
        {
            self.write_inlines(label)?;
        } else {
            self.write_text_expr(&format!("[{}]", anchor.id));
        }

        if backlink {
            self.writer.raw("]");
        }
        self.write_inlines(content)
    }

    pub(crate) fn write_horizontal_description_list(
        &mut self,
        list: &DescriptionList<'_>,
    ) -> Result<(), Error> {
        if list.items.is_empty() {
            return Ok(());
        }

        self.writer
            .raw("#layout(size => {\nlet term-width = calc.min(calc.max(0pt,\n");
        for row in description_list_rows(&list.items) {
            self.writer.raw("measure([");
            self.write_horizontal_description_terms(row, false)?;
            self.writer.raw("]).width,\n");
        }
        self.writer.raw(
            "), size.width * 50%)\ngrid(columns: (term-width, 1fr), column-gutter: 20pt, row-gutter: 0.5em, align: top,\n",
        );

        for row in description_list_rows(&list.items) {
            let Some(item) = row.last() else {
                continue;
            };
            self.writer.raw("[");
            self.write_horizontal_description_terms(row, true)?;
            self.writer.raw("], [");
            self.write_description_list_item(item)?;
            self.writer.raw("],\n");
        }

        self.writer.raw(")\n})\n\n");
        Ok(())
    }

    pub(crate) fn write_description_list(
        &mut self,
        list: &DescriptionList<'_>,
    ) -> Result<(), Error> {
        let indent = "  ".repeat(self.list_depth);

        for row in description_list_rows(&list.items) {
            let Some(description) = row.last() else {
                continue;
            };
            let _ = writeln!(
                self.writer,
                "{indent}#block(width: 100%, above: 0pt, below: 0.5em)["
            );
            for (index, item) in row.iter().enumerate() {
                if index > 0 {
                    self.writer.raw("#linebreak()");
                }
                for anchor in &item.anchors {
                    self.write_anchor_target(anchor);
                }
                self.writer.raw("#text(weight: \"bold\")[");
                self.write_inlines(&item.term)?;
                self.writer.raw("]");
            }
            if !description.principal_text.is_empty() || !description.description.is_empty() {
                self.writer
                    .raw("\n#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[");
                self.list_depth += 1;
                self.write_description_list_item(description)?;
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
        list: &DescriptionList<'_>,
    ) -> Result<(), Error> {
        if list.items.is_empty() {
            return Ok(());
        }

        let indent = "  ".repeat(self.list_depth);
        let _ = writeln!(self.writer, "{indent}#[");
        let _ = write!(self.writer, "{indent}#set enum(numbering: ");
        self.write_ordered_list_numbering(OrderedListNumbering::Arabic);
        self.writer.raw(", spacing: 1em)\n");

        for row in description_list_rows(&list.items) {
            let Some(answer) = row.last() else {
                continue;
            };
            let _ = write!(self.writer, "{indent}  + #block(width: 100%)[");
            self.writer.raw("#block(width: 100%, breakable: false)[");
            self.write_paragraph_alignment(&list.metadata, |visitor| {
                visitor.write_qanda_description_terms(row)?;
                if !answer.principal_text.is_empty() {
                    visitor.writer.raw("#linebreak()\n");
                    visitor.write_inlines(&answer.principal_text)?;
                }
                Ok(())
            })?;
            self.writer.raw("]");
            if !answer.description.is_empty() {
                self.writer.raw("\n\n");
                self.write_blocks(&answer.description)?;
            }
            self.writer.raw("]\n");
        }

        let _ = writeln!(self.writer, "{indent}]\n");
        Ok(())
    }

    pub(crate) fn write_marker_description_list(
        &mut self,
        list: &DescriptionList<'_>,
    ) -> Result<(), Error> {
        if list.items.is_empty() {
            return Ok(());
        }

        let ordered = list.metadata.style == Some("ordered");
        let stacked = list.metadata.roles.contains(&"stack");
        let subject_stop = list.metadata.attributes.get_string("subject-stop");
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

        for row in description_list_rows(&list.items) {
            let Some(subject) = row.first() else {
                continue;
            };
            let Some(description) = row.last() else {
                continue;
            };
            let marker = if ordered { "+" } else { "-" };
            let _ = write!(self.writer, "{indent}  {marker} #block(width: 100%)[");
            for anchor in &subject.anchors {
                self.write_anchor_target(anchor);
            }
            self.writer.raw("#strong[");
            self.write_inlines(&subject.term)?;
            let subject_text = inlines_to_string(&subject.term);
            let has_stop = subject_text
                .trim_end()
                .chars()
                .next_back()
                .is_some_and(|last| matches!(last, '.' | '!' | '?' | ';' | ':'));
            if !has_stop
                && let Some(stop) = subject_stop
                    .as_deref()
                    .or_else(|| (!stacked).then_some(":"))
            {
                self.write_text_expr(stop);
            }
            self.writer.raw("]");

            if !description.principal_text.is_empty() {
                self.writer
                    .raw(if stacked { "#linebreak()\n" } else { " " });
                self.write_inlines(&description.principal_text)?;
            }
            if !description.description.is_empty() {
                self.writer.raw("\n\n");
                self.write_blocks(&description.description)?;
            }
            self.writer.raw("]\n");
        }

        let _ = writeln!(self.writer, "{indent}]\n");
        Ok(())
    }

    fn write_horizontal_description_terms(
        &mut self,
        items: &[DescriptionListItem<'_>],
        write_anchors: bool,
    ) -> Result<(), Error> {
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                self.writer.raw("#linebreak()");
            }
            if write_anchors {
                for anchor in &item.anchors {
                    self.write_anchor_target(anchor);
                }
            }
            self.writer.raw("#text(weight: \"bold\")[");
            self.write_inlines(&item.term)?;
            self.writer.raw("]");
        }
        Ok(())
    }

    fn write_qanda_description_terms(
        &mut self,
        items: &[DescriptionListItem<'_>],
    ) -> Result<(), Error> {
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                self.writer.raw("#linebreak()");
            }
            for anchor in &item.anchors {
                self.write_anchor_target(anchor);
            }
            self.writer.raw("#emph[");
            self.write_inlines(&item.term)?;
            self.writer.raw("]");
        }
        Ok(())
    }

    fn write_description_list_item(&mut self, item: &DescriptionListItem<'_>) -> Result<(), Error> {
        if !item.principal_text.is_empty() {
            self.write_inlines(&item.principal_text)?;
            if !item.description.is_empty() {
                self.writer.raw("\n\n");
            }
        }
        self.write_blocks(&item.description)
    }

    pub(crate) fn write_ordered_list_start(&mut self, metadata: &BlockMetadata<'_>, marker: &str) {
        let indent = "  ".repeat(self.list_depth);
        let _ = writeln!(self.writer, "{indent}#[");
        let _ = write!(self.writer, "{indent}#set enum(numbering: ");
        let markerless = matches!(
            metadata.style,
            Some("none" | "no-bullet" | "unnumbered" | "unstyled")
        );
        if metadata.style == Some("none") {
            self.writer.raw("(..numbers) => box(width: 0.5em)[]");
        } else if markerless {
            self.writer.raw("(..numbers) => none");
        } else {
            let numbering = match metadata.style {
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
        if metadata.style == Some("unstyled") {
            self.writer.raw(", body-indent: 0pt");
            self.ordered_unstyled_scope_depth += 1;
        } else if self.ordered_unstyled_scope_depth > 0 {
            self.writer.raw(", body-indent: 0.5em");
        }
        if let Some(start) = metadata
            .attributes
            .get_string("start")
            .and_then(|start| start.parse::<i64>().ok())
            .filter(|start| *start > 0)
        {
            let _ = write!(self.writer, ", start: {start}");
        }
        if metadata.options.contains(&"reversed") {
            self.writer.raw(", reversed: true");
        }
        self.writer.raw(")\n");
    }

    pub(crate) fn write_ordered_list_end(&mut self, metadata: &BlockMetadata<'_>) {
        if metadata.style == Some("unstyled") {
            self.ordered_unstyled_scope_depth -= 1;
        }
    }

    pub(crate) fn write_unordered_list_start(&mut self, metadata: &BlockMetadata<'_>) -> bool {
        if metadata.options.contains(&"checklist") {
            return false;
        }
        let style = metadata.style.and_then(UnorderedMarker::from_style);
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
        table: &Table<'_>,
        metadata: &BlockMetadata<'_>,
    ) -> Result<(), Error> {
        let column_count = determine_column_count(table);
        let width = table_width(metadata);
        let autowidth = metadata.options.contains(&"autowidth");
        let constrained = width.is_some() || metadata.roles.contains(&"stretch");
        if autowidth && !constrained {
            let _ = write!(self.writer, "#table(columns: {column_count}");
        } else {
            let tracks = table_column_tracks(table, column_count);
            let _ = write!(
                self.writer,
                "#table(columns: {}",
                typst_table_column_tracks(&tracks)
            );
        }
        let alignments = table_column_alignments(table, column_count);
        let _ = write!(self.writer, ", align: {alignments}, stroke: none");

        let grid = build_grid(table, column_count);
        let presentation = table.presentation().unwrap_or_else(|| {
            TablePresentation::from_attributes(metadata, self.processor.document_attributes())
        });
        let header_rows = usize::from(grid.first().is_some_and(|row| row.is_header));
        let row_count = grid.len();
        let available_columns = self
            .table_cell_code_wrap_columns()
            .unwrap_or(self.code_wrap_columns)
            .saturating_mul(usize::from(width.unwrap_or(100)))
            / 100;
        let context = TableRenderContext {
            table,
            presentation,
            column_count,
            row_count,
            header_rows,
            column_width_columns: if autowidth && !constrained {
                equal_table_column_widths(column_count, available_columns)
            } else {
                table_column_width_columns(
                    &table_column_tracks(table, column_count),
                    available_columns,
                )
            },
        };
        if let Some(header) = grid.first().filter(|row| row.is_header) {
            self.writer.raw(", table.header(repeat: true, ");
            self.write_table_row_cells(&context, header, 0, "")?;
            self.writer.raw(")");
        }

        // Typst owns the merged geometry. Emit each real cell at its logical
        // position and omit the grid's horizontal/vertical span placeholders.
        for (y, row) in grid
            .iter()
            .enumerate()
            .filter(|(_, row)| !row.is_header && !row.is_footer)
        {
            self.write_table_row_cells(&context, row, y, ", ")?;
        }

        if let Some(footer) = grid.last().filter(|row| row.is_footer) {
            self.writer.raw(", table.footer(repeat: false, ");
            self.write_table_row_cells(&context, footer, grid.len() - 1, "")?;
            self.writer.raw(")");
        }

        self.writer.raw(")\n\n");
        Ok(())
    }

    pub(crate) fn write_table_width_start(&mut self, metadata: &BlockMetadata<'_>) -> bool {
        let Some(width) = table_width(metadata).filter(|width| *width < 100) else {
            return false;
        };
        let _ = writeln!(self.writer, "#block(width: {width}%)[");
        true
    }

    pub(crate) fn write_table_alignment_start(&mut self, metadata: &BlockMetadata<'_>) -> bool {
        let alignment = table_alignment(metadata, self.table_theme.align);
        let has_reduced_width = table_width(metadata).is_some_and(|width| width < 100);
        if alignment == TableAlignment::Left
            || !(has_reduced_width || Self::table_has_intrinsic_width(metadata))
        {
            return false;
        }
        let _ = writeln!(self.writer, "#align({})[", typst_table_alignment(alignment));
        true
    }

    pub(crate) fn write_table_wrappers_start(
        &mut self,
        metadata: &BlockMetadata<'_>,
        enabled: bool,
    ) -> (bool, bool) {
        let aligned = enabled && self.write_table_alignment_start(metadata);
        let sized = enabled && self.write_table_width_start(metadata);
        (sized, aligned)
    }

    pub(crate) fn table_has_intrinsic_width(metadata: &BlockMetadata<'_>) -> bool {
        metadata.options.contains(&"autowidth")
            && table_width(metadata).is_none()
            && !metadata.roles.contains(&"stretch")
    }

    pub(crate) fn write_intrinsic_table_start(&mut self) {
        self.writer.raw("#context {\nlet acdc-table-body = [\n");
    }

    pub(crate) fn write_intrinsic_table_end(
        &mut self,
        title: &Title<'_>,
        metadata: &BlockMetadata<'_>,
        fallback: Option<CaptionKind>,
    ) -> Result<(), Error> {
        let alignment = table_alignment(metadata, self.table_theme.align);
        let _ = writeln!(
            self.writer,
            "]\nalign({}, [\n#context block(width: measure(acdc-table-body).width)[",
            typst_table_alignment(alignment)
        );
        let sticky_title = metadata.options.contains(&"breakable")
            && !metadata.options.contains(&"unbreakable")
            && !title.is_empty();
        if sticky_title {
            self.writer
                .raw("#block(sticky: true, above: 0pt, below: 0pt)[\n");
        }
        self.write_captioned_title(title, metadata, fallback)?;
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

    pub(crate) fn omit_zero_width_table(&mut self, metadata: &BlockMetadata<'_>) -> bool {
        if table_width(metadata) != Some(0) {
            return false;
        }
        self.diagnostics.warn(
            "cannot fit contents of table cell into a table with a width of 0%; omitting the table",
        );
        true
    }

    fn write_table_row_cells(
        &mut self,
        context: &TableRenderContext<'_, '_>,
        row: &GridRow<'_>,
        y: usize,
        first_separator: &str,
    ) -> Result<(), Error> {
        let mut separator = first_separator;
        for (x, cell) in row.cells.iter().enumerate() {
            let CellKind::Content { cell_index } = cell else {
                continue;
            };
            if let Some(ast_cell) = row.ast_row.columns.get(*cell_index) {
                self.writer.raw(separator);
                self.write_table_cell(
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
        context: &TableRenderContext<'_, '_>,
        cell: &TableColumn<'_>,
        source_column_index: usize,
        position: TableCellPosition,
    ) -> Result<(), Error> {
        let TableCellPosition { x, y, .. } = position;
        let _ = write!(self.writer, "table.cell(x: {x}, y: {y}");
        if cell.colspan > 1 {
            let _ = write!(self.writer, ", colspan: {}", cell.colspan);
        }
        if cell.rowspan > 1 {
            let _ = write!(self.writer, ", rowspan: {}", cell.rowspan);
        }
        let default_alignment = || (HorizontalAlignment::default(), VerticalAlignment::default());
        let source_column_alignment = context
            .table
            .columns
            .get(source_column_index)
            .map_or_else(default_alignment, |column| (column.halign, column.valign));
        let physical_column_alignment = context
            .table
            .columns
            .get(x)
            .map_or_else(default_alignment, |column| (column.halign, column.valign));
        if let Some(alignment) =
            table_cell_alignment(cell, source_column_alignment, physical_column_alignment)
        {
            let _ = write!(self.writer, ", align: {alignment}");
        }
        self.write_table_cell_stroke(context, cell, position);
        self.write_table_cell_fill(context, position);
        self.writer.raw(")[");
        let outer_cell_state = std::mem::replace(
            &mut self.table_cell_section_state,
            TableCellSectionState::Inside {
                width_columns: table_cell_width_columns(context, x, cell.colspan),
            },
        );
        let result = self.write_table_cell_content(cell, position.is_header);
        self.table_cell_section_state = outer_cell_state;
        result?;
        self.writer.raw("]");
        Ok(())
    }

    fn write_table_cell_stroke(
        &mut self,
        context: &TableRenderContext<'_, '_>,
        cell: &TableColumn<'_>,
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
        let right = x.saturating_add(cell.colspan).min(column_count);
        let bottom = y.saturating_add(cell.rowspan).min(row_count);

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
        context: &TableRenderContext<'_, '_>,
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
        cell: &TableColumn<'_>,
        is_header: bool,
    ) -> Result<(), Error> {
        let style = if is_header {
            Some(ColumnStyle::Header)
        } else {
            cell.style
        };

        if style == Some(ColumnStyle::Literal)
            && let Some(text) = literal_table_cell_text(&cell.content)
        {
            self.write_table_literal(&text);
            return Ok(());
        }

        let wrapper = match style {
            Some(ColumnStyle::Emphasis) => Some("#tableemphasis["),
            Some(ColumnStyle::Header) => Some("#tableheader["),
            Some(ColumnStyle::Monospace | ColumnStyle::Literal) => Some("#tablemonospace["),
            Some(ColumnStyle::Strong) => Some("#tablestrong["),
            None | Some(_) => None,
        };
        if let Some(wrapper) = wrapper {
            self.writer.raw(wrapper);
        }
        if style == Some(ColumnStyle::AsciiDoc) {
            self.write_asciidoc_table_cell(&cell.content)?;
        } else {
            self.write_blocks(&cell.content)?;
        }
        if cell.content.is_empty() {
            self.write_text_expr("");
        }
        if wrapper.is_some() {
            self.writer.raw("]");
        }
        Ok(())
    }

    fn write_asciidoc_table_cell(&mut self, blocks: &[Block<'_>]) -> Result<(), Error> {
        self.write_blocks(blocks)
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

    pub(crate) fn write_block_image(&mut self, image: &Image<'_>) -> Result<(), Error> {
        if block_image_float(&image.metadata).is_some() {
            self.warn_unsupported(
                "block image side wrapping",
                "rendering the image on the requested side with following content below it",
            );
        }
        let has_caption = !image.title.is_empty();
        if has_caption {
            self.writer.raw("#block(width: 100%, breakable: false)[\n");
        }
        let source = self.static_media_source_target(&image.source);
        let alt = block_image_alt(image);
        let link = image
            .metadata
            .attributes
            .get_string("link")
            .filter(|target| !target.is_empty());
        if let Some(asset) = self.assets.get(&source) {
            let width = image_width(&image.metadata, true);
            let alignment = block_image_alignment(&image.metadata);
            let align_to_page = image.metadata.options.contains(&"align-to-page");
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
                    link.as_deref(),
                    align_to_page,
                );
            }
        } else {
            self.write_block_image_fallback(image, &alt, link.as_deref());
            self.writer.raw("\n");
        }
        if has_caption {
            self.write_captioned_title_with(
                "imagecaption",
                &image.title,
                &image.metadata,
                Some(CaptionKind::Figure),
            )?;
            self.writer.raw("]\n\n");
        } else {
            self.writer.raw("\n");
        }
        Ok(())
    }

    fn write_block_image_fallback(&mut self, image: &Image<'_>, alt: &str, link: Option<&str>) {
        if let Some(target) = link {
            self.writer.raw("#link(");
            self.writer.string_literal(target);
            self.writer.raw(")[");
        }
        self.write_text_expr(&format!("[{alt}]"));
        if link.is_some() {
            self.writer.raw("]");
        }
        self.write_text_expr(&format!(" | {}", image.source));
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

    pub(crate) fn write_inline_image(&mut self, image: &Image<'_>) {
        let source = self.static_media_source_target(&image.source);
        let alt = inline_image_alt(image);
        let link = image
            .metadata
            .attributes
            .get_string("link")
            .filter(|target| !target.is_empty());
        if let Some(asset) = self.assets.get(&source) {
            let fit = image
                .metadata
                .attributes
                .get_string("fit")
                .unwrap_or_default();
            let fit_line = fit == "line";
            if fit == "none" {
                self.warn_unsupported_once(
                    "inline-image-fit-none",
                    "inline image `fit=none` page-height sizing is not supported by the PDF backend; rendering with normal intrinsic sizing",
                    "Use `fit=line` to constrain the image to the text line, or use Asciidoctor PDF when the image must use the full page height.",
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
            match image_width(&image.metadata, false) {
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
            self.write_text_expr(&inline_image_fallback_text(image));
            if link.is_some() {
                self.writer.raw("]");
            }
        }
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

    fn write_icon(&mut self, icon: &Icon<'_>) {
        let alt = icon_alt(&icon.target, &icon.attributes);
        match IconMode::from(self.processor.document_attributes()) {
            IconMode::Font => {
                if let Some(glyph) = builtin_icon_glyph(&icon.target.to_string()) {
                    match icon.attributes.get_string("size").as_deref() {
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
                    self.write_text_expr(&format!("[{alt}]"));
                }
            }
            IconMode::Image => {
                let source = icon_image_source(self.processor.document_attributes(), &icon.target);
                if let Some(asset) = self.assets.get(&source) {
                    self.writer.raw("#box(image(");
                    self.writer.string_literal(&asset.virtual_path);
                    self.writer.raw(", alt: ");
                    self.writer.string_literal(&alt);
                    self.writer.raw(", height: 1em))");
                } else {
                    self.write_text_expr(&format!("[{}]", icon.target));
                }
            }
            IconMode::Text | _ => self.write_text_expr(&format!("[{alt}]")),
        }
    }

    fn write_sized_icon_glyph(&mut self, glyph: &str, size: &str) {
        self.writer.raw("#text(size: ");
        self.writer.raw(size);
        self.writer.raw(")[");
        self.write_text_expr(glyph);
        self.writer.raw("]");
    }

    pub(crate) fn write_inline_macro(
        &mut self,
        inline_macro: &InlineMacro<'_>,
    ) -> Result<(), Error> {
        match inline_macro {
            InlineMacro::Footnote(footnote) => {
                if let Some(id) = footnote.id.filter(|_| footnote.content.is_empty()) {
                    // Typst label references reuse the definition's counter value
                    // without advancing the footnote counter.
                    let _ = write!(self.writer, "#footnote(<{}>)", encode_footnote_label(id));
                } else {
                    let _ = write!(
                        self.writer,
                        "#counter(footnote).update({})",
                        footnote.number.saturating_sub(1)
                    );
                    self.writer.raw("#footnote[");
                    self.write_inlines(&footnote.content)?;
                    self.writer.raw("]");
                    if let Some(id) = footnote.id {
                        let _ = write!(self.writer, " <{}>", encode_footnote_label(id));
                    }
                }
            }
            InlineMacro::Icon(icon) => self.write_icon(icon),
            InlineMacro::Image(image) => self.write_inline_image(image),
            InlineMacro::Keyboard(keyboard) => {
                for (index, key) in keyboard.keys.iter().enumerate() {
                    if index > 0 {
                        self.write_text_expr("\u{202f}+\u{202f}");
                    }
                    self.writer.raw("#box(baseline: 15%, fill: rgb(");
                    self.writer.string_literal(&self.palette.callout_bg);
                    self.writer.raw("), stroke: 0.5pt + rgb(");
                    self.writer.string_literal(&self.palette.border);
                    self.writer
                        .raw("), radius: 2pt, inset: (x: 2pt, y: 0.5pt))[");
                    self.write_inline_verbatim(key);
                    self.writer.raw("]");
                }
            }
            InlineMacro::Button(button) => {
                self.writer.raw("#strong[\\[\u{2009}");
                self.write_text_expr(button.label);
                self.writer.raw("\u{2009}\\]]");
            }
            InlineMacro::Menu(menu) => self.write_menu(menu),
            InlineMacro::Url(url) => {
                let target = url.target.to_string();
                let fallback = link_fallback(&target, url.hides_uri_scheme());
                self.write_link(&target, &url.text, Some(&url.attributes), fallback)?;
            }
            InlineMacro::Link(link) => {
                let target = link.target.to_string();
                let fallback = link_fallback(&target, link.hides_uri_scheme());
                self.write_link(&target, &link.text, Some(&link.attributes), fallback)?;
            }
            InlineMacro::Mailto(mailto) => {
                let target = mailto.target.to_string();
                let fallback = mailto_fallback(&target);
                self.write_link(&target, &mailto.text, Some(&mailto.attributes), fallback)?;
            }
            InlineMacro::Autolink(autolink) => self.write_autolink(autolink)?,
            InlineMacro::CrossReference(xref) => self.write_cross_reference(xref)?,
            InlineMacro::Pass(pass) => {
                if let Some(text) = pass.text {
                    self.write_text_expr(text);
                }
            }
            InlineMacro::Stem(stem) => {
                self.write_stem_fallback(stem.content, false);
            }
            InlineMacro::IndexTerm(term) => {
                self.write_index_term(term)?;
            }
            _ => self.warn_unsupported_parser_variant("inline macro"),
        }
        Ok(())
    }

    fn write_index_term(&mut self, term: &IndexTerm<'_>) -> Result<(), Error> {
        if !self.index_catalog.is_suspended() {
            let primary = self.render_index_catalog_term(term.term())?;
            let secondary = term
                .secondary()
                .map(|inlines| self.render_index_catalog_term(inlines))
                .transpose()?;
            let tertiary = term
                .tertiary()
                .map(|inlines| self.render_index_catalog_term(inlines))
                .transpose()?;
            if let Some(anchor) = self.index_catalog.add(primary, secondary, tertiary) {
                let _ = write!(self.writer, "#metadata(none) <__indexterm-{anchor}>");
            }
        }
        if term.is_visible() {
            self.write_inlines(term.term())?;
        }
        Ok(())
    }

    fn render_index_catalog_term(
        &mut self,
        inlines: &[InlineNode<'_>],
    ) -> Result<CatalogTerm, Error> {
        let plain = InlineTextTransform::default().to_string(inlines);
        let output = std::mem::replace(&mut self.writer, Writer::new());
        let previous = self.index_catalog.set_suspended(true);
        let result = self.write_inlines(inlines);
        self.index_catalog.set_suspended(previous);
        let markup = std::mem::replace(&mut self.writer, output).into_string();
        result?;
        Ok(CatalogTerm { plain, markup })
    }

    fn write_menu(&mut self, menu: &Menu<'_>) {
        let mut parts = Vec::with_capacity(menu.items.len() + 1);
        parts.push(menu.target);
        parts.extend(menu.items.iter().copied());
        self.writer.raw("#strong[");
        for (index, part) in parts.into_iter().enumerate() {
            if index > 0 {
                self.write_text_expr(" ");
                self.writer.raw("#text(size: 1.15em, fill: rgb(");
                self.writer.string_literal(&self.palette.accent);
                self.writer.raw("))[\u{203a}]");
                self.write_text_expr(" ");
            }
            self.write_text_expr(part);
        }
        self.writer.raw("]");
    }

    fn write_autolink(&mut self, autolink: &Autolink<'_>) -> Result<(), Error> {
        let target = autolink.url.to_string();
        let (fallback, angle_brackets) =
            autolink_fallback(&target, autolink.bracketed, autolink.hides_uri_scheme());
        if angle_brackets {
            self.write_text_expr("<");
        }
        self.write_link(&target, &[], None, fallback)?;
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
    /// alone. Every catalogued target gets a label from `write_block_anchor`,
    /// `write_anchor_target` or the section heading.
    fn write_cross_reference(&mut self, xref: &CrossReference<'_>) -> Result<(), Error> {
        // Clone the handles so the borrowed reference text and the resolution
        // guard both outlive the `&mut self` render calls.
        let references = Rc::clone(&self.processor.references);
        let guard = self.processor.xref_guard.clone();

        if xref.text.is_empty()
            && references
                .get(xref.target)
                .is_some_and(acdc_parser::Reference::is_bibliography)
            && self
                .bibliography_backlinks_written
                .insert(xref.target.to_string())
        {
            let label = encode_bibliography_reference_label(xref.target);
            let _ = write!(self.writer, "#metadata(none) <{label}>");
        }

        if !xref.text.is_empty() {
            if let Some((target, _)) = self.interdocument_xref(xref.target) {
                self.write_external_link(&target, |visitor| visitor.write_inlines(&xref.text))?;
            } else if references.contains_key(xref.target) {
                self.write_labelled_link(xref.target, |visitor| visitor.write_inlines(&xref.text))?;
            } else {
                self.write_inlines(&xref.text)?;
            }
            return Ok(());
        }

        let previous = self.index_catalog.set_suspended(true);
        let result = match resolve_xref(references.get(xref.target), xref, &guard) {
            XrefDisplay::Title(inlines, _scope) | XrefDisplay::Label(inlines, _scope) => {
                self.write_labelled_link(xref.target, |visitor| visitor.write_inlines(inlines))
            }
            XrefDisplay::ShortCaption(prefix) => self.write_labelled_link(xref.target, |visitor| {
                visitor.write_text_expr(&prefix);
                Ok(())
            }),
            XrefDisplay::FullCaption(prefix, inlines, _scope) => {
                self.write_labelled_link(xref.target, |visitor| {
                    visitor.write_text_expr(&prefix);
                    visitor.write_text_expr(", “");
                    visitor.write_inlines(inlines)?;
                    visitor.write_text_expr("”");
                    Ok(())
                })
            }
            XrefDisplay::Fallback(text) => self.write_labelled_link(xref.target, |visitor| {
                visitor.write_text_expr(&text);
                Ok(())
            }),
            XrefDisplay::Unresolved(text) | XrefDisplay::Nested(text) => {
                self.write_text_expr(&text);
                Ok(())
            }
            XrefDisplay::External(target) => {
                if let Some((target, text)) = self.interdocument_xref(&target) {
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
        self.index_catalog.set_suspended(previous);
        result
    }

    fn interdocument_xref(&self, target: &str) -> Option<(String, String)> {
        let attributes = self.processor.document_attributes();
        let extension = attributes
            .get_string("relfilesuffix")
            .or_else(|| attributes.get_string("outfilesuffix"))
            .map_or_else(|| "pdf".to_string(), Cow::into_owned);
        interdocument_xref(target, extension.strip_prefix('.').unwrap_or(&extension))
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
        let label = crate::encode_label(target);
        let _ = write!(self.writer, "#link(<{label}>)[");
        content(self)?;
        self.writer.raw("]");
        Ok(())
    }

    fn write_link(
        &mut self,
        target: &str,
        text: &[InlineNode<'_>],
        attributes: Option<&ElementAttributes<'_>>,
        fallback: &str,
    ) -> Result<(), Error> {
        let role = attributes.and_then(|attributes| attributes.get_string("role"));
        let state = self.write_inline_span_start(None, role.as_deref());

        match text {
            [InlineNode::Macro(InlineMacro::Image(image))]
                if image.metadata.attributes.get_string("link").is_some() =>
            {
                self.write_inline_image(image);
                self.write_inline_span_end(state);
                return Ok(());
            }
            _ => {}
        }

        self.writer.raw("#link(");
        self.writer.string_literal(target);
        self.writer.raw(")[");
        if text.is_empty() {
            self.write_text_expr(fallback);
        } else {
            self.write_inlines(text)?;
        }
        self.writer.raw("]");
        self.write_inline_span_end(state);
        Ok(())
    }
}

fn description_list_rows<'items, 'document>(
    items: &'items [DescriptionListItem<'document>],
) -> impl Iterator<Item = &'items [DescriptionListItem<'document>]> {
    items.split_inclusive(|item| !item.principal_text.is_empty() || !item.description.is_empty())
}

fn table_column_tracks(table: &Table<'_>, column_count: usize) -> Vec<TableColumnTrack> {
    if table.columns.is_empty() {
        return vec![TableColumnTrack::Fraction(1); column_count];
    }

    let has_automatic_width = table
        .columns
        .iter()
        .any(|column| column.width == ColumnWidth::Auto);
    let fixed_width_total: u64 = table
        .columns
        .iter()
        .filter_map(|column| match column.width {
            ColumnWidth::Proportional(width) | ColumnWidth::Percentage(width) => {
                Some(u64::from(width))
            }
            ColumnWidth::Auto => None,
            _ => Some(1),
        })
        .sum();
    let fixed_widths_fit = fixed_width_total <= 100;
    let all_fixed_widths_are_zero = !has_automatic_width && fixed_width_total == 0;

    table
        .columns
        .iter()
        .map(|column| match column.width {
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
    context: &TableRenderContext<'_, '_>,
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

fn table_width(metadata: &BlockMetadata<'_>) -> Option<u8> {
    let AttributeValue::String(value) = metadata.attributes.get("width")? else {
        return Some(100);
    };
    if matches!(value.as_ref(), "0" | "0%") {
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

fn table_alignment(metadata: &BlockMetadata<'_>, theme_default: TableAlignment) -> TableAlignment {
    if let Some(AttributeValue::String(value)) = metadata.attributes.get("align")
        && let Some(alignment) = source_table_alignment(value)
    {
        return alignment;
    }
    metadata
        .roles
        .iter()
        .rev()
        .find_map(|role| source_table_alignment(role))
        .unwrap_or(theme_default)
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

fn table_column_alignments(table: &Table<'_>, column_count: usize) -> String {
    if table.columns.is_empty() {
        return format!("({})", vec!["left + top"; column_count].join(", "));
    }

    let alignments = table
        .columns
        .iter()
        .map(|column| {
            let horizontal = typst_horizontal_alignment(column.halign);
            let vertical = typst_vertical_alignment(column.valign);
            format!("{horizontal} + {vertical}")
        })
        .collect::<Vec<_>>();

    format!("({})", alignments.join(", "))
}

fn table_cell_alignment(
    cell: &TableColumn<'_>,
    source_column_alignment: (HorizontalAlignment, VerticalAlignment),
    physical_column_alignment: (HorizontalAlignment, VerticalAlignment),
) -> Option<String> {
    let (column_horizontal, column_vertical) = source_column_alignment;
    let horizontal = cell.halign.unwrap_or(column_horizontal);
    let vertical = cell.valign.unwrap_or(column_vertical);

    if cell.style == Some(ColumnStyle::AsciiDoc) {
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

    match (cell.halign, cell.valign) {
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
    metadata: &BlockMetadata<'_>,
    kind: Option<CaptionKind>,
) -> bool {
    kind == Some(CaptionKind::Example) && metadata.options.contains(&"collapsible")
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

fn code_text_without_callout_guards(nodes: &[InlineNode<'_>]) -> String {
    let mut text = String::new();
    let transform = InlineTextTransform::default().line_break("\n");

    for (index, node) in nodes.iter().enumerate() {
        if let InlineNode::VerbatimText(verbatim) = node {
            let mut content = verbatim.content;
            if index
                .checked_sub(1)
                .is_some_and(|previous| is_xml_callout(nodes, previous))
            {
                content = content.strip_prefix("-->").unwrap_or(content);
            }
            if index
                .checked_add(1)
                .is_some_and(|next| matches!(nodes.get(next), Some(InlineNode::CalloutRef(_))))
            {
                if index
                    .checked_add(1)
                    .is_some_and(|next| is_xml_callout(nodes, next))
                {
                    content = content.strip_suffix("<!--").unwrap_or(content);
                }
                content = strip_pdf_callout_guard(content);
            }
            text.push_str(content);
        } else if let InlineNode::CalloutRef(callout) = node {
            let _ = write!(text, "({})", callout.number);
        } else {
            let _ = transform.write(&mut text, std::slice::from_ref(node));
        }
    }

    text
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

fn strip_pdf_callout_guard(text: &str) -> &str {
    let before_guard = text.strip_suffix(' ').unwrap_or(text);
    ["//", "#", "--", ";;"]
        .iter()
        .find_map(|guard| before_guard.strip_suffix(*guard))
        .unwrap_or(text)
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

fn inline_image_fallback_text(image: &Image<'_>) -> String {
    if !image.title.is_empty() {
        return inlines_to_string(image.title.as_ref());
    }
    image
        .metadata
        .attributes
        .get_string("alt")
        .map_or_else(|| format!("[image: {}]", image.source), Cow::into_owned)
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

fn block_image_default_alt(image: &Image<'_>) -> String {
    image
        .source
        .get_filename()
        .and_then(|filename| std::path::Path::new(filename).file_stem())
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .replace(['-', '_'], " ")
}

fn block_image_alt<'a>(image: &Image<'a>) -> Cow<'a, str> {
    image
        .metadata
        .attributes
        .get_string("alt")
        .unwrap_or_else(|| Cow::Owned(block_image_default_alt(image)))
}

fn inline_image_alt<'a>(image: &Image<'a>) -> Cow<'a, str> {
    if image.title.is_empty() {
        block_image_alt(image)
    } else {
        Cow::Owned(inlines_to_string(&image.title))
    }
}

fn block_image_alignment(metadata: &BlockMetadata<'_>) -> BlockImageAlignment {
    if let Some(side) = block_image_float(metadata) {
        return side;
    }
    if metadata.attributes.contains_key("align") {
        return metadata
            .attributes
            .get_string("align")
            .as_deref()
            .and_then(BlockImageAlignment::from_name)
            .unwrap_or_default();
    }
    metadata
        .roles
        .iter()
        .rev()
        .find_map(|role| BlockImageAlignment::from_name(role))
        .unwrap_or_default()
}

fn block_image_float(metadata: &BlockMetadata<'_>) -> Option<BlockImageAlignment> {
    metadata
        .attributes
        .get_string("float")
        .and_then(|value| BlockImageAlignment::from_name(&value))
        .filter(|alignment| *alignment != BlockImageAlignment::Center)
}

fn image_width(metadata: &BlockMetadata<'_>, supports_viewport_width: bool) -> Option<ImageWidth> {
    if let Some(value) = metadata.attributes.get("pdfwidth") {
        let value = if let AttributeValue::String(value) = value {
            value.as_ref()
        } else {
            ""
        };
        return Some(pdf_image_width(value, supports_viewport_width));
    }

    if let Some(scale) = metadata.attributes.get_string("scale") {
        return Some(ImageWidth::IntrinsicRatio(leading_number(&scale) / 100.0));
    }

    if let Some(scaledwidth) = metadata.attributes.get_string("scaledwidth") {
        if let Some(percentage) = scaledwidth.strip_suffix('%') {
            return Some(ImageWidth::ContainerRatio {
                value: leading_number(percentage) / 100.0,
                constrain_to_bounds: false,
            });
        }
        if scaledwidth.bytes().all(|byte| byte.is_ascii_digit()) {
            return Some(ImageWidth::ContainerRatio {
                value: leading_number(&scaledwidth) / 100.0,
                constrain_to_bounds: false,
            });
        }
        return Some(ImageWidth::Points {
            value: measurement_points(&scaledwidth),
            constrain_to_bounds: false,
        });
    }

    let width = metadata.attributes.get_string("width")?;
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

fn literal_table_cell_text(blocks: &[Block<'_>]) -> Option<String> {
    let mut text = String::new();
    for (index, block) in blocks.iter().enumerate() {
        let Block::Paragraph(paragraph) = block else {
            return None;
        };
        if index > 0 {
            text.push_str("\n\n");
        }
        let transform = InlineTextTransform::default().line_break("\n");
        let _ = transform.write(&mut text, &paragraph.content);
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
    use acdc_parser::{AttributeValue, BlockMetadata};

    fn metadata_with_width(width: &'static str) -> BlockMetadata<'static> {
        let mut metadata = BlockMetadata::default();
        metadata
            .attributes
            .set("width".into(), AttributeValue::String(width.into()));
        metadata
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
            image_width(&metadata_with_width("120"), true),
            Some(ImageWidth::Points {
                value: 90.0,
                constrain_to_bounds: true,
            })
        );
        assert_eq!(
            image_width(&metadata_with_width("40%"), true),
            Some(ImageWidth::ContainerRatio {
                value: 0.4,
                constrain_to_bounds: true,
            })
        );
        assert_eq!(
            image_width(&metadata_with_width("150%"), true),
            Some(ImageWidth::ContainerRatio {
                value: 1.0,
                constrain_to_bounds: true,
            })
        );
        for ignored in ["120px", "12.5", "-1", "invalid%"] {
            assert_eq!(image_width(&metadata_with_width(ignored), true), None);
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
        let mut metadata = metadata_with_width("40");
        metadata
            .attributes
            .set("pdfwidth".into(), AttributeValue::String("60".into()));
        assert_eq!(
            image_width(&metadata, true),
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
        let mut metadata = metadata_with_width("120");
        metadata
            .attributes
            .set("scaledwidth".into(), AttributeValue::String("40%".into()));
        metadata
            .attributes
            .set("scale".into(), AttributeValue::String("25".into()));
        metadata
            .attributes
            .set("pdfwidth".into(), AttributeValue::String("72pt".into()));

        assert_eq!(
            image_width(&metadata, true),
            Some(ImageWidth::Points {
                value: 72.0,
                constrain_to_bounds: false,
            })
        );
        metadata.attributes.remove("pdfwidth");
        assert_eq!(
            image_width(&metadata, true),
            Some(ImageWidth::IntrinsicRatio(0.25))
        );
        metadata.attributes.remove("scale");
        assert_eq!(
            image_width(&metadata, true),
            Some(ImageWidth::ContainerRatio {
                value: 0.4,
                constrain_to_bounds: false,
            })
        );
        metadata
            .attributes
            .set("scaledwidth".into(), AttributeValue::String("1in".into()));
        assert_eq!(
            image_width(&metadata, true),
            Some(ImageWidth::Points {
                value: 72.0,
                constrain_to_bounds: false,
            })
        );
    }

    #[test]
    fn pdfwidth_supports_container_intrinsic_and_block_viewport_units() {
        let metadata_with_pdfwidth = |width: &'static str| {
            let mut metadata = BlockMetadata::default();
            metadata
                .attributes
                .set("pdfwidth".into(), AttributeValue::String(width.into()));
            metadata
        };

        assert_eq!(
            image_width(&metadata_with_pdfwidth("200%"), true),
            Some(ImageWidth::ContainerRatio {
                value: 2.0,
                constrain_to_bounds: false,
            })
        );
        assert_eq!(
            image_width(&metadata_with_pdfwidth("50iw"), true),
            Some(ImageWidth::IntrinsicRatio(0.5))
        );
        assert_eq!(
            image_width(&metadata_with_pdfwidth("50vw"), true),
            Some(ImageWidth::ViewportRatio(0.5))
        );
        assert_eq!(
            image_width(&metadata_with_pdfwidth("50vw"), false),
            Some(ImageWidth::Points {
                value: 50.0,
                constrain_to_bounds: false,
            })
        );
    }

    #[test]
    fn block_image_alignment_follows_float_attribute_and_role_precedence() {
        let mut metadata = BlockMetadata::default();
        assert_eq!(block_image_alignment(&metadata), BlockImageAlignment::Left);

        metadata.roles = vec!["left", "unrelated", "right"];
        assert_eq!(block_image_alignment(&metadata), BlockImageAlignment::Right);

        metadata
            .attributes
            .set("align".into(), AttributeValue::String("center".into()));
        assert_eq!(
            block_image_alignment(&metadata),
            BlockImageAlignment::Center
        );

        metadata
            .attributes
            .set("align".into(), AttributeValue::String("invalid".into()));
        assert_eq!(block_image_alignment(&metadata), BlockImageAlignment::Left);

        metadata
            .attributes
            .set("float".into(), AttributeValue::String("right".into()));
        assert_eq!(block_image_alignment(&metadata), BlockImageAlignment::Right);

        metadata
            .attributes
            .set("float".into(), AttributeValue::String("invalid".into()));
        metadata
            .attributes
            .set("align".into(), AttributeValue::String("center".into()));
        assert_eq!(
            block_image_alignment(&metadata),
            BlockImageAlignment::Center
        );
    }

    #[test]
    fn table_break_ranges_select_only_long_uninterrupted_runs() {
        let ranges = long_unbreakable_ranges("short abcdefghij after", 8);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges.first(), Some(&(6..16)));
        assert!(long_unbreakable_ranges("one two three", 8).is_empty());
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
