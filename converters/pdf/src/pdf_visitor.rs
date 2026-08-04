use std::{borrow::Cow, fmt::Write as _, rc::Rc};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::{apply_replacements, effective_subs_flags};
use acdc_converters_core::{
    Diagnostics, InlineTextTransform, inlines_to_string,
    section::{AppendixTracker, SectionNumberTracker, SpecialSectionTracker},
    substitutions::Replacements,
    table::{CellKind, GridRow, build_grid, determine_column_count},
    toc::Config as TocConfig,
    visitor::Visitor,
    xref::{XrefDisplay, resolve_xref},
};
use acdc_parser::{
    Anchor, Block, BlockMetadata, CrossReference, Image, IndexTermKind, InlineMacro, InlineNode,
    ListItem, Paragraph, Source, Table, TableColumn, TableOfContents, Title,
};
use acdc_pdf_images::ImageMap;
use acdc_pdf_typst::Writer;

use crate::{Error, Processor, encode_footnote_label};

pub(crate) struct PdfVisitor<'a, 'd, 'm> {
    pub(crate) writer: Writer,
    pub(crate) processor: Processor<'a>,
    assets: &'m ImageMap,
    diagnostics: Diagnostics<'d>,
    pub(crate) section_number_tracker: SectionNumberTracker,
    pub(crate) appendix_tracker: AppendixTracker,
    pub(crate) special_section_tracker: SpecialSectionTracker,
    pub(crate) list_depth: usize,
    has_toc_entries: bool,
    toc_written: bool,
}

impl<'a, 'd, 'm> PdfVisitor<'a, 'd, 'm> {
    pub(crate) fn new(
        processor: Processor<'a>,
        assets: &'m ImageMap,
        has_toc_entries: bool,
        diagnostics: Diagnostics<'d>,
    ) -> Self {
        let section_number_tracker = SectionNumberTracker::new(processor.document_attributes());
        let appendix_tracker = AppendixTracker::new(
            processor.document_attributes(),
            section_number_tracker.clone(),
        );
        Self {
            writer: Writer::new(),
            processor,
            assets,
            diagnostics,
            section_number_tracker,
            appendix_tracker,
            special_section_tracker: SpecialSectionTracker::new(),
            list_depth: 0,
            has_toc_entries,
            toc_written: false,
        }
    }

    pub(crate) fn render_toc(&mut self, toc_macro: Option<&TableOfContents<'_>>, placement: &str) {
        if self.toc_written || !self.has_toc_entries {
            return;
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
            return;
        }

        self.toc_written = true;
        self.writer.raw("#outline(title: ");
        match config.title() {
            Some("") | None => self.writer.raw("none"),
            Some(title) => self.writer.string_literal(title),
        }
        let _ = write!(
            self.writer,
            ", depth: {})\n#pagebreak()\n\n",
            config.levels()
        );
    }

