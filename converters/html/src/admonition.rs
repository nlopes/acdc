use std::io::Write;

use acdc_converters_core::{
    TraversalContext,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{Admonition, AdmonitionVariant};

use crate::{Error, HtmlVariant, HtmlVisitor, build_class, write_id};

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    pub(crate) fn render_admonition(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        admon: &'a Admonition<'a>,
    ) -> Result<(), Error> {
        let processor = self.processor.clone();

        // Get the appropriate caption attribute for this admonition type
        // Note: Parser sets defaults, so these attributes are guaranteed to exist
        let caption_attr = match admon.variant {
            AdmonitionVariant::Note => "note-caption",
            AdmonitionVariant::Tip => "tip-caption",
            AdmonitionVariant::Important => "important-caption",
            AdmonitionVariant::Warning => "warning-caption",
            AdmonitionVariant::Caution => "caution-caption",
        };

        let caption = traversal
            .get(caption_attr)
            .and_then(|value| value.text())
            .ok_or(Error::InvalidAdmonitionCaption(caption_attr.to_string()))?
            .to_string();
        let plain_caption = admon.metadata.caption_content().map_or_else(
            || caption.clone(),
            |content| {
                acdc_converters_core::InlineTextTransform::default()
                    .rendered_replacements(true)
                    .to_string(content)
            },
        );
        let caption = match admon.metadata.caption_content() {
            Some(content) => self.content_html(traversal, content)?,
            None => crate::inlines::escape_attribute(&caption),
        };
        let icon = self.admonition_icon(traversal, admon, &plain_caption);

        if processor.variant() == HtmlVariant::Semantic {
            return visit_admonition_semantic(traversal, self, admon, &caption, icon.as_deref());
        }

        let class = build_class(
            &format!("admonitionblock {}", admon.variant),
            &admon.metadata.roles,
        );
        write!(self.writer, "<div")?;
        write_id(&mut self.writer, &admon.metadata)?;
        writeln!(self.writer, " class=\"{class}\">")?;
        writeln!(self.writer, "<table>")?;
        writeln!(self.writer, "<tr>")?;
        writeln!(self.writer, "<td class=\"icon\">")?;

        if let Some(icon) = icon {
            writeln!(self.writer, "{icon}")?;
        } else {
            writeln!(self.writer, "<div class=\"title\">{caption}</div>")?;
        }
        writeln!(self.writer, "</td>")?;
        writeln!(self.writer, "<td class=\"content\">")?;
        if admon.title.is_present() {
            write!(self.writer, "<div class=\"title\">")?;
            self.visit_title(traversal, &admon.title)?;
            writeln!(self.writer, "</div>")?;
        }

        // Handle paragraph rendering based on block count
        // Single paragraph: wrap in <div class="paragraph"><p>...</p></div>
        // Multiple blocks: render each with normal wrapper
        match admon.blocks.as_slice() {
            [acdc_parser::Block::Paragraph(para)] => {
                writeln!(self.writer, "<div class=\"paragraph\">")?;
                write!(self.writer, "<p>")?;
                self.visit_inline_nodes(traversal, &para.content)?;
                writeln!(self.writer, "</p>")?;
                writeln!(self.writer, "</div>")?;
            }
            [block] => {
                // Single non-paragraph block: use normal rendering
                traversal.visit_block(self, block)?;
            }
            blocks => {
                // Multiple blocks: use normal rendering for all
                for block in blocks {
                    traversal.visit_block(self, block)?;
                }
            }
        }

        writeln!(self.writer, "</td>")?;
        writeln!(self.writer, "</tr>")?;
        writeln!(self.writer, "</table>")?;
        writeln!(self.writer, "</div>")?;
        Ok(())
    }
}

