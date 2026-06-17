use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{DiscreteHeader, Section, SectionKind};

use crate::{Error, HtmlVariant, HtmlVisitor};

impl<W: Write> HtmlVisitor<'_, '_, W> {
    /// Visit a section using the visitor pattern
    ///
    /// Renders the section header, walks nested blocks, then renders footer.
    /// A section with the `[index]` style gets acdc's generated index catalog
    /// (an extension over asciidoctor's html5 backend, which leaves `[index]`
    /// empty — see `crate::index`) only when it's the document's last section;
    /// any other `[index]` section renders like a normal section, so its
    /// heading is still emitted (matching asciidoctor) rather than dropped.
    pub(crate) fn render_section(&mut self, section: &Section) -> Result<(), Error> {
        let processor = self.processor.clone();

        let is_index_section = section.kind == SectionKind::Index;
        let render_catalog = is_index_section && processor.generate_index();

        self.render_section_header(section)?;

        if render_catalog {
            // Render the collected index catalog
            crate::index::render(section, self)?;
        } else {
            // Normal section (and non-last index sections): render nested blocks
            for nested_block in &section.content {
                self.visit_block(nested_block)?;
            }
        }

        self.render_section_footer(section)?;
        Ok(())
    }

    /// Render the section header (opening tags and title)
    ///
    /// Call this before walking the section's nested blocks.
    fn render_section_header(&mut self, section: &Section) -> Result<(), Error> {
        let processor = self.processor.clone();
        let id = Section::generate_id_string(&section.metadata, &section.title);

        // Special sections (and every subsection nested under one) are excluded
        // from `:sectnums:` numbering. The tracker is fed every section in
        // document order, so it must be consulted once here for each section.
        let skip_numbering = !processor
            .special_section_tracker()
            .enter(section.level, section.kind);

        // Appendix sections at level 0 are demoted to level 1
        let is_appendix = section.kind == SectionKind::Appendix;
        let effective_level = if is_appendix && section.level == 0 {
            1
        } else {
            section.level
        };
        let heading_level = effective_level + 1; // Level 1 = h2

        if section.level == 0 && !is_appendix {
            // Parts (level 0) in book doctype: standalone h1 with class="sect0", no wrapper div
            let class = crate::build_class("sect0", &section.metadata.roles);
            write!(
                self.writer,
                "<h{heading_level} id=\"{id}\" class=\"{class}\">"
            )?;

            // Prepend part number if :partnums: is enabled
            if !skip_numbering
                && let Some(part_label) = processor.part_number_tracker().enter_part()
            {
                write!(self.writer, "{part_label}")?;
            }
        } else {
            if processor.variant() == HtmlVariant::Semantic {
                let class = crate::build_class(
                    &format!("doc-section level-{effective_level}"),
                    &section.metadata.roles,
                );
                writeln!(self.writer, "<section class=\"{class}\">")?;
            } else {
                let class =
                    crate::build_class(&format!("sect{effective_level}"), &section.metadata.roles);
                writeln!(self.writer, "<div class=\"{class}\">")?;
            }
            write!(self.writer, "<h{heading_level} id=\"{id}\">")?;

            // Prepend appendix label for appendix sections (any level)
            if is_appendix {
                if let Some(appendix_label) = processor.appendix_tracker().enter_appendix() {
                    write!(self.writer, "{appendix_label}")?;
                }
            } else if !skip_numbering
                && let Some(number) = processor
                    .section_number_tracker()
                    .enter_section(effective_level)
            {
                // Prepend section number if sectnums is enabled and this isn't a special section
                write!(self.writer, "{number}")?;
            }
        }

        self.visit_inline_nodes(&section.title)?;
        writeln!(self.writer, "</h{heading_level}>")?;

        // sect1 (or appendix demoted to sect1) gets a sectionbody wrapper in standard mode
        if processor.variant() == HtmlVariant::Standard && effective_level == 1 {
            writeln!(self.writer, "<div class=\"sectionbody\">")?;
        }
        Ok(())
    }

    /// Render the section footer (closing tags)
    ///
    /// Call this after walking the section's nested blocks.
    fn render_section_footer(&mut self, section: &Section) -> Result<(), Error> {
        let processor = self.processor.clone();
        // Appendix sections at level 0 are demoted to level 1
        let is_appendix = section.kind == SectionKind::Appendix;
        let effective_level = if is_appendix && section.level == 0 {
            1
        } else {
            section.level
        };

        // Parts (level 0, non-appendix) have no wrapper div to close
        if section.level == 0 && !is_appendix {
            return Ok(());
        }

        if processor.variant() == HtmlVariant::Semantic {
            writeln!(self.writer, "</section>")?;
        } else {
            // sect1 (or appendix demoted to sect1) has a sectionbody wrapper to close
            if effective_level == 1 {
                writeln!(self.writer, "</div>")?; // Close sectionbody
            }
            writeln!(self.writer, "</div>")?; // Close sectN
        }
        Ok(())
    }
}

pub(crate) fn visit_discrete_header<V: WritableVisitor<Error = Error>>(
    header: &DiscreteHeader,
    visitor: &mut V,
) -> Result<(), Error> {
    let level = header.level + 1; // Level 1 = h2
    let id = Section::generate_id_string(&header.metadata, &header.title);

    // asciidoctor emits a `class` only when the discreteness came from the
    // `discrete`/`float` block style (`[discrete]` → `class="discrete"`, plus any
    // roles). The bare positional form (`[#id,discrete]`) renders no class at all.
    let class = match header.metadata.style {
        Some(style @ ("discrete" | "float")) => {
            Some(crate::build_class(style, &header.metadata.roles))
        }
        _ => None,
    };

    let mut w = visitor.writer_mut();
    if let Some(class) = class {
        write!(w, "<h{level} id=\"{id}\" class=\"{class}\">")?;
    } else {
        write!(w, "<h{level} id=\"{id}\">")?;
    }
    let _ = w;
    visitor.visit_inline_nodes(&header.title)?;
    w = visitor.writer_mut();
    writeln!(w, "</h{level}>")?;
    Ok(())
}
