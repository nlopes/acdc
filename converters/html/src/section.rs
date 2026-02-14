use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::{DiscreteHeader, Section, UNNUMBERED_SECTION_STYLES};

use crate::{Error, HtmlVariant, Processor};

/// Visit a section using the visitor pattern
///
/// Renders the section header, walks nested blocks, then renders footer.
/// For sections with `[index]` style, renders a populated index catalog
/// only if it's the last section in the document.
pub(crate) fn visit_section<V: WritableVisitor<Error = Error>>(
    section: &Section,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    // Check if this is an index section
    let is_index_section = section
        .metadata
        .style
        .as_ref()
        .is_some_and(|s| s == "index");

    // Index sections are only rendered if they're the last section
    if is_index_section && !processor.has_valid_index_section() {
        // Skip rendering entirely - not even the title
        return Ok(());
    }

    render_section_header(section, visitor, processor)?;

    if is_index_section {
        // Render the collected index catalog
        crate::index::render(section, visitor, processor)?;
    } else {
        // Normal section: render nested blocks
        for nested_block in &section.content {
            visitor.visit_block(nested_block)?;
        }
    }

    render_section_footer(section, visitor, processor)?;
    Ok(())
}

/// Render the section header (opening tags and title)
///
/// Call this before walking the section's nested blocks.
fn render_section_header<V: WritableVisitor<Error = Error>>(
    section: &Section,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    let id = Section::generate_id(&section.metadata, &section.title);

    // Special section styles (bibliography, glossary, etc.) should not be numbered
    let skip_numbering = section
        .metadata
        .style
        .as_ref()
        .is_some_and(|s| UNNUMBERED_SECTION_STYLES.contains(&s.as_str()));

    // Appendix sections at level 0 are demoted to level 1
    let is_appendix = section
        .metadata
        .style
        .as_ref()
        .is_some_and(|s| s == "appendix");
    let effective_level = if is_appendix && section.level == 0 {
        1
    } else {
        section.level
    };
    let heading_level = effective_level + 1; // Level 1 = h2

    let mut w = visitor.writer_mut();

    if section.level == 0 && !is_appendix {
        // Parts (level 0) in book doctype: standalone h1 with class="sect0", no wrapper div
        write!(w, "<h{heading_level} id=\"{id}\" class=\"sect0\">")?;

        // Prepend part number if :partnums: is enabled
        if !skip_numbering && let Some(part_label) = processor.part_number_tracker().enter_part() {
            write!(w, "{part_label}")?;
        }
    } else {
        if processor.variant() == HtmlVariant::Semantic {
            writeln!(w, "<section class=\"doc-section level-{effective_level}\">")?;
        } else {
            writeln!(w, "<div class=\"sect{effective_level}\">")?;
        }
        write!(w, "<h{heading_level} id=\"{id}\">")?;

        // Prepend appendix label for appendix sections (any level)
        if is_appendix {
            if let Some(appendix_label) = processor.appendix_tracker().enter_appendix() {
                write!(w, "{appendix_label}")?;
            }
        } else if !skip_numbering
            && let Some(number) = processor
                .section_number_tracker()
                .enter_section(effective_level)
        {
            // Prepend section number if sectnums is enabled and this isn't a special section
            write!(w, "{number}")?;
        }
    }

    let _ = w;
    visitor.visit_inline_nodes(&section.title)?;
    w = visitor.writer_mut();
    writeln!(w, "</h{heading_level}>")?;

    // sect1 (or appendix demoted to sect1) gets a sectionbody wrapper in standard mode
    if processor.variant() == HtmlVariant::Standard && effective_level == 1 {
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
    processor: &Processor,
) -> Result<(), Error> {
    // Appendix sections at level 0 are demoted to level 1
    let is_appendix = section
        .metadata
        .style
        .as_ref()
        .is_some_and(|s| s == "appendix");
    let effective_level = if is_appendix && section.level == 0 {
        1
    } else {
        section.level
    };

    // Parts (level 0, non-appendix) have no wrapper div to close
    if section.level == 0 && !is_appendix {
        return Ok(());
    }

    let w = visitor.writer_mut();

    if processor.variant() == HtmlVariant::Semantic {
        writeln!(w, "</section>")?;
    } else {
        // sect1 (or appendix demoted to sect1) has a sectionbody wrapper to close
        if effective_level == 1 {
            writeln!(w, "</div>")?; // Close sectionbody
        }
        writeln!(w, "</div>")?; // Close sectN
    }
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
