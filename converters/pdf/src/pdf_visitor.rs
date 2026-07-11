use std::{borrow::Cow, fmt::Write as _};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::apply_replacements;
use acdc_converters_core::{
    Diagnostics, InlineTextTransform, inlines_to_string,
    section::{AppendixTracker, SectionNumberTracker, SpecialSectionTracker},
    substitutions::Replacements,
    table::{build_grid, determine_column_count, table_has_spans},
    toc::Config as TocConfig,
    visitor::Visitor,
};
use acdc_parser::{
    Block, Image, IndexTermKind, InlineMacro, InlineNode, ListItem, Source, Table, TableOfContents,
    Title,
};
use acdc_pdf_images::ImageMap;
use acdc_pdf_typst::Writer;

use crate::{Error, Processor};

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
        self.write_text_expr(&text);
    }

    pub(crate) fn write_quoted_span(
        &mut self,
        prefix: &str,
        nodes: &[InlineNode<'_>],
        suffix: &str,
    ) -> Result<(), Error> {
        self.writer.raw(prefix);
        self.write_inlines(nodes)?;
        self.writer.raw(suffix);
        Ok(())
    }

    pub(crate) fn write_block_title(&mut self, title: &Title<'_>) -> Result<(), Error> {
        if title.is_empty() {
            return Ok(());
        }
        self.writer.raw("#text(weight: \"bold\")[");
        self.write_title(title)?;
        self.writer.raw("]\n");
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

    pub(crate) fn write_framed_blocks(
        &mut self,
        label: Option<&str>,
        blocks: &[Block<'_>],
    ) -> Result<(), Error> {
        self.writer
            .raw("#block(fill: luma(248), inset: 8pt, width: 100%)[\n");
        if let Some(label) = label {
            self.writer.raw("#text(weight: \"bold\")[");
            self.write_text_expr(label);
            self.writer.raw("]#linebreak()\n");
        }
        self.write_blocks(blocks)?;
        self.writer.raw("]\n\n");
        Ok(())
    }

    pub(crate) fn write_callout(&mut self, kind: &str, blocks: &[Block<'_>]) -> Result<(), Error> {
        self.writer.raw("#callout(");
        self.writer.string_literal(kind);
        self.writer.raw(")[\n");
        self.write_blocks(blocks)?;
        self.writer.raw("]\n\n");
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
        if table_has_spans(table) {
            self.warn_unsupported("table cell spans", "rendering cells without span geometry");
        }

        let column_count = determine_column_count(table);
        let _ = write!(self.writer, "#table(columns: {column_count}");
        for row in build_grid(table, column_count) {
            for cell in row.cells {
                match cell {
                    acdc_converters_core::table::CellKind::Content { cell_index } => {
                        if let Some(ast_cell) = row.ast_row.columns.get(cell_index) {
                            self.writer.raw(", [");
                            if row.is_header {
                                self.writer.raw("#tableheader[");
                            }
                            self.write_blocks(&ast_cell.content)?;
                            if ast_cell.content.is_empty() {
                                self.write_text_expr("");
                            }
                            if row.is_header {
                                self.writer.raw("]");
                            }
                            self.writer.raw("]");
                        }
                    }
                    acdc_converters_core::table::CellKind::HSpan
                    | acdc_converters_core::table::CellKind::VSpan => {
                        self.writer.raw(", []");
                    }
                }
            }
        }
        self.writer.raw(")\n\n");
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
                self.writer.raw("#footnote[");
                self.write_inlines(&footnote.content)?;
                self.writer.raw("]");
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
            InlineMacro::CrossReference(xref) => {
                let label = crate::encode_label(xref.target);
                let text = if xref.text.is_empty() {
                    self.processor
                        .document_attributes()
                        .get_string(xref.target)
                        .map_or_else(|| xref.target.to_string(), Cow::into_owned)
                } else {
                    inlines_to_string(&xref.text)
                };
                let _ = write!(self.writer, "#link(<{label}>)[");
                self.write_text_expr(&text);
                self.writer.raw("]");
            }
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
