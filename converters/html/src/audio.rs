use std::io::Write;

use acdc_converters_core::{
    TraversalContext,
    media::{resolve_target, with_time_range},
};
use acdc_parser::Audio;

use crate::{Error, HtmlVariant, HtmlVisitor, build_class, inlines::escape_attribute, write_id};

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    pub(crate) fn render_audio(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        audio: &Audio,
    ) -> Result<(), Error> {
        if self.processor.variant() == HtmlVariant::Semantic {
            return visit_audio_semantic(traversal, audio, self);
        }

        write!(self.writer, "<div")?;
        write_id(&mut self.writer, &audio.metadata)?;
        let class = escape_attribute(&build_class("audioblock", &audio.metadata.roles));
        writeln!(self.writer, " class=\"{class}\">")?;

        if audio.title.is_present() {
            write!(self.writer, "<div class=\"title\">")?;
            self.visit_title(traversal, &audio.title)?;
            writeln!(self.writer, "</div>")?;
        }

        writeln!(self.writer, "<div class=\"content\">")?;

        let src = audio_target(audio, traversal);
        write!(self.writer, "<audio src=\"{}\"", escape_attribute(&src))?;

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

fn audio_target(audio: &Audio<'_>, attributes: &TraversalContext<'_>) -> String {
    with_time_range(
        &resolve_target(&audio.source.to_string(), attributes),
        &audio.metadata.attributes,
    )
}

fn render_audio_element(
    audio: &Audio,
    attributes: &TraversalContext<'_>,
    w: &mut dyn std::io::Write,
) -> Result<(), Error> {
    let src = audio_target(audio, attributes);

    write!(w, "<audio src=\"{}\"", escape_attribute(&src))?;

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

fn visit_audio_semantic<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    audio: &Audio,
    visitor: &mut HtmlVisitor<'a, '_, W>,
) -> Result<(), Error> {
    let has_title = audio.title.is_present();

    let tag = if has_title { "figure" } else { "div" };
    let class = escape_attribute(&build_class("audio-block", &audio.metadata.roles));
    write!(visitor.writer, "<{tag}")?;
    write_id(&mut visitor.writer, &audio.metadata)?;
    write!(visitor.writer, " class=\"{class}\"")?;
    writeln!(visitor.writer, ">")?;

    render_audio_element(audio, traversal, &mut visitor.writer)?;

    if has_title {
        write!(visitor.writer, "<figcaption>")?;
        visitor.visit_title(traversal, &audio.title)?;
        writeln!(visitor.writer, "</figcaption>")?;
    }

    writeln!(visitor.writer, "</{tag}>")?;
    Ok(())
}
