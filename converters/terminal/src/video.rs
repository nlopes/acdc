use acdc_converters_common::{video::TryUrl, visitor::WritableVisitor};
use acdc_parser::Video;
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::Error;

pub(crate) fn visit_video<V: WritableVisitor<Error = Error>>(
    video: &Video,
    visitor: &mut V,
) -> Result<(), Error> {
    if video.sources.is_empty() {
        return Ok(());
    }

    let url = video.try_url(false)?;
    let w = visitor.writer_mut();
    w.queue(PrintStyledContent(url.italic()))?;
    Ok(())
}
