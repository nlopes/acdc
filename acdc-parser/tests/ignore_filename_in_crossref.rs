//! `ignore_filename_in_crossref`: resolving `<<file.adoc#anchor>>` as
//! `<<anchor>>`.

use acdc_parser::{InlineMacro, InlineNode, Options, parse_inline};

type Error = Box<dyn std::error::Error>;

/// The target and plain text of the single cross-reference in `source`.
fn xref(source: &str, ignore_filename: bool) -> Result<(String, String), Error> {
    let mut builder = Options::builder();
    if ignore_filename {
        builder = builder.with_ignore_filename_in_crossref();
    }
    let parsed = parse_inline(source, &builder.build()?)?;
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
fn the_filename_is_kept_by_default() -> Result<(), Error> {
    assert_eq!(xref("<<other.adoc#anchor>>", false)?.0, "other.adoc#anchor");
    assert_eq!(
        xref("xref:other.adoc#anchor[text]", false)?.0,
        "other.adoc#anchor"
    );
    Ok(())
}

#[test]
fn the_filename_and_its_hash_are_dropped() -> Result<(), Error> {
    for source in ["<<other.adoc#anchor>>", "<<other#anchor>>"] {
        assert_eq!(xref(source, true)?, ("anchor".to_string(), String::new()));
    }
    assert_eq!(
        xref("xref:other.adoc#anchor[text]", true)?,
        ("anchor".to_string(), "text".to_string())
    );
    Ok(())
}

#[test]
fn custom_text_is_kept() -> Result<(), Error> {
    assert_eq!(
        xref("<<other.adoc#anchor,the anchor>>", true)?,
        ("anchor".to_string(), "the anchor".to_string())
    );
    Ok(())
}

#[test]
fn a_target_without_an_anchor_is_left_as_written() -> Result<(), Error> {
    for target in ["other.adoc", "other.adoc#", "anchor"] {
        assert_eq!(
            xref(&format!("<<{target}>>"), true)?.0,
            target,
            "target {target} should be untouched"
        );
    }
    Ok(())
}

#[test]
fn whitespace_after_the_comma_is_ignored() -> Result<(), Error> {
    for ignore_filename in [false, true] {
        assert_eq!(xref("<<anchor,  text>>", ignore_filename)?.1, "text");
    }
    assert_eq!(xref("<<other.adoc#anchor,  text>>", true)?.1, "text");
    Ok(())
}
