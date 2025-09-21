use std::io::Write;

use acdc_parser::{Image, Source};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::Render;

impl Render for Image {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        match &self.source {
            Source::Url(url) => {
                w.queue(PrintStyledContent(format!("[Image: {url}]").italic()))?;
            }
            Source::Path(path) => {
                let conf = viuer::Config::default();
                viuer::print_from_file(&path, &conf).unwrap_or_else(|e| {
                    tracing::warn!(?path, ?e, "Failed to display image");
                    (0, 0)
                });
                w.flush()?;
            }
            Source::Name(_) => {
                todo!("Handle named images");
            }
        }
        Ok(())
    }
}
