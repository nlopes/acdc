use std::path::PathBuf;

use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::Image;

use crate::Error;

pub(crate) fn visit_image<V: WritableVisitor<Error = Error>>(
    img: &Image,
    visitor: &mut V,
) -> Result<(), Error> {
    let mut w = visitor.writer_mut();
    write!(w, "<div class=\"imageblock\">")?;
    write!(w, "<div class=\"content\">")?;
    let link = img.metadata.attributes.get("link");
    if let Some(link) = link {
        write!(w, "<a class=\"image\" href=\"{link}\">")?;
    }
    write!(w, "<img src=\"{}\"", img.source)?;
    if let Some(alt) = img.metadata.attributes.get("alt") {
        write!(w, " alt=\"{alt}\"")?;
    } else {
        // If no alt text is provided, take the filename without the extension, and
        // then use spaces instead of dashes and underscores for the alt text
        let mut filepath = PathBuf::from(img.source.get_filename().unwrap_or(""));
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
    if !img.title.is_empty() {
        write!(w, "<div class=\"title\">")?;
        let _ = w;
        visitor.visit_inline_nodes(&img.title)?;
        w = visitor.writer_mut();
        write!(w, "</div>")?;
    }
    write!(w, "</div>")?; // close imageblock
    Ok(())
}
