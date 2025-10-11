use std::io::Write;

use acdc_parser::ThematicBreak;

use crate::{FALLBACK_TERMINAL_WIDTH, Processor, Render};

impl Render for ThematicBreak {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, _processor: &Processor) -> Result<(), Self::Error> {
        let width = crossterm::terminal::size()
            .map(|(cols, _)| usize::from(cols))
            .unwrap_or(FALLBACK_TERMINAL_WIDTH);
        writeln!(w, "{}", "─".repeat(width))?;
        Ok(())
    }
}
