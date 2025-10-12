use std::io::Write;

use acdc_converters_common::video::TryUrl;
use acdc_parser::Video;
use crossterm::{queue, style::PrintStyledContent, style::Stylize};

use crate::{Processor, Render};

impl Render for Video {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, _processor: &Processor) -> Result<(), Self::Error> {
        if self.sources.is_empty() {
            return Ok(());
        }

        let url = self.try_url(false)?;
        queue!(w, PrintStyledContent(url.italic()))?;
        Ok(())
    }
}
