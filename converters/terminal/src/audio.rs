use std::io::Write;

use acdc_parser::{Audio, Source};
use crossterm::{queue, style::PrintStyledContent, style::Stylize};

use crate::{Processor, Render};

impl Render for Audio {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, _processor: &Processor) -> Result<(), Self::Error> {
        match &self.source {
            Source::Url(url) => {
                queue!(w, PrintStyledContent(format!("[Audio: {url}]").italic()))?;
            }
            Source::Path(path) | Source::Name(path) => {
                queue!(w, PrintStyledContent(format!("[Audio: {path}]").italic()))?;
            }
        }
        Ok(())
    }
}
