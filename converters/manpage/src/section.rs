//! Section rendering for manpages.
//!
//! Handles `.SH` (section) and `.SS` (subsection) macros.

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{Section, SectionKind};

use crate::{
    Error, ManpageVisitor,
    document::extract_heading_text,
    escape::{escape_quoted, uppercase_title},
};

impl<W: Write> ManpageVisitor<'_, '_, W> {
    /// Visit a section and its content.
    pub(crate) fn render_section(&mut self, section: &Section) -> Result<(), Error> {
        self.collect_index_terms_from_inlines(&section.title)?;
        let title_text = extract_heading_text(&section.title, &self.processor.references);

        // Track level-1 section titles for convention validation
        if section.level == 1 {
            self.record_section_title(&title_text);
        }

        let name_section_title = self
            .processor
            .document_attributes
            .get_string("manname-title")
            .unwrap_or_else(|| "Name".into());
        let is_name_section =
            section.level == 1 && title_text.eq_ignore_ascii_case(name_section_title.as_ref());

        // In embedded mode, skip the name section (matches asciidoctor --embedded).
        if self.processor.options.embedded() && is_name_section {
            return Ok(());
        }

        // Level 1 sections use .SH, level 2+ use .SS
        // Manpage convention: uppercase section titles for level 1
        let w = self.writer_mut();

        if section.level == 1 {
            // Main section - .SH with uppercase title
            writeln!(
                w,
                ".SH \"{}\"",
                escape_quoted(&uppercase_title(&title_text))
            )?;
        } else if section.level <= 2 {
            // Subsection - .SS (preserve original case, matching asciidoctor)
            writeln!(w, ".SS \"{}\"", escape_quoted(&title_text))?;
        } else {
            // Levels 3+ - no roff section macro exists; render as bold paragraph heading
            writeln!(w, ".sp")?;
            write!(w, "\\fB")?;
            self.visit_inline_nodes(&section.title)?;
            let w = self.writer_mut();
            writeln!(w, "\\fP")?;
        }

        if is_name_section {
            self.in_name_section = true;
        }

        if section.kind == SectionKind::Index && self.processor.has_valid_index_section {
            self.render_index_catalog()?;
        } else {
            for block in &section.content.clone() {
                self.visit_block(block)?;
            }
        }

        if is_name_section {
            self.in_name_section = false;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::escape::uppercase_title;

    #[test]
    fn test_uppercase_section() {
        assert_eq!(uppercase_title("description"), "DESCRIPTION");
        assert_eq!(uppercase_title("See Also"), "SEE ALSO");
        assert_eq!(uppercase_title("NAME"), "NAME");
    }
}
