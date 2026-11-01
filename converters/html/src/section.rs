use std::io::Write;

use acdc_converters_core::{
    TraversalContext,
    section::{
        appendix_number_prefix, book_chapter_signifier, effective_section_level,
        part_number_prefix, section_number_prefix,
    },
};
use acdc_parser::{
    SectionKind,
    ast::{DiscreteHeaderRef, SectionRef},
};

use crate::{Error, HtmlVariant, HtmlVisitor, build_class};

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    /// Render the section and its authored content, then any opted-in catalog.
    pub(crate) fn render_section(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        section: SectionRef<'a>,
    ) -> Result<(), Error> {
        let processor = self.processor.clone();
        let data = section;

        let is_index_section = data.kind() == SectionKind::Index;
        let render_catalog = is_index_section && processor.generate_index();

        self.render_section_header(traversal, data)?;

        // Keep authored content before the generated catalog.
        for nested_block in section.blocks() {
            traversal.visit_block(self, nested_block)?;
        }
        if render_catalog {
            crate::index::render(self)?;
        }

        self.render_section_footer(data)?;
        Ok(())
    }

    /// Render the section header (opening tags and title)
    ///
    /// Call this before walking the section's nested blocks.
    fn render_section_header(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        section: SectionRef<'_>,
    ) -> Result<(), Error> {
        let processor = self.processor.clone();
        let id = section.resolve_id();
        let effective_level = effective_section_level(section.level(), section.kind());

        let is_appendix = section.kind() == SectionKind::Appendix;
        let is_part = section.level() == 0 && section.kind() == SectionKind::Normal;
        let heading_level = effective_level + 1; // Level 1 = h2

        if is_part {
            // Parts (level 0) in book doctype: standalone h1 with class="sect0", no wrapper div
            let class = build_class("sect0", section.roles().iter());
            write!(
                self.writer,
                "<h{heading_level} id=\"{id}\" class=\"{class}\">"
            )?;

            if let Some(number) = section.number() {
                let prefix =
                    part_number_prefix(number, string_attribute(traversal, "part-signifier"));
                write!(self.writer, "{prefix}")?;
            }
        } else {
            if processor.variant() == HtmlVariant::Semantic {
                let class = build_class(
                    &format!("doc-section level-{effective_level}"),
                    section.roles().iter(),
                );
                writeln!(self.writer, "<section class=\"{class}\">")?;
            } else {
                let class = build_class(&format!("sect{effective_level}"), section.roles().iter());
                writeln!(self.writer, "<div class=\"{class}\">")?;
            }
            write!(self.writer, "<h{heading_level} id=\"{id}\">")?;

            if let Some(number) = section.number() {
                let prefix = if is_appendix {
                    appendix_number_prefix(number, string_attribute(traversal, "appendix-caption"))
                } else {
                    let signifier = (section.level() == 1 && section.kind() == SectionKind::Normal)
                        .then(|| book_chapter_signifier(traversal, None))
                        .flatten();
                    section_number_prefix(number, signifier)
                };
                write!(self.writer, "{prefix}")?;
            }
        }

        self.visit_title(traversal, section.title())?;
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
    fn render_section_footer(&mut self, section: SectionRef<'_>) -> Result<(), Error> {
        let processor = self.processor.clone();
        let effective_level = effective_section_level(section.level(), section.kind());
        let is_part = section.level() == 0 && section.kind() == SectionKind::Normal;

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

fn string_attribute<'a>(attributes: &'a TraversalContext<'_>, name: &str) -> Option<&'a str> {
    attributes.get(name).and_then(|value| value.text())
}

pub(crate) fn visit_discrete_header<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    header: DiscreteHeaderRef<'_>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
) -> Result<(), Error> {
    let id = header.resolve_id();
    let level = header.level() + 1; // Level 1 = h2

    // asciidoctor emits a `class` only when the discreteness came from the
    // `discrete`/`float` block style (`[discrete]` → `class="discrete"`, plus any
    // roles). The bare positional form (`[#id,discrete]`) renders no class at all.
    let class = match header.style() {
        Some(style @ ("discrete" | "float")) => Some(build_class(style, header.roles().iter())),
        _ => None,
    };

    let mut w = visitor.writer_mut();
    if let Some(class) = class {
        write!(w, "<h{level} id=\"{id}\" class=\"{class}\">")?;
    } else {
        write!(w, "<h{level} id=\"{id}\">")?;
    }
    let _ = w;
    visitor.visit_title(traversal, header.title())?;
    w = visitor.writer_mut();
    writeln!(w, "</h{level}>")?;
    Ok(())
}
