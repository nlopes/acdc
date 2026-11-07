use std::io::Write;

use acdc_converters_core::{
    TraversalContext,
    media::{resolve_target, with_time_range_from_attributes},
};
use acdc_parser::ast::AudioRef;

use crate::{
    Error, HtmlVariant, HtmlVisitor, build_class, inlines::escape_attribute, write_element_id,
};

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    pub(crate) fn render_audio(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        audio: AudioRef<'_>,
    ) -> Result<(), Error> {
        if self.processor.variant() == HtmlVariant::Semantic {
            return visit_audio_semantic(traversal, audio, self);
        }

        write!(self.writer, "<div")?;
        write_element_id(&mut self.writer, audio.id())?;
        let class = build_class("audioblock", audio.roles().iter());
        writeln!(self.writer, " class=\"{class}\">")?;

        if audio.title().is_some() {
            write!(self.writer, "<div class=\"title\">")?;
            self.visit_title(traversal, audio.title())?;
            writeln!(self.writer, "</div>")?;
        }

        writeln!(self.writer, "<div class=\"content\">")?;

        render_audio_element(audio, traversal, &mut self.writer)?;
        writeln!(self.writer, "</div>")?;
        writeln!(self.writer, "</div>")?;

        Ok(())
    }
}

fn audio_target(audio: AudioRef<'_>, attributes: &TraversalContext<'_>) -> String {
    with_time_range_from_attributes(
        &resolve_target(&audio.source().to_string(), attributes),
        audio.attributes(),
    )
}

fn render_audio_element(
    audio: AudioRef<'_>,
    attributes: &TraversalContext<'_>,
    w: &mut dyn std::io::Write,
) -> Result<(), Error> {
    let src = audio_target(audio, attributes);

    write!(w, "<audio src=\"{}\"", escape_attribute(&src))?;

    if audio.options().contains("autoplay") {
        write!(w, " autoplay")?;
    }

    if audio.options().contains("loop") {
        write!(w, " loop")?;
    }

    if !audio.options().contains("nocontrols") {
        write!(w, " controls")?;
    }

    writeln!(w, ">")?;
    writeln!(w, "Your browser does not support the audio tag.")?;
    writeln!(w, "</audio>")?;

    Ok(())
}

fn visit_audio_semantic<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    audio: AudioRef<'_>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
) -> Result<(), Error> {
    let has_title = audio.title().is_some();

    let tag = if has_title { "figure" } else { "div" };
    let class = build_class("audio-block", audio.roles().iter());
    write!(visitor.writer, "<{tag}")?;
    write_element_id(&mut visitor.writer, audio.id())?;
    write!(visitor.writer, " class=\"{class}\"")?;
    writeln!(visitor.writer, ">")?;

    render_audio_element(audio, traversal, &mut visitor.writer)?;

    if has_title {
        write!(visitor.writer, "<figcaption>")?;
        visitor.visit_title(traversal, audio.title())?;
        writeln!(visitor.writer, "</figcaption>")?;
    }

    writeln!(visitor.writer, "</{tag}>")?;
    Ok(())
}
