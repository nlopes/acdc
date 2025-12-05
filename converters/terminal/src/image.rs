use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{Image, Source};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::Error;

pub(crate) fn visit_image<V: WritableVisitor<Error = Error>>(
    img: &Image,
    visitor: &mut V,
) -> Result<(), Error> {
    let w = visitor.writer_mut();
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
