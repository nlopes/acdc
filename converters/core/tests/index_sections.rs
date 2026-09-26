//! Index discovery boundaries and compatibility with existing style queries.

use acdc_converters_core::section::{has_index_section, last_section_has_style};
use acdc_parser::{Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn index_detection_is_independent_of_last_section_style() -> Result<(), Error> {
    let parsed = parse(
        "= Manual\n\n[index]\n== Index\n\n[bibliography]\n== References\n",
        &Options::default(),
    )?;
    let blocks = &parsed.document().blocks;
    assert!(has_index_section(blocks));
    assert!(!last_section_has_style(blocks, "index"));
    assert!(last_section_has_style(blocks, "bibliography"));
    Ok(())
}

#[test]
fn index_detection_reaches_sections_inside_book_parts() -> Result<(), Error> {
    let parsed = parse(
        "= Manual\n:doctype: book\n\n= Part\n\n== Chapter\n\nText.\n\n[index]\n== Index\n\n= Later Part\n\n== Later Chapter\n",
        &Options::default(),
    )?;
    assert!(has_index_section(&parsed.document().blocks));
    Ok(())
}

#[test]
fn index_detection_excludes_styled_paragraphs_and_nested_documents() -> Result<(), Error> {
    for input in [
        "= Manual\n\n[index]\nParagraph.\n",
        "= Manual\n\n[cols=a]\n|===\n| [index]\n== Cell Index\n|===\n",
    ] {
        let parsed = parse(input, &Options::default())?;
        assert!(!has_index_section(&parsed.document().blocks), "{input}");
    }
    Ok(())
}
