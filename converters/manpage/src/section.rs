//! Section rendering for manpages.
//!
//! Handles `.SH` (section) and `.SS` (subsection) macros.

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::Section;

use crate::{
    Error, ManpageVisitor,
    document::extract_plain_text,
    escape::{escape_quoted, uppercase_title},
};

/// Visit a section and its content.
pub(crate) fn visit_section<W: Write>(
    section: &Section,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    let title_text = extract_plain_text(&section.title);

    // Track level-1 section titles for convention validation
    if section.level == 1 {
        visitor.record_section_title(&title_text);
    }

    // Check if this is the NAME section (which has special formatting rules)
    let is_name_section = title_text.eq_ignore_ascii_case("name");

    // In embedded mode, skip the NAME section entirely (matches asciidoctor --embedded)
    if visitor.processor.options.embedded() && is_name_section {
        return Ok(());
    }

    // Level 1 sections use .SH, level 2+ use .SS
    // Manpage convention: uppercase section titles for level 1
    let w = visitor.writer_mut();

    if section.level == 1 {
        // Main section - .SH with uppercase title
        writeln!(
            w,
            ".SH \"{}\"",
            escape_quoted(&uppercase_title(&title_text))
        )?;
    } else {
        // Subsection - .SS
        // Level 2 subsections are also typically uppercased in manpages
        let title = if section.level == 2 {
            uppercase_title(&title_text)
        } else {
            title_text
        };
        writeln!(w, ".SS \"{}\"", escape_quoted(&title))?;
    }

    // Set NAME section flag for content rendering
    if is_name_section {
        visitor.in_name_section = true;
    }

    // Visit section content
    for block in &section.content {
        visitor.visit_block(block)?;
    }

    // Reset NAME section flag
    if is_name_section {
        visitor.in_name_section = false;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uppercase_section() {
        assert_eq!(uppercase_title("description"), "DESCRIPTION");
        assert_eq!(uppercase_title("See Also"), "SEE ALSO");
        assert_eq!(uppercase_title("NAME"), "NAME");
    }
}
