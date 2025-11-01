use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{Audio, Source};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::Error;

pub(crate) fn visit_audio<V: WritableVisitor<Error = Error>>(
    audio: &Audio,
    visitor: &mut V,
) -> Result<(), Error> {
    let w = visitor.writer_mut();
    match &audio.source {
        Source::Url(url) => {
            w.queue(PrintStyledContent(url.as_str().to_string().italic()))?;
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
