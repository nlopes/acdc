use std::{fmt::Write as _, io::Write};

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{AttributeValue, Audio};

use crate::{Error, HtmlVariant, HtmlVisitor};

impl<W: Write> HtmlVisitor<'_, W> {
    pub(crate) fn render_audio(&mut self, audio: &Audio) -> Result<(), Error> {
        if self.processor.variant() == HtmlVariant::Semantic {
            return visit_audio_semantic(audio, self);
        }

        let mut w = self.writer_mut();
        write!(w, "<div")?;
        if let Some(id) = &audio.metadata.id {
            write!(w, " id=\"{}\"", id.id)?;
        }
        writeln!(w, " class=\"audioblock\">")?;

        if !audio.title.is_empty() {
            write!(w, "<div class=\"title\">")?;
            let _ = w;
            self.visit_inline_nodes(&audio.title)?;
            w = self.writer_mut();
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
        if audio.metadata.options.contains(&"autoplay") {
            write!(w, " autoplay")?;
        }

        // Add loop option if present
        if audio.metadata.options.contains(&"loop") {
            write!(w, " loop")?;
        }

        // Add nocontrols option check - if present, don't add controls
        if !audio.metadata.options.contains(&"nocontrols") {
            write!(w, " controls")?;
        }

        writeln!(w, ">")?;
        writeln!(w, "Your browser does not support the audio tag.")?;
        writeln!(w, "</audio>")?;
        writeln!(w, "</div>")?;
        writeln!(w, "</div>")?;

        Ok(())
    }
}

fn render_audio_element(audio: &Audio, w: &mut dyn std::io::Write) -> Result<(), Error> {
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

    if audio.metadata.options.contains(&"autoplay") {
        write!(w, " autoplay")?;
    }

    if audio.metadata.options.contains(&"loop") {
        write!(w, " loop")?;
    }

    if !audio.metadata.options.contains(&"nocontrols") {
        write!(w, " controls")?;
    }

    writeln!(w, ">")?;
    writeln!(w, "Your browser does not support the audio tag.")?;
    writeln!(w, "</audio>")?;

    Ok(())
}

fn visit_audio_semantic<W: Write>(
    audio: &Audio,
    visitor: &mut HtmlVisitor<'_, W>,
) -> Result<(), Error> {
    let has_title = !audio.title.is_empty();
    let mut w = visitor.writer_mut();

    let tag = if has_title { "figure" } else { "div" };
    write!(w, "<{tag} class=\"audio-block\"")?;
    if let Some(id) = &audio.metadata.id {
        write!(w, " id=\"{}\"", id.id)?;
    }
    writeln!(w, ">")?;

    render_audio_element(audio, w)?;

    if has_title {
        w = visitor.writer_mut();
        write!(w, "<figcaption>")?;
        let _ = w;
        visitor.visit_inline_nodes(&audio.title)?;
        w = visitor.writer_mut();
        writeln!(w, "</figcaption>")?;
    }

    writeln!(w, "</{tag}>")?;
    Ok(())
}
