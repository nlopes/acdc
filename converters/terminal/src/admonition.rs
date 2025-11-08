use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{Admonition, AdmonitionVariant, AttributeValue};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Error, Processor};

/// Visit an admonition block (NOTE, TIP, IMPORTANT, WARNING, CAUTION).
///
/// Renders with a styled variant label followed by title and content.
pub(crate) fn visit_admonition<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    admon: &Admonition,
    processor: &Processor,
) -> Result<(), Error> {
    let w = visitor.writer_mut();
    writeln!(w)?;

    // Get the appropriate caption attribute for this admonition type
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
            AttributeValue::String(s) => Some(s.to_uppercase()),
            _ => None,
        })
        .ok_or(Error::InvalidAdmonitionCaption(caption_attr.to_string()))?;

    // Get styled caption label
    let styled_variant = match admon.variant {
        AdmonitionVariant::Note => caption.blue().bold(),
        AdmonitionVariant::Tip => caption.green().bold(),
        AdmonitionVariant::Important => caption.yellow().bold(),
        AdmonitionVariant::Warning => caption.red().bold(),
        AdmonitionVariant::Caution => caption.magenta().bold(),
    };

    // Write the variant label
    QueueableCommand::queue(w, PrintStyledContent(styled_variant))?;
    writeln!(w)?;

    // Render title if present
    if !admon.title.is_empty() {
        visitor.visit_inline_nodes(&admon.title)?;
        let w = visitor.writer_mut();
        writeln!(w)?;
        writeln!(w)?;
    }

    // Render content blocks
    for block in &admon.blocks {
        visitor.visit_block(block)?;
    }

    // End marker with three dots in the same color as the variant
    let w = visitor.writer_mut();
    let end_marker = "• • •";
    let styled_end_marker = match admon.variant {
        AdmonitionVariant::Note => end_marker.blue().bold(),
        AdmonitionVariant::Tip => end_marker.green().bold(),
        AdmonitionVariant::Important => end_marker.yellow().bold(),
        AdmonitionVariant::Warning => end_marker.red().bold(),
        AdmonitionVariant::Caution => end_marker.magenta().bold(),
    };
    QueueableCommand::queue(w, PrintStyledContent(styled_end_marker))?;
    writeln!(w)?;

    Ok(())
}
