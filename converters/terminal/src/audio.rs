use std::io::Write;

use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::{Audio, Source};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Error, TerminalVisitor};

impl<W: Write> TerminalVisitor<'_, '_, W> {
    pub(crate) fn render_audio(&mut self, audio: &Audio) -> Result<(), Error> {
        let w = self.writer_mut();
        match &audio.source {
            Source::Url(url) => {
                w.queue(PrintStyledContent(url.as_ref().to_string().italic()))?;
            }
            Source::Path(path) => {
                w.queue(PrintStyledContent(
                    format!("[Audio: {}]", path.display()).italic(),
                ))?;
            }
            Source::Name(name) => {
                w.queue(PrintStyledContent(format!("[Audio: {name}]").italic()))?;
            }
        }
        Ok(())
    }
}
