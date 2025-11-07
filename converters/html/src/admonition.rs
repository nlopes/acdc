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
            _ => None,
        })
        .expect("caption attribute should exist from parser defaults");

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
    for block in &admon.blocks {
        visitor.visit_block(block)?;
    }
    writer = visitor.writer_mut();
    writeln!(writer, "</td>")?;
    writeln!(writer, "</tr>")?;
    writeln!(writer, "</table>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}
