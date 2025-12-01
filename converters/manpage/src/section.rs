//! Section rendering for manpages.
//!
//! Handles `.SH` (section) and `.SS` (subsection) macros.

use std::io::Write;

use acdc_converters_common::visitor::{Visitor, WritableVisitor};
use acdc_parser::Section;

use crate::{
    Error, ManpageVisitor,
    document::extract_plain_text,
    escape::{escape_quoted, uppercase_title},
};

/// Visit a section and its content.
pub fn visit_section<W: Write>(
    section: &Section,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    let title_text = extract_plain_text(&section.title);

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

    // Visit section content
    for block in &section.content {
        visitor.visit_block(block)?;
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
