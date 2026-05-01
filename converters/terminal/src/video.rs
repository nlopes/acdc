use std::io::Write;

use acdc_converters_core::{video::TryUrl, visitor::WritableVisitor};
use acdc_parser::Video;
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Error, TerminalVisitor};

impl<W: Write> TerminalVisitor<'_, '_, W> {
    pub(crate) fn render_video(&mut self, video: &Video) -> Result<(), Error> {
        if video.sources.is_empty() {
            return Ok(());
        }

        let url = video.try_url(false)?;
        let w = self.writer_mut();
        w.queue(PrintStyledContent(url.italic()))?;
        Ok(())
    }
}
