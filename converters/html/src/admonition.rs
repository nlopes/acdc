use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{Admonition, AdmonitionVariant, AttributeValue};

use crate::{Error, Processor};

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

    let mut writer = visitor.writer_mut();
    writeln!(writer, "<div class=\"admonitionblock {}\">", admon.variant)?;
    writeln!(writer, "<table>")?;
    writeln!(writer, "<tr>")?;
    writeln!(writer, "<td class=\"icon\">")?;
    writeln!(writer, "<div class=\"title\">{caption}</div>")?;
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
    // Single paragraph: render content directly (no wrapper)
    // Multiple blocks: render each with normal wrapper
    match admon.blocks.as_slice() {
        [acdc_parser::Block::Paragraph(para)] => {
            // Single paragraph: render inline content directly without wrapper
            visitor.visit_inline_nodes(&para.content)?;
            writer = visitor.writer_mut();
            writeln!(writer)?;
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
