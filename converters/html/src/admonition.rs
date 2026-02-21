use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::{Admonition, AdmonitionVariant, AttributeValue};

use crate::{Error, HtmlVariant, Processor};

pub(crate) fn visit_admonition<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    admon: &Admonition,
    processor: &Processor,
) -> Result<(), Error> {
    // Get the appropriate caption attribute for this admonition type
    // Note: Parser sets defaults, so these attributes are guaranteed to exist
    let caption_attr = match admon.variant {
        AdmonitionVariant::Note => "note-caption",
        AdmonitionVariant::Tip => "tip-caption",
        AdmonitionVariant::Important => "important-caption",
        AdmonitionVariant::Warning => "warning-caption",
        AdmonitionVariant::Caution => "caution-caption",
    };

    let caption = processor
        .document_attributes
        .get(caption_attr)
        .and_then(|v| match v {
            AttributeValue::String(s) => Some(s.as_str()),
            AttributeValue::Bool(_) | AttributeValue::None | _ => None,
        })
        .ok_or(Error::InvalidAdmonitionCaption(caption_attr.to_string()))?;

    if processor.variant() == HtmlVariant::Semantic {
        return visit_admonition_semantic(visitor, admon, caption);
    }

    let mut writer = visitor.writer_mut();
    writeln!(writer, "<div class=\"admonitionblock {}\">", admon.variant)?;
    writeln!(writer, "<table>")?;
    writeln!(writer, "<tr>")?;
    writeln!(writer, "<td class=\"icon\">")?;

    // Output icon based on `:icons:` document attribute
    // - Font mode (`icons=font`): Use Font Awesome <i> element
    // - Default: Use text label in <div class="title">
    if processor.is_font_icons_mode() {
        let fa_icon = match admon.variant {
            AdmonitionVariant::Note => "fa-circle-info",
            AdmonitionVariant::Tip => "fa-lightbulb",
            AdmonitionVariant::Important => "fa-circle-exclamation",
            AdmonitionVariant::Warning => "fa-triangle-exclamation",
            AdmonitionVariant::Caution => "fa-fire",
        };
        writeln!(
            writer,
            "<i class=\"fa-solid {fa_icon}\" title=\"{caption}\"></i>",
        )?;
    } else {
        writeln!(writer, "<div class=\"title\">{caption}</div>")?;
    }
    writeln!(writer, "</td>")?;
    writeln!(writer, "<td class=\"content\">")?;
    if !admon.title.is_empty() {
        write!(writer, "<div class=\"title\">")?;
        let _ = writer;
        visitor.visit_inline_nodes(&admon.title)?;
        writer = visitor.writer_mut();
        writeln!(writer, "</div>")?;
    }
    let _ = writer;

    // Handle paragraph rendering based on block count
    // Single paragraph: wrap in <div class="paragraph"><p>...</p></div>
    // Multiple blocks: render each with normal wrapper
    match admon.blocks.as_slice() {
        [acdc_parser::Block::Paragraph(para)] => {
            writeln!(writer, "<div class=\"paragraph\">")?;
            write!(writer, "<p>")?;
            let _ = writer;
            visitor.visit_inline_nodes(&para.content)?;
            writer = visitor.writer_mut();
            writeln!(writer, "</p>")?;
            writeln!(writer, "</div>")?;
        }
        [block] => {
            // Single non-paragraph block: use normal rendering
            visitor.visit_block(block)?;
            writer = visitor.writer_mut();
        }
        blocks => {
            // Multiple blocks: use normal rendering for all
            for block in blocks {
                visitor.visit_block(block)?;
            }
            writer = visitor.writer_mut();
        }
    }

    writeln!(writer, "</td>")?;
    writeln!(writer, "</tr>")?;
    writeln!(writer, "</table>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}

/// Render an admonition block in semantic HTML5 mode.
fn visit_admonition_semantic<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    admon: &Admonition,
    caption: &str,
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

    if admon.title.is_empty() {
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
        visitor.visit_inline_nodes(&admon.title)?;
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
            visitor.visit_inline_nodes(&para.content)?;
            let writer = visitor.writer_mut();
            writeln!(writer, "</p>")?;
        }
        [block] => {
            visitor.visit_block(block)?;
        }
        blocks => {
            for block in blocks {
                visitor.visit_block(block)?;
            }
        }
    }

    let writer = visitor.writer_mut();
    writeln!(writer, "</{tag}>")?;
    Ok(())
}
