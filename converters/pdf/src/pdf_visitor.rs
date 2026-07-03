use std::{borrow::Cow, fmt::Write};

use acdc_converters_core::{
    Diagnostics, InlineTextTransform, inlines_to_string,
    section::{AppendixTracker, SectionNumberTracker, SpecialSectionTracker},
    substitutions::Replacements,
    table::{build_grid, determine_column_count, table_has_spans},
    visitor::Visitor,
};

use acdc_parser::{
    Block, Document, IndexTermKind, InlineMacro, InlineNode, ListItem, Source, Table, Title,
};

use crate::{Error, Processor, typst_string};

pub(crate) struct PdfVisitor<'a, 'd> {
    pub(crate) source: String,
    pub(crate) processor: Processor<'a>,
    diagnostics: Diagnostics<'d>,
    pub(crate) section_number_tracker: SectionNumberTracker,
    pub(crate) appendix_tracker: AppendixTracker,
    pub(crate) special_section_tracker: SpecialSectionTracker,
    pub(crate) list_depth: usize,
}

impl<'a, 'd> PdfVisitor<'a, 'd> {
    pub(crate) fn new(processor: Processor<'a>, diagnostics: Diagnostics<'d>) -> Self {
        let section_number_tracker = SectionNumberTracker::new(&processor.document_attributes);
        let appendix_tracker = AppendixTracker::new(
            &processor.document_attributes,
            section_number_tracker.clone(),
        );
        Self {
            source: String::new(),
            processor,
            diagnostics,
            section_number_tracker,
            appendix_tracker,
            special_section_tracker: SpecialSectionTracker::new(),
            list_depth: 0,
        }
    }

    pub(crate) fn write_preamble(&mut self, doc: &Document<'_>) {
        let lang = doc
            .attributes
            .get_string("lang")
            .unwrap_or(Cow::Borrowed("en"));
        let toc_enabled = doc.attributes.contains_key("toc");
        let page_size = doc
            .attributes
            .get_string("pdf-page-size")
            .unwrap_or(Cow::Borrowed("us-letter"));

        let _ = writeln!(
            self.source,
            "#set page(paper: {}, margin: (x: 0.8in, y: 0.8in))",
            typst_string(&page_size)
        );
        let _ = writeln!(
            self.source,
            "#set text(font: \"New Computer Modern\", size: 10pt, lang: {})",
            typst_string(&lang)
        );
        self.source.push_str(
            "#set par(justify: true, leading: 0.65em)\n\
             #set heading(supplement: none)\n\
             #show link: underline\n\n",
        );

        if toc_enabled {
            self.write_toc();
        }
    }

    pub(crate) fn write_toc(&mut self) {
        self.source
            .push_str("#outline(title: [Table of Contents])\n#pagebreak()\n\n");
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
        let _ = write!(self.source, "#text({})", typst_string(text));
    }

    pub(crate) fn write_plain(&mut self, text: &str) {
        self.write_text_expr(&Replacements::unicode().transform(text, false));
    }

    pub(crate) fn write_quoted_span(
        &mut self,
        prefix: &str,
        nodes: &[InlineNode<'_>],
        suffix: &str,
    ) -> Result<(), Error> {
        self.source.push_str(prefix);
        self.write_inlines(nodes)?;
        self.source.push_str(suffix);
        Ok(())
    }

    pub(crate) fn write_block_title(&mut self, title: &Title<'_>) -> Result<(), Error> {
        if title.is_empty() {
            return Ok(());
        }
        self.source.push_str("#text(weight: \"bold\")[");
        self.write_title(title)?;
        self.source.push_str("]\n");
        Ok(())
    }

    pub(crate) fn write_verbatim_block(&mut self, nodes: &[InlineNode<'_>]) {
        let text = InlineTextTransform::default()
            .line_break("\n")
            .to_string(nodes);
        let _ = writeln!(
            self.source,
            "#block(fill: luma(245), inset: 8pt, width: 100%)[#raw({}, block: true)]\n",
            typst_string(&text)
        );
    }

    pub(crate) fn write_framed_blocks(
        &mut self,
        label: Option<&str>,
        blocks: &[Block<'_>],
    ) -> Result<(), Error> {
        self.source
            .push_str("#block(fill: luma(248), inset: 8pt, width: 100%)[\n");
        if let Some(label) = label {
            let _ = writeln!(
                self.source,
                "#text(weight: \"bold\")[{}]#linebreak()",
                typst_string(label)
            );
        }
        self.write_blocks(blocks)?;
        self.source.push_str("]\n\n");
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
        let _ = write!(self.source, "{indent}{marker} ");
        if let Some(checked) = &item.checked {
            let checkbox = match checked {
                acdc_parser::ListItemCheckedStatus::Checked => "[x] ",
                acdc_parser::ListItemCheckedStatus::Unchecked => "[ ] ",
                _ => "",
            };
            self.write_text_expr(checkbox);
        }
        self.write_inlines(&item.principal)?;
        self.source.push('\n');

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
        let _ = write!(self.source, "#table(columns: {column_count}");
        for row in build_grid(table, column_count) {
            for cell in row.cells {
                match cell {
                    acdc_converters_core::table::CellKind::Content { cell_index } => {
                        if let Some(ast_cell) = row.ast_row.columns.get(cell_index) {
                            self.source.push_str(", [");
                            let was_empty = self.source.len();
                            self.write_blocks(&ast_cell.content)?;
                            if self.source.len() == was_empty {
                                self.write_text_expr("");
                            }
                            self.source.push(']');
                        }
                    }
                    acdc_converters_core::table::CellKind::HSpan
                    | acdc_converters_core::table::CellKind::VSpan => {
                        self.source.push_str(", []");
                    }
                }
            }
        }
        self.source.push_str(")\n\n");
        Ok(())
    }

    pub(crate) fn write_inline_macro(
        &mut self,
        inline_macro: &InlineMacro<'_>,
    ) -> Result<(), Error> {
        match inline_macro {
            InlineMacro::Footnote(footnote) => {
                self.source.push_str("#footnote[");
                self.write_inlines(&footnote.content)?;
                self.source.push(']');
            }
            InlineMacro::Icon(icon) => {
                self.warn_unsupported("inline icons", "rendering the icon name as text");
                self.write_text_expr(&format!("[icon: {}]", icon.target));
            }
            InlineMacro::Image(img) => {
                self.warn_unsupported("inline images", "rendering the image target as text");
                self.write_text_expr(&format!("[image: {}]", img.source));
            }
            InlineMacro::Keyboard(keyboard) => {
                let joined = keyboard.keys.join("+");
                let _ = write!(self.source, "#raw({})", typst_string(&joined));
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
                let label = crate::sanitize_label(xref.target);
                let text = if xref.text.is_empty() {
                    self.processor
                        .document_attributes
                        .get_string(xref.target)
                        .map_or_else(|| xref.target.to_string(), Cow::into_owned)
                } else {
                    inlines_to_string(&xref.text)
                };
                let _ = write!(
                    self.source,
                    "#link(<{}>)[#text({})]",
                    label,
                    typst_string(&text)
                );
            }
            InlineMacro::Pass(pass) => {
                if let Some(text) = pass.text {
                    self.write_text_expr(text);
                }
            }
            InlineMacro::Stem(stem) => {
                let _ = write!(self.source, "$ {} $", crate::escape_math(stem.content));
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
        let _ = write!(self.source, "#link({})[", typst_string(target));
        if text.is_empty() {
            self.write_text_expr(target);
        } else {
            self.write_inlines(text)?;
        }
        self.source.push(']');
        Ok(())
    }
}
