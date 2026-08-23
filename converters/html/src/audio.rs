use std::{fmt::Write as _, io::Write};

use acdc_converters_core::{media::resolve_target, visitor::Visitor};
use acdc_parser::{AttributeValue, Audio, DocumentAttributes};

use crate::{Error, HtmlVariant, HtmlVisitor, inlines::escape_href};

impl<W: Write> HtmlVisitor<'_, '_, W> {
    pub(crate) fn render_audio(&mut self, audio: &Audio) -> Result<(), Error> {
        if self.processor.variant() == HtmlVariant::Semantic {
            return visit_audio_semantic(audio, self);
        }

        write!(self.writer, "<div")?;
        if let Some(id) = &audio.metadata.id {
            write!(self.writer, " id=\"{}\"", id.id)?;
        }
        writeln!(self.writer, " class=\"audioblock\">")?;

        if !audio.title.is_empty() {
            write!(self.writer, "<div class=\"title\">")?;
            self.visit_inline_nodes(&audio.title)?;
            writeln!(self.writer, "</div>")?;
        }

        writeln!(self.writer, "<div class=\"content\">")?;

        let src = audio_target(audio, self.processor.document_attributes())?;
        write!(self.writer, "<audio src=\"{}\"", escape_href(&src))?;

        // Add autoplay option if present
        if audio.metadata.options.contains(&"autoplay") {
            write!(self.writer, " autoplay")?;
        }

        // Add loop option if present
        if audio.metadata.options.contains(&"loop") {
            write!(self.writer, " loop")?;
        }

        // Add nocontrols option check - if present, don't add controls
        if !audio.metadata.options.contains(&"nocontrols") {
            write!(self.writer, " controls")?;
        }

        writeln!(self.writer, ">")?;
        writeln!(self.writer, "Your browser does not support the audio tag.")?;
        writeln!(self.writer, "</audio>")?;
        writeln!(self.writer, "</div>")?;
        writeln!(self.writer, "</div>")?;

        Ok(())
    }
}

fn audio_target(
    audio: &Audio<'_>,
    attributes: &DocumentAttributes<'_>,
) -> Result<String, std::fmt::Error> {
    let mut src = resolve_target(&audio.source.to_string(), attributes);
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

    Ok(src)
}

fn render_audio_element(
    audio: &Audio,
    attributes: &DocumentAttributes<'_>,
    w: &mut dyn std::io::Write,
) -> Result<(), Error> {
    let src = audio_target(audio, attributes)?;

    write!(w, "<audio src=\"{}\"", escape_href(&src))?;

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
    visitor: &mut HtmlVisitor<'_, '_, W>,
) -> Result<(), Error> {
    let has_title = !audio.title.is_empty();

    let tag = if has_title { "figure" } else { "div" };
    write!(visitor.writer, "<{tag} class=\"audio-block\"")?;
    if let Some(id) = &audio.metadata.id {
        write!(visitor.writer, " id=\"{}\"", id.id)?;
    }
    writeln!(visitor.writer, ">")?;

    render_audio_element(
        audio,
        visitor.processor.document_attributes(),
        &mut visitor.writer,
    )?;

    if has_title {
        write!(visitor.writer, "<figcaption>")?;
        visitor.visit_inline_nodes(&audio.title)?;
        writeln!(visitor.writer, "</figcaption>")?;
    }

    writeln!(visitor.writer, "</{tag}>")?;
    Ok(())
}
