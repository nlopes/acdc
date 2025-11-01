use acdc_converters_common::visitor::{WritableVisitor, WritableVisitorExt};
use acdc_parser::Paragraph;

use crate::Error;

/// Visit a paragraph using the visitor pattern
///
/// This is called from the `HtmlVisitor` trait implementation.
pub(crate) fn visit_paragraph<V: WritableVisitor<Error = Error>>(
    para: &Paragraph,
    visitor: &mut V,
) -> Result<(), Error> {
    // Check if this paragraph should be rendered as a literal block
    if let Some(style) = &para.metadata.style
        && style == "literal"
    {
        let mut w = visitor.writer_mut();
        writeln!(w, "<div class=\"literalblock\">")?;
        let _ = w;
        visitor.render_title_with_wrapper(&para.title, "<div class=\"title\">", "</div>\n")?;
        w = visitor.writer_mut();
        writeln!(w, "<div class=\"content\">")?;
        write!(w, "<pre>")?;
        let _ = w;
        visitor.visit_inline_nodes(&para.content)?;
        w = visitor.writer_mut();
        writeln!(w, "</pre>")?;
        writeln!(w, "</div>")?;
        writeln!(w, "</div>")?;
        return Ok(());
    }

    // Regular paragraph rendering
    let mut w = visitor.writer_mut();
    writeln!(w, "<div class=\"paragraph\">")?;
    let _ = w;
    visitor.render_title_with_wrapper(&para.title, "<div class=\"title\">", "</div>\n")?;
    w = visitor.writer_mut();
    write!(w, "<p>")?;
    let _ = w;
    visitor.visit_inline_nodes(&para.content)?;
    w = visitor.writer_mut();
    writeln!(w, "</p>")?;
    writeln!(w, "</div>")?;
    Ok(())
}
