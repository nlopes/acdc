//! `ignore_filename_in_crossref`: resolving `<<file.adoc#anchor>>` as
//! `<<anchor>>`.

use acdc_parser::{Block, InlineMacro, InlineNode, Options, parse, parse_inline};

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
    for source in [
        "<<other.adoc#anchor>>",
        "<<other#anchor>>",
        "<<C:/chapters/other.adoc#anchor>>",
    ] {
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
        assert_eq!(
            xref(&format!("xref:{target}[]"), true)?.0,
            target,
            "macro target {target} should be untouched"
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

#[test]
fn url_targets_are_preserved_in_both_syntaxes() -> Result<(), Error> {
    for target in [
        "https://example.org/other.adoc#anchor",
        "http://example.org/other.adoc#anchor",
        "ftp://example.org/other.adoc#anchor",
        "irc://example.org/channel#anchor",
        "mailto:author@example.org#anchor",
        "//example.org/other.adoc#anchor",
    ] {
        for ignore_filename in [false, true] {
            for source in [
                format!("<<{target},Remote>>"),
                format!("xref:{target}[Remote]"),
            ] {
                assert_eq!(
                    xref(&source, ignore_filename)?,
                    (target.to_owned(), "Remote".to_owned()),
                    "{source}, ignore_filename={ignore_filename}"
                );
            }
        }
    }
    for target in [
        "file:///other.adoc#anchor",
        "HTTPS://example.org/other.adoc#anchor",
    ] {
        assert_eq!(xref(&format!("<<{target},Remote>>"), true)?.0, target);
    }
    Ok(())
}

#[test]
fn protected_urls_are_preserved_before_filename_removal() -> Result<(), Error> {
    let target = "https://example.org/other.adoc#anchor";
    let source = "<<https://example.org/pass:[other.adoc]#anchor,Remote>>\n\nxref:https://example.org/pass:[other.adoc]#anchor[Remote]\n";
    let options = Options::builder()
        .with_ignore_filename_in_crossref()
        .build()?;
    let parsed = parse(source, &options)?;
    let targets = parsed
        .document()
        .blocks
        .iter()
        .filter_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            let InlineNode::Macro(InlineMacro::CrossReference(xref)) = paragraph.content.first()?
            else {
                return None;
            };
            Some(xref.target)
        })
        .collect::<Vec<_>>();
    assert_eq!(targets, [target, target]);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}
