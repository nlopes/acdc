use std::{fmt::Write as _, io::Write};

use acdc_parser::Audio;

use crate::{Processor, Render, RenderOptions};

impl Render for Audio {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        write!(w, "<div")?;
        if let Some(id) = &self.metadata.id {
            write!(w, " id=\"{}\"", id.id)?;
        }
        writeln!(w, " class=\"audioblock\">")?;

        if !self.title.is_empty() {
            write!(w, "<div class=\"title\">")?;
            crate::inlines::render_inlines(&self.title, w, processor, options)?;
            writeln!(w, "</div>")?;
        }

        writeln!(w, "<div class=\"content\">")?;

        // Build the src attribute with optional start and end time
        let mut src = self.source.to_string();
        let start = self.metadata.attributes.get("start");
        let end = self.metadata.attributes.get("end");

        match (start, end) {
            (
                Some(acdc_parser::AttributeValue::String(s)),
                Some(acdc_parser::AttributeValue::String(e)),
            ) => {
                write!(src, "#t={s},{e}")?;
            }
            (Some(acdc_parser::AttributeValue::String(s)), None) => {
                write!(src, "#t={s}")?;
            }
            _ => {}
        }

        write!(w, "<audio src=\"{src}\"")?;

        // Add autoplay option if present
        if self.metadata.options.contains(&"autoplay".to_string()) {
            write!(w, " autoplay")?;
        }

        // Add loop option if present
        if self.metadata.options.contains(&"loop".to_string()) {
            write!(w, " loop")?;
        }

        // Add nocontrols option check - if present, don't add controls
        if !self.metadata.options.contains(&"nocontrols".to_string()) {
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
