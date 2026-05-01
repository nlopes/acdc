use std::io::Write;

use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::{Image, Source};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Error, TerminalVisitor};

impl<W: Write> TerminalVisitor<'_, '_, W> {
    pub(crate) fn render_image(&mut self, img: &Image) -> Result<(), Error> {
        match &img.source {
            Source::Url(url) => {
                let w = self.writer_mut();
                w.queue(PrintStyledContent(format!("[Image: {url}]").italic()))?;
            }
            Source::Path(path) => {
                #[cfg(feature = "images")]
                {
                    // Keep this flush else the image will render before any text in the
                    // buffer.
                    self.writer_mut().flush()?;
                    let conf = viuer::Config::default();
                    if viuer::print_from_file(path, &conf).is_err() {
                        self.diagnostics.warn_with_advice(
                            format!("failed to display image `{}`", path.display()),
                            "Verify the image path is relative to the input document and that the terminal image feature can load it.",
                        );
                    }
                    self.writer_mut().flush()?;
                }
                #[cfg(not(feature = "images"))]
                {
                    let display = path.display();
                    let w = self.writer_mut();
                    w.queue(PrintStyledContent(format!("[Image: {display}]").italic()))?;
                }
            }
            Source::Name(name) => {
                let w = self.writer_mut();
                w.queue(PrintStyledContent(format!("[Image: {name}]").italic()))?;
            }
        }
        Ok(())
    }
}