    pub(crate) fn write_blocks(&mut self, blocks: &[Block<'_>]) -> Result<(), Error> {
        for block in blocks {
            self.visit_block(block)?;
        }
        Ok(())
    }

    pub(crate) fn write_title(&mut self, title: &Title<'_>) -> Result<(), Error> {
        if !title.is_empty() {
            self.write_inlines(title.as_ref())?;
        }
        Ok(())
    }

    pub(crate) fn write_inlines(&mut self, nodes: &[InlineNode<'_>]) -> Result<(), Error> {
        for node in nodes {
            self.visit_inline_node(node)?;
        }
        Ok(())
    }

    pub(crate) fn write_text_expr(&mut self, text: &str) {
        self.writer.raw("#text(");
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
    ) -> usize {
        if let Some(id) = id {
            let _ = write!(self.writer, "#metadata(none) <{}>", crate::encode_label(id));
        }

        let mut wrappers = 0;
        for role in role.into_iter().flat_map(str::split_whitespace) {
            let prefix = match role {
                "line-through" => Some("#strike["),
                "underline" => Some("#underline["),
                "overline" => Some("#overline["),
                "big" => Some("#text(size: 1.2em)["),
                "small" => Some("#text(size: 0.8em)["),
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
        wrappers
    }

    pub(crate) fn write_inline_span_end(&mut self, wrappers: usize) {
        for _ in 0..wrappers {
            self.writer.raw("]");
        }
    }

    pub(crate) fn write_plain(&mut self, text: &str) {
        #[cfg(feature = "pre-spec-subs")]
        let text = apply_replacements(
            text,
            self.processor.current_subs.get(),
            &Replacements::unicode(),
            false,
        );
        #[cfg(not(feature = "pre-spec-subs"))]
        let text = Cow::Owned(Replacements::unicode().transform(text, false));
        let text = collapse_source_whitespace(&text);
        self.write_text_expr(&text);
    }

    pub(crate) fn write_quoted_span(
        &mut self,
        id: Option<&str>,
        role: Option<&str>,
        prefix: &str,
        nodes: &[InlineNode<'_>],
        suffix: &str,
    ) -> Result<(), Error> {
        let wrappers = self.write_inline_span_start(id, role);
        self.writer.raw(prefix);
        self.write_inlines(nodes)?;
        self.writer.raw(suffix);
        self.write_inline_span_end(wrappers);
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
        self.writer
            .raw("#block(below: 0.5em)[#text(weight: \"bold\")[");
        self.write_title(title)?;
        self.writer.raw("]]\n");
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
            if write_title {
                self.write_block_title(&para.title)?;
            }
            self.write_paragraph_body(para)
        })();

        #[cfg(feature = "pre-spec-subs")]
        self.processor.current_subs.set(previous_subs);
        result
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

    /// Write verse content, which keeps its line breaks and stays proportional.
    pub(crate) fn write_verse_block(
        &mut self,
        nodes: &[InlineNode<'_>],
        metadata: &BlockMetadata<'_>,
    ) -> Result<(), Error> {
        let text = InlineTextTransform::default()
            .line_break("\n")
            .to_string(nodes);
        self.writer.raw("#verse[");
        self.write_text_expr(&text);
        self.writer.raw("]\n\n");
        self.write_attribution(metadata)
    }

    /// Write a paragraph's content in the shape its style asks for.
    ///
    /// A `[quote]`, `[verse]`, `[literal]`, `[listing]`, `[source]`, or
    /// `[example]` paragraph reads as its delimited counterpart, matching
    /// `asciidoctor`.
    fn write_paragraph_body(&mut self, para: &Paragraph<'_>) -> Result<(), Error> {
        match para.metadata.style {
            Some("quote") => {
                self.writer.raw("#blockquote[\n");
                self.write_inlines(&para.content)?;
                self.writer.raw("\n]\n\n");
                self.write_attribution(&para.metadata)
            }
            Some("verse") => self.write_verse_block(&para.content, &para.metadata),
            Some("literal" | "listing" | "source") => {
                self.write_verbatim_block(&para.content);
                Ok(())
            }
            Some("example") => {
                self.writer.raw("#examplebox[\n");
                self.write_inlines(&para.content)?;
                self.writer.raw("\n]\n\n");
                Ok(())
            }
            _ => {
                self.write_inlines(&para.content)?;
                self.writer.raw("\n\n");
                Ok(())
            }
        }
    }

    /// Write an example block under its numbered caption.
    ///
    /// A titled example reads as `Example 1. Title`, using the
    /// `example-caption` attribute, the same numbering the other backends
    /// apply. An untitled example takes no caption.
    pub(crate) fn write_example(
        &mut self,
        title: &Title<'_>,
        blocks: &[Block<'_>],
    ) -> Result<(), Error> {
        if !title.is_empty() {
            let caption = self
                .processor
                .document_attributes()
                .get("example-caption")
                .map_or_else(|| "Example".to_string(), ToString::to_string);
            let number = self.processor.example_counter.get() + 1;
            self.processor.example_counter.set(number);
            self.writer
                .raw("#block(below: 0.5em)[#text(weight: \"bold\")[");
            self.write_text_expr(&format!("{caption} {number}. "));
            self.write_title(title)?;
            self.writer.raw("]]\n");
        }
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

    pub(crate) fn write_verbatim_block(&mut self, nodes: &[InlineNode<'_>]) {
        let text = InlineTextTransform::default()
            .line_break("\n")
            .to_string(nodes);
        self.writer.raw("#raw(block: true, ");
        #[cfg(feature = "pre-spec-subs")]
        {
            let text = apply_replacements(
                &text,
                self.processor.current_subs.get(),
                &Replacements::unicode(),
                true,
            );
            self.writer.string_literal(&text);
        }
        #[cfg(not(feature = "pre-spec-subs"))]
        self.writer.string_literal(&text);
        self.writer.raw(")\n\n");
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
            let _ = write!(self.writer, "#{frame}[\n");
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

    pub(crate) fn write_list_item(
        &mut self,
        marker: &str,
        item: &ListItem<'_>,
    ) -> Result<(), Error> {
        let indent = "  ".repeat(self.list_depth);
        let _ = write!(self.writer, "{indent}{marker} ");
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

        if !item.blocks.is_empty() {
            self.list_depth += 1;
            for block in &item.blocks {
                self.visit_block(block)?;
            }
            self.list_depth -= 1;
        }
        Ok(())
    }

    pub(crate) fn write_table(&mut self, table: &Table<'_>) -> Result<(), Error> {
        let column_count = determine_column_count(table);
        let _ = write!(self.writer, "#table(columns: {column_count}");

        let grid = build_grid(table, column_count);
        if let Some(header) = grid.first().filter(|row| row.is_header) {
            self.writer.raw(", table.header(repeat: true, ");
            self.write_table_row_cells(header, 0, "")?;
            self.writer.raw(")");
        }

        // Typst owns the merged geometry. Emit each real cell at its logical
        // position and omit the grid's horizontal/vertical span placeholders.
        for (y, row) in grid.iter().enumerate().filter(|(_, row)| !row.is_header) {
            self.write_table_row_cells(row, y, ", ")?;
        }

        self.writer.raw(")\n\n");
        Ok(())
    }

    fn write_table_row_cells(
        &mut self,
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
                self.write_table_cell(ast_cell, x, y, row.is_header)?;
                separator = ", ";
            }
        }
        Ok(())
    }

    fn write_table_cell(
        &mut self,
        cell: &TableColumn<'_>,
        x: usize,
        y: usize,
        is_header: bool,
    ) -> Result<(), Error> {
        let _ = write!(self.writer, "table.cell(x: {x}, y: {y}");
        if cell.colspan > 1 {
            let _ = write!(self.writer, ", colspan: {}", cell.colspan);
        }
        if cell.rowspan > 1 {
            let _ = write!(self.writer, ", rowspan: {}", cell.rowspan);
        }
        self.writer.raw(")[");
        if is_header {
            self.writer.raw("#tableheader[");
        }
        self.write_blocks(&cell.content)?;
        if cell.content.is_empty() {
            self.write_text_expr("");
        }
        if is_header {
            self.writer.raw("]");
        }
        self.writer.raw("]");
        Ok(())
    }

    pub(crate) fn write_block_image(&mut self, image: &Image<'_>) -> Result<(), Error> {
        self.write_block_title(&image.title)?;
        let source = image.source.to_string();
        if let Some(asset) = self.assets.get(&source) {
            self.writer.raw("#docimage(");
            self.writer.string_literal(&asset.virtual_path);
            self.writer.raw(")\n\n");
        } else {
            self.write_text_expr(&image_fallback_text(image));
            self.writer.raw("\n\n");
        }
        Ok(())
    }

    pub(crate) fn write_inline_image(&mut self, image: &Image<'_>) {
        let source = image.source.to_string();
        if let Some(asset) = self.assets.get(&source) {
            self.writer.raw("#image(");
            self.writer.string_literal(&asset.virtual_path);
            self.writer.raw(", height: 1em)");
        } else {
            self.write_text_expr(&image_fallback_text(image));
        }
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
            InlineMacro::Icon(icon) => {
                self.warn_unsupported("inline icons", "rendering the icon name as text");
                self.write_text_expr(&format!("[icon: {}]", icon.target));
            }
            InlineMacro::Image(image) => self.write_inline_image(image),
            InlineMacro::Keyboard(keyboard) => {
                let joined = keyboard.keys.join("+");
                self.writer.raw("#raw(");
                self.writer.string_literal(&joined);
                self.writer.raw(")");
            }
            InlineMacro::Button(button) => self.write_text_expr(button.label),
            InlineMacro::Menu(menu) => {
                let mut parts = Vec::with_capacity(menu.items.len() + 1);
                parts.push(menu.target);
                parts.extend(menu.items.iter().copied());
                self.write_text_expr(&parts.join(" > "));
            }
            InlineMacro::Url(url) => self.write_link(&url.target, &url.text)?,
            InlineMacro::Link(link) => self.write_link(&link.target, &link.text)?,
            InlineMacro::Mailto(mailto) => {
                let target = format!("mailto:{}", mailto.target);
                self.write_link_text(&target, &mailto.text)?;
            }
            InlineMacro::Autolink(autolink) => {
                let target = autolink.url.to_string();
                self.write_link_text(&target, &[])?;
            }
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
                if let IndexTermKind::Flow(text) = &term.kind {
                    self.write_text_expr(text);
                }
            }
            _ => {}
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

        if !xref.text.is_empty() {
            if references.contains_key(xref.target) {
                self.write_labelled_link(xref.target, |visitor| visitor.write_inlines(&xref.text))?;
            } else {
                self.write_inlines(&xref.text)?;
            }
            return Ok(());
        }

        match resolve_xref(references.get(xref.target), xref.target, &guard) {
            XrefDisplay::Title(inlines, _scope) | XrefDisplay::Label(inlines, _scope) => {
                self.write_labelled_link(xref.target, |visitor| visitor.write_inlines(inlines))?;
            }
            XrefDisplay::Fallback(text) => {
                self.write_labelled_link(xref.target, |visitor| {
                    visitor.write_text_expr(&text);
                    Ok(())
                })?;
            }
            XrefDisplay::Unresolved(text) | XrefDisplay::Nested(text) => {
                self.write_text_expr(&text);
            }
        }
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

    fn write_link(&mut self, target: &Source<'_>, text: &[InlineNode<'_>]) -> Result<(), Error> {
        self.write_link_text(&target.to_string(), text)
    }

    fn write_link_text(&mut self, target: &str, text: &[InlineNode<'_>]) -> Result<(), Error> {
        self.writer.raw("#link(");
        self.writer.string_literal(target);
        self.writer.raw(")[");
        if text.is_empty() {
            self.write_text_expr(target);
        } else {
            self.write_inlines(text)?;
        }
        self.writer.raw("]");
        Ok(())
    }
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

fn image_fallback_text(image: &Image<'_>) -> String {
    if !image.title.is_empty() {
        return inlines_to_string(image.title.as_ref());
    }
    image
        .metadata
        .attributes
        .get_string("alt")
        .map_or_else(|| format!("[image: {}]", image.source), Cow::into_owned)
}
