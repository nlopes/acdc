use acdc_converters_common::visitor::{WritableVisitor, WritableVisitorExt};
use acdc_parser::{Admonition, AdmonitionVariant};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::Error;

/// Visit an admonition block (NOTE, TIP, IMPORTANT, WARNING, CAUTION).
///
/// Renders with icon/label and content in a visually distinct box.
pub(crate) fn visit_admonition<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    admon: &Admonition,
) -> Result<(), Error> {
    let w = visitor.writer_mut();
    writeln!(w)?;

    // Get styled variant label
    let variant_str = format!("{}", admon.variant);
    let styled_variant = match admon.variant {
        AdmonitionVariant::Note => variant_str.blue().bold(),
        AdmonitionVariant::Tip => variant_str.green().bold(),
        AdmonitionVariant::Important => variant_str.yellow().bold(),
        AdmonitionVariant::Warning => variant_str.red().bold(),
        AdmonitionVariant::Caution => variant_str.magenta().bold(),
    };

    // Top border with variant label
    write!(w, "  ╔═")?;
    QueueableCommand::queue(w, PrintStyledContent(styled_variant))?;
    writeln!(w, "═{}╗", "═".repeat(70))?;

    // Render title if present
    let _ = w;
    visitor.render_title_with_wrapper(&admon.title, "  ║ ", " ║\n")?;
    if !admon.title.is_empty() {
        let w = visitor.writer_mut();
        writeln!(w, "  ╠{}╣", "═".repeat(76))?;
    }

    // Render content blocks
    for block in &admon.blocks {
        let w = visitor.writer_mut();
        write!(w, "  ║ ")?;
        let _ = w;
        visitor.visit_block(block)?;
    }

    let w = visitor.writer_mut();
    writeln!(w, "  ╚{}╝", "═".repeat(76))?;

    Ok(())
}
