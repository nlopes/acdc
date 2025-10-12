use std::io::Write;

use acdc_parser::{Image, Source};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Processor, Render};

impl Render for Image {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, _processor: &Processor) -> Result<(), Self::Error> {
        match &self.source {
            Source::Url(url) => {
                w.queue(PrintStyledContent(format!("[Image: {url}]").italic()))?;
            }
            Source::Path(path) => {
                let conf = viuer::Config::default();
                viuer::print_from_file(path, &conf).unwrap_or_else(|e| {
                    tracing::warn!(?path, ?e, "Failed to display image");
                    (0, 0)
                });
                w.flush()?;
            }
            Source::Name(name) => {
                w.queue(PrintStyledContent(format!("[Image: {name}]").italic()))?;
            }
        }
        Ok(())
    }
}
