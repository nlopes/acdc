use std::fmt::Write as _;

use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{AttributeValue, Audio};

use crate::Error;

pub(crate) fn visit_audio<V: WritableVisitor<Error = Error>>(
    audio: &Audio,
    visitor: &mut V,
) -> Result<(), Error> {
    let mut w = visitor.writer_mut();
    write!(w, "<div")?;
    if let Some(id) = &audio.metadata.id {
        write!(w, " id=\"{}\"", id.id)?;
    }
    writeln!(w, " class=\"audioblock\">")?;

    if !audio.title.is_empty() {
        write!(w, "<div class=\"title\">")?;
        let _ = w;
        visitor.visit_inline_nodes(&audio.title)?;
        w = visitor.writer_mut();
        writeln!(w, "</div>")?;
    }

    writeln!(w, "<div class=\"content\">")?;

    // Build the src attribute with optional start and end time
    let mut src = audio.source.to_string();
    let start = audio.metadata.attributes.get("start");
    let end = audio.metadata.attributes.get("end");

    match (start, end) {
        (Some(AttributeValue::String(s)), Some(AttributeValue::String(e))) => {
            write!(src, "#t={s},{e}")?;
        }
        (Some(AttributeValue::String(s)), None) => {
            write!(src, "#t={s}")?;
        }
        _ => {}
    }

    write!(w, "<audio src=\"{src}\"")?;

    // Add autoplay option if present
    if audio.metadata.options.contains(&"autoplay".to_string()) {
        write!(w, " autoplay")?;
    }

    // Add loop option if present
    if audio.metadata.options.contains(&"loop".to_string()) {
        write!(w, " loop")?;
    }

    // Add nocontrols option check - if present, don't add controls
    if !audio.metadata.options.contains(&"nocontrols".to_string()) {
        write!(w, " controls")?;
    }

    writeln!(w, ">")?;
    writeln!(w, "Your browser does not support the audio tag.")?;
    writeln!(w, "</audio>")?;
    writeln!(w, "</div>")?;
    writeln!(w, "</div>")?;

    Ok(())
}
