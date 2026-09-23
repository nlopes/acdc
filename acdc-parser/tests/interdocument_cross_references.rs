//! `ignore_filename_in_crossrefs`: resolving `<<file.adoc#anchor>>` by its
//! anchor alone, the way Antora resolves a reference into another page.

use acdc_parser::{Block, InlineMacro, InlineNode, Options, ParseResult, parse, parse_inline};

type Error = Box<dyn std::error::Error>;

fn options(ignore_filename: bool) -> Result<Options<'static>, Error> {
    Ok(Options::builder()
        .with_ignore_filename_in_crossrefs(ignore_filename)
        .build()?)
}

/// The target and plain display text of the single cross-reference in `source`.
fn xref_of(source: &str, ignore_filename: bool) -> Result<(String, String), Error> {
    let parsed = parse_inline(source, &options(ignore_filename)?)?;
    let [InlineNode::Macro(InlineMacro::CrossReference(xref))] = parsed.inlines() else {
        return Err(format!("expected one cross-reference, got {:?}", parsed.inlines()).into());
    };
    let text = xref
        .text
        .iter()
        .map(|node| {
            if let InlineNode::PlainText(text) = node {
                text.content
            } else {
                "?"
            }
        })
        .collect::<String>();
    Ok((xref.target.to_string(), text))
}

#[test]
fn the_file_part_is_kept_by_default() -> Result<(), Error> {
    assert_eq!(
        xref_of("<<other.adoc#anchor>>", false)?,
        ("other.adoc#anchor".to_string(), String::new())
    );
    assert_eq!(
        xref_of("xref:other.adoc#anchor[text]", false)?,
        ("other.adoc#anchor".to_string(), "text".to_string())
    );
    Ok(())
}

#[test]
fn the_file_part_is_dropped_when_asked() -> Result<(), Error> {
    assert_eq!(
        xref_of("<<other.adoc#anchor>>", true)?,
        ("anchor".to_string(), String::new())
    );
    assert_eq!(
        xref_of("xref:other.adoc#anchor[text]", true)?,
        ("anchor".to_string(), "text".to_string())
    );
    Ok(())
}

#[test]
fn custom_text_survives_dropping_the_file_part() -> Result<(), Error> {
    assert_eq!(
        xref_of("<<other.adoc#anchor,a figure>>", true)?,
        ("anchor".to_string(), "a figure".to_string())
    );
    Ok(())
}

#[test]
fn a_target_without_an_anchor_is_left_as_written() -> Result<(), Error> {
    for target in ["other.adoc", "other.adoc#", "anchor"] {
        assert_eq!(
            xref_of(&format!("<<{target}>>"), true)?.0,
            target.to_string(),
            "target {target} should be untouched"
        );
    }
    Ok(())
}

/// A document whose second section is referenced through the file that
/// defines it, as an assembled set of included files does.
fn assembled(ignore_filename: bool) -> Result<ParseResult, Error> {
    let source = "= Doc\n\n\
         == First\n\n\
         See <<other.adoc#target>>.\n\n\
         [[target]]\n\
         == Target\n\n\
         body\n";
    Ok(parse(source, &options(ignore_filename)?)?)
}

#[test]
fn a_dropped_file_part_resolves_against_this_documents_catalog() -> Result<(), Error> {
    let parsed = assembled(true)?;
    let document = parsed.document();
    assert!(document.references.contains_key("target"));

    let [Block::Section(first), ..] = document.blocks.as_slice() else {
        return Err(format!("expected sections, got {:?}", document.blocks).into());
    };
    let [Block::Paragraph(paragraph)] = first.content.as_slice() else {
        return Err(format!("expected one paragraph, got {:?}", first.content).into());
    };
    let targets = paragraph
        .content
        .iter()
        .filter_map(|node| {
            if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = node {
                Some(xref.target)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(targets, ["target"]);
    Ok(())
}
