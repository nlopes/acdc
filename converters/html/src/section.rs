use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::{DiscreteHeader, Section};

use crate::Error;

/// Visit a section using the visitor pattern
///
/// Renders the section header, walks nested blocks, then renders footer.
pub(crate) fn visit_section<V: WritableVisitor<Error = Error>>(
    section: &Section,
    visitor: &mut V,
) -> Result<(), Error> {
    render_section_header(section, visitor)?;

    for nested_block in &section.content {
        visitor.visit_block(nested_block)?;
    }

    render_section_footer(section, visitor)?;
    Ok(())
}

/// Render the section header (opening tags and title)
///
/// Call this before walking the section's nested blocks.
fn render_section_header<V: WritableVisitor<Error = Error>>(
    section: &Section,
    visitor: &mut V,
) -> Result<(), Error> {
    let level = section.level + 1; // Level 1 = h2
    let id = Section::generate_id(&section.metadata, &section.title);

    let mut w = visitor.writer_mut();
    writeln!(w, "<div class=\"sect{}\">", section.level)?;
    write!(w, "<h{level} id=\"{id}\">")?;
    let _ = w;
    visitor.visit_inline_nodes(&section.title)?;
    w = visitor.writer_mut();
    writeln!(w, "</h{level}>")?;

    // Only sect1 gets a sectionbody wrapper in asciidoctor
    // sect2 and higher have content directly in the sectN div
    if section.level == 1 {
        writeln!(w, "<div class=\"sectionbody\">")?;
    }
    Ok(())
}

/// Render the section footer (closing tags)
///
/// Call this after walking the section's nested blocks.
fn render_section_footer<V: WritableVisitor<Error = Error>>(
    section: &Section,
    visitor: &mut V,
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    // Only sect1 has a sectionbody wrapper to close
    if section.level == 1 {
        writeln!(w, "</div>")?; // Close sectionbody
    }

    writeln!(w, "</div>")?; // Close sectN
    Ok(())
}

pub(crate) fn visit_discrete_header<V: WritableVisitor<Error = Error>>(
    header: &DiscreteHeader,
    visitor: &mut V,
) -> Result<(), Error> {
    let level = header.level + 1; // Level 1 = h2
    let id = Section::generate_id(&header.metadata, &header.title);

    let mut w = visitor.writer_mut();
    write!(w, "<h{level} id=\"{id}\" class=\"discrete\">")?;
    let _ = w;
    visitor.visit_inline_nodes(&header.title)?;
    w = visitor.writer_mut();
    writeln!(w, "</h{level}>")?;
    Ok(())
}
