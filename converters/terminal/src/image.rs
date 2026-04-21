use std::io::Write;

use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::{Image, Source};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Error, TerminalVisitor};

impl<W: Write> TerminalVisitor<'_, W> {
    pub(crate) fn render_image(&mut self, img: &Image) -> Result<(), Error> {
        let w = self.writer_mut();
        match &img.source {
            Source::Url(url) => {
                w.queue(PrintStyledContent(format!("[Image: {url}]").italic()))?;
            }
            Source::Path(path) => {
                #[cfg(feature = "images")]
                {
                    // Keep this flush else the image will render before any text in the
                    // buffer.
                    w.flush()?;
                    let conf = viuer::Config::default();
                    viuer::print_from_file(path, &conf).unwrap_or_else(|e| {
                        tracing::warn!(?path, ?e, "Failed to display image");
                        (0, 0)
                    });
                    w.flush()?;
                }
                #[cfg(not(feature = "images"))]
                {
                    let display = path.display();
                    w.queue(PrintStyledContent(format!("[Image: {display}]").italic()))?;
                }
            }
            Source::Name(name) => {
                w.queue(PrintStyledContent(format!("[Image: {name}]").italic()))?;
            }
        }
        Ok(())
    }
}
