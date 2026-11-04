use std::io::Write;

use acdc_converters_core::TraversalContext;
use acdc_parser::{
    AdmonitionVariant,
    ast::{AdmonitionRef, BlockKind},
};

use crate::{
    Error, HtmlVariant, HtmlVisitor, build_class, image_helpers::attribute_text, write_element_id,
};

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    pub(crate) fn render_admonition(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        admon: AdmonitionRef<'a>,
    ) -> Result<(), Error> {
        let processor = self.processor.clone();

        // Get the appropriate caption attribute for this admonition type
        // Note: Parser sets defaults, so these attributes are guaranteed to exist
        let caption_attr = match admon.variant() {
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
        let plain_caption = admon.caption_content().map_or_else(
            || caption.clone(),
            |content| {
                acdc_converters_core::InlineTextTransform::default()
                    .rendered()
                    .content_to_string(content)
            },
        );
        let caption = match admon.caption_content() {
            Some(content) => self.content_html(traversal, content)?,
            None => crate::inlines::escape_attribute(&caption),
        };
        let icon = self.admonition_icon(traversal, admon, &plain_caption);

        if processor.variant() == HtmlVariant::Semantic {
            return visit_admonition_semantic(traversal, self, admon, &caption, icon.as_deref());
        }

        let class = build_class(
            &format!("admonitionblock {}", admon.variant()),
            admon.roles().iter(),
        );
        write!(self.writer, "<div")?;
        write_element_id(&mut self.writer, admon.id())?;
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
        if admon.title().is_some() {
            write!(self.writer, "<div class=\"title\">")?;
            self.visit_title(traversal, admon.title())?;
            writeln!(self.writer, "</div>")?;
        }

        render_admonition_body(traversal, self, admon, false)?;

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
    admon: AdmonitionRef<'a>,
    caption: &str,
    icon: Option<&str>,
) -> Result<(), Error> {
    // Note/Tip use <aside> with role="note"/"doc-tip"
    // Warning/Important/Caution use <section> with role="doc-notice"
    let (tag, role) = match admon.variant() {
        AdmonitionVariant::Note => ("aside", "note"),
        AdmonitionVariant::Tip => ("aside", "doc-tip"),
        AdmonitionVariant::Warning | AdmonitionVariant::Important | AdmonitionVariant::Caution => {
            ("section", "doc-notice")
        }
    };

    let mut writer = visitor.writer_mut();
    // Build class: "admonition-block {variant}" + roles
    let base_class = format!("admonition-block {}", admon.variant());
    let class = crate::build_class(&base_class, admon.roles().iter());
    write!(writer, "<{tag} class=\"{class}\"")?;
    if let Some(id) = admon.id() {
        crate::write_element_id(writer, Some(id))?;
    }
    writeln!(writer, " role=\"{role}\">")?;

    if let Some(icon) = icon {
        writeln!(writer, "<h6 class=\"block-title label-only\">{icon}</h6>")?;
        if admon.title().is_some() {
            write!(writer, "<h6 class=\"block-title\">")?;
            let _ = writer;
            visitor.visit_title(traversal, admon.title())?;
            writer = visitor.writer_mut();
            writeln!(writer, "</h6>")?;
        }
    } else if admon.title().is_none() {
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
        visitor.visit_title(traversal, admon.title())?;
        writer = visitor.writer_mut();
        writeln!(writer, "</h6>")?;
    }
    let _ = writer;

    render_admonition_body(traversal, visitor, admon, true)?;

    let writer = visitor.writer_mut();
    writeln!(writer, "</{tag}>")?;
    Ok(())
}

fn render_admonition_body<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
    admon: AdmonitionRef<'a>,
    semantic: bool,
) -> Result<(), Error> {
    let mut blocks = admon.blocks().peekable();
    let Some(first) = blocks.next() else {
        return Ok(());
    };
    if let BlockKind::Paragraph(paragraph) = first.kind()
        && blocks.peek().is_none()
    {
        if admon.simple_paragraph().is_some() && !semantic {
            visitor.visit_content(traversal, paragraph.content())?;
            writeln!(visitor.writer_mut())?;
            return Ok(());
        }
        if !semantic {
            writeln!(visitor.writer_mut(), "<div class=\"paragraph\">")?;
        }
        write!(visitor.writer_mut(), "<p>")?;
        visitor.visit_content(traversal, paragraph.content())?;
        writeln!(visitor.writer_mut(), "</p>")?;
        if !semantic {
            writeln!(visitor.writer_mut(), "</div>")?;
        }
        Ok(())
    } else {
        traversal.visit_blocks(visitor, std::iter::once(first).chain(blocks))
    }
}

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    fn admonition_icon(
        &mut self,
        traversal: &TraversalContext<'a>,
        admon: AdmonitionRef<'_>,
        caption: &str,
    ) -> Option<String> {
        use acdc_converters_core::icon::{IconMode, image_source};
        let caption = crate::inlines::escape_attribute(caption);
        let custom = attribute_text(admon.attributes(), "icon");
        match IconMode::from_attributes(traversal) {
            IconMode::Text => None,
            IconMode::Font if custom.is_none() => {
                let glyph = match admon.variant() {
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
                let name = admon.variant().to_string();
                let target = custom.map_or_else(
                    || image_source(traversal, &acdc_parser::Source::Name(&name)),
                    |value| acdc_converters_core::icon::custom_image_source(traversal, value),
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
