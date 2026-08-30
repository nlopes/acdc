use std::io::Write;

use acdc_converters_core::{
    section::{
        appendix_number_prefix, book_chapter_signifier, effective_section_level,
        part_number_prefix, section_number_prefix,
    },
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{AttributeValue, DiscreteHeader, DocumentAttributes, Section, SectionKind};

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
        let effective_level = effective_section_level(section.level, section.kind);

        let is_appendix = section.kind == SectionKind::Appendix;
        let is_part = section.level == 0 && section.kind == SectionKind::Normal;
        let heading_level = effective_level + 1; // Level 1 = h2

        if is_part {
            // Parts (level 0) in book doctype: standalone h1 with class="sect0", no wrapper div
            let class = crate::build_class("sect0", &section.metadata.roles);
            write!(
                self.writer,
                "<h{heading_level} id=\"{id}\" class=\"{class}\">"
            )?;

            if let Some(number) = section.number() {
                let prefix = part_number_prefix(
                    number,
                    string_attribute(processor.document_attributes(), "part-signifier"),
                );
                write!(self.writer, "{prefix}")?;
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

            if let Some(number) = section.number() {
                let prefix = if is_appendix {
                    appendix_number_prefix(
                        number,
                        string_attribute(processor.document_attributes(), "appendix-caption"),
                    )
                } else {
                    let signifier = (section.level == 1 && section.kind == SectionKind::Normal)
                        .then(|| book_chapter_signifier(processor.document_attributes(), None))
                        .flatten();
                    section_number_prefix(number, signifier)
                };
                write!(self.writer, "{prefix}")?;
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
        let effective_level = effective_section_level(section.level, section.kind);
        let is_part = section.level == 0 && section.kind == SectionKind::Normal;

        // Normal level-0 parts have no wrapper element to close.
        if is_part {
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

fn string_attribute<'a>(attributes: &'a DocumentAttributes<'_>, name: &str) -> Option<&'a str> {
    match attributes.get(name) {
        Some(AttributeValue::String(value)) => Some(value.as_ref()),
        Some(_) | None => None,
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