/// Render an admonition block in semantic HTML5 mode.
fn visit_admonition_semantic<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
    admon: &'a Admonition<'a>,
    caption: &str,
    icon: Option<&str>,
) -> Result<(), Error> {
    // Note/Tip use <aside> with role="note"/"doc-tip"
    // Warning/Important/Caution use <section> with role="doc-notice"
    let (tag, role) = match admon.variant {
        AdmonitionVariant::Note => ("aside", "note"),
        AdmonitionVariant::Tip => ("aside", "doc-tip"),
        AdmonitionVariant::Warning | AdmonitionVariant::Important | AdmonitionVariant::Caution => {
            ("section", "doc-notice")
        }
    };

    let mut writer = visitor.writer_mut();
    // Build class: "admonition-block {variant}" + roles
    let base_class = format!("admonition-block {}", admon.variant);
    let class = crate::build_class(&base_class, &admon.metadata.roles);
    write!(writer, "<{tag} class=\"{class}\"")?;
    // Propagate id
    if let Some(id) = &admon.metadata.id {
        write!(writer, " id=\"{}\"", id.id)?;
    } else if let Some(anchor) = admon.metadata.anchors.first() {
        write!(writer, " id=\"{}\"", anchor.id)?;
    }
    writeln!(writer, " role=\"{role}\">")?;

    if let Some(icon) = icon {
        writeln!(writer, "<h6 class=\"block-title label-only\">{icon}</h6>")?;
        if admon.title.is_present() {
            write!(writer, "<h6 class=\"block-title\">")?;
            let _ = writer;
            visitor.visit_title(traversal, &admon.title)?;
            writer = visitor.writer_mut();
            writeln!(writer, "</h6>")?;
        }
    } else if !admon.title.is_present() {
        // Label-only: no trailing space after colon
        writeln!(
            writer,
            "<h6 class=\"block-title label-only\"><span class=\"title-label\">{caption}:</span></h6>"
        )?;
    } else {
        // With title: single h6 combining label + title (space after colon)
        write!(
            writer,
            "<h6 class=\"block-title\"><span class=\"title-label\">{caption}: </span>"
        )?;
        let _ = writer;
        visitor.visit_title(traversal, &admon.title)?;
        writer = visitor.writer_mut();
        writeln!(writer, "</h6>")?;
    }
    let _ = writer;

    // Render content blocks
    match admon.blocks.as_slice() {
        [acdc_parser::Block::Paragraph(para)] => {
            let writer = visitor.writer_mut();
            write!(writer, "<p>")?;
            let _ = writer;
            visitor.visit_inline_nodes(traversal, &para.content)?;
            let writer = visitor.writer_mut();
            writeln!(writer, "</p>")?;
        }
        [block] => {
            traversal.visit_block(visitor, block)?;
        }
        blocks => {
            for block in blocks {
                traversal.visit_block(visitor, block)?;
            }
        }
    }

    let writer = visitor.writer_mut();
    writeln!(writer, "</{tag}>")?;
    Ok(())
}

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    fn admonition_icon(
        &mut self,
        traversal: &TraversalContext<'a>,
        admon: &Admonition<'_>,
        caption: &str,
    ) -> Option<String> {
        use acdc_converters_core::icon::{IconMode, image_source};
        let caption = crate::inlines::escape_attribute(caption);
        let custom = admon.metadata.attributes.get_string("icon");
        match IconMode::from_attributes(traversal) {
            IconMode::Text => None,
            IconMode::Font if custom.is_none() => {
                let glyph = match admon.variant {
                    AdmonitionVariant::Note => "fa-circle-info",
                    AdmonitionVariant::Tip => "fa-lightbulb",
                    AdmonitionVariant::Important => "fa-circle-exclamation",
                    AdmonitionVariant::Warning => "fa-triangle-exclamation",
                    AdmonitionVariant::Caution => "fa-fire",
                };
                Some(format!(
                    "<i class=\"fa-solid {glyph}\" title=\"{caption}\"></i>"
                ))
            }
            IconMode::Font | IconMode::Image | _ => {
                let name = admon.variant.to_string();
                let target = custom.map_or_else(
                    || image_source(traversal, &acdc_parser::Source::Name(&name)),
                    |value| acdc_converters_core::icon::custom_image_source(traversal, &value),
                );
                let source = self.image_uri(&target, traversal);
                Some(format!(
                    "<img src=\"{}\" alt=\"{caption}\">",
                    crate::inlines::escape_attribute(&source)
                ))
            }
        }
    }
}
