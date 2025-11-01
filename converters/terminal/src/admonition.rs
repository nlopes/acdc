use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{Admonition, AdmonitionVariant};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::Error;

/// Visit an admonition block (NOTE, TIP, IMPORTANT, WARNING, CAUTION).
///
/// Renders with a styled variant label followed by title and content.
pub(crate) fn visit_admonition<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    admon: &Admonition,
) -> Result<(), Error> {
    let w = visitor.writer_mut();
    writeln!(w)?;

    // Get styled variant label
    let variant_str = format!("{}", admon.variant).to_uppercase();
    let styled_variant = match admon.variant {
        AdmonitionVariant::Note => variant_str.blue().bold(),
        AdmonitionVariant::Tip => variant_str.green().bold(),
        AdmonitionVariant::Important => variant_str.yellow().bold(),
        AdmonitionVariant::Warning => variant_str.red().bold(),
        AdmonitionVariant::Caution => variant_str.magenta().bold(),
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
