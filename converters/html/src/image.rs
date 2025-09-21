use std::{io::Write, path::PathBuf};

use acdc_parser::Image;

use crate::{Processor, Render, RenderOptions};

impl Render for Image {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        write!(w, "<div class=\"imageblock\">")?;
        write!(w, "<div class=\"content\">")?;
        let link = self.metadata.attributes.get("link");
        if let Some(link) = link {
            write!(w, "<a class=\"image\" href=\"{link}\">")?;
        }
        write!(w, "<img src=\"{}\"", self.source)?;
        if let Some(alt) = self.metadata.attributes.get("alt") {
            write!(w, " alt=\"{alt}\"")?;
        } else {
            // If no alt text is provided, take the filename without the extension, and
            // then use spaces instead of dashes and underscores for the alt text
            let mut filepath = PathBuf::from(self.source.get_filename().unwrap_or(""));
            filepath.set_extension("");
            write!(
                w,
                " alt=\"{}\"",
                filepath.to_str().unwrap_or("").replace(['-', '_'], " ")
            )?;
        }
        write!(w, " />")?;
        if link.is_some() {
            write!(w, "</a>")?;
        }
        write!(w, "</div>")?; // close content
        if !self.title.is_empty() {
            write!(w, "<div class=\"title\">")?;
            self.title
                .iter()
                .try_for_each(|node| node.render(w, processor, options))?;
            write!(w, "</div>")?;
        }
        write!(w, "</div>")?; // close imageblock
        Ok(())
    }
}
