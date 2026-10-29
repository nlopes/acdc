//! `force_local_file_xrefs`: resolving `<<file.adoc#anchor>>` as
//! `<<anchor>>`.

use acdc_parser::{
    Block, CrossReference, InlineMacro, InlineNode, Options, SafeMode, WarningKind, parse,
    parse_inline,
};

type Error = Box<dyn std::error::Error>;

fn paragraph_xrefs<'d, 'a>(blocks: &'d [Block<'a>]) -> Vec<&'d CrossReference<'a>> {
    blocks
        .iter()
        .filter_map(|block| {
            if let Block::Paragraph(paragraph) = block {
                Some(&paragraph.content)
            } else {
                None
            }
        })
        .flatten()
        .filter_map(|inline| {
            if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline {
                Some(xref.as_ref())
            } else {
                None
            }
        })
        .collect()
}

/// The target and plain text of the single cross-reference in `source`.
fn xref(source: &str, force_local: bool) -> Result<(String, String), Error> {
    let mut builder = Options::builder();
    if force_local {
        builder = builder.with_force_local_file_xrefs();
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
fn reopening_options_preserves_file_xref_resolution() -> Result<(), Error> {
    for force_local in [false, true] {
        let mut builder = Options::builder();
        if force_local {
            builder = builder.with_force_local_file_xrefs();
        }
        let options = builder
            .build()?
            .into_static()
            .into_builder()
            .with_attribute("sectnums", true)
            .build()?;
        let parsed = parse_inline("xref:other.adoc#anchor[text]", &options)?;
        let [InlineNode::Macro(InlineMacro::CrossReference(xref))] = parsed.inlines() else {
            return Err("expected one cross-reference".into());
        };
        let expected = if force_local {
            "anchor"
        } else {
            "other.adoc#anchor"
        };
        assert_eq!(xref.target, expected);
        assert_eq!(xref.target_is_local, force_local);
    }
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
    for force_local in [false, true] {
        assert_eq!(xref("<<anchor,  text>>", force_local)?.1, "text");
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
        for force_local in [false, true] {
            for source in [
                format!("<<{target},Remote>>"),
                format!("xref:{target}[Remote]"),
            ] {
                assert_eq!(
                    xref(&source, force_local)?,
                    (target.to_owned(), "Remote".to_owned()),
                    "{source}, force_local={force_local}"
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
    let options = Options::builder().with_force_local_file_xrefs().build()?;
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

#[test]
fn forced_fragments_are_local_for_both_syntaxes() -> Result<(), Error> {
    let options = Options::builder().with_force_local_file_xrefs().build()?;
    for target in [
        "rule.1",
        "urn:rule",
        "chapter.adoc",
        "chapter#rule",
        "path/rule",
    ] {
        for source in [
            format!("<<other.adoc#{target},Explicit>>"),
            format!("xref:other.adoc#{target}[Explicit]"),
        ] {
            let parsed = parse_inline(&source, &options)?;
            let [InlineNode::Macro(InlineMacro::CrossReference(xref))] = parsed.inlines() else {
                return Err(format!("missing xref: {source}").into());
            };
            assert_eq!(xref.target, target);
            assert!(xref.target_is_local, "{source}");
        }
    }
    Ok(())
}

#[test]
fn forced_fragments_use_local_diagnostics_and_natural_labels() -> Result<(), Error> {
    let options = Options::builder().with_force_local_file_xrefs().build()?;
    let source = "xref:other.adoc#rule.1[Explicit] <<other.adoc#rule.1>> <<other.adoc#Rule>>\n\nxref:other.adoc#missing.id[] <<other.adoc#missing:id,Missing>>\n\nxref:other.adoc#++rule.1++[Protected]\n\n[[rule.1]]\n== Rule\n";
    let parsed = parse(source, &options)?;
    let xrefs = paragraph_xrefs(&parsed.document().blocks);
    assert_eq!(
        xrefs.iter().map(|xref| xref.target).collect::<Vec<_>>(),
        [
            "rule.1",
            "rule.1",
            "rule.1",
            "missing.id",
            "missing:id",
            "rule.1"
        ]
    );
    assert!(xrefs.iter().all(|xref| xref.target_is_local));
    let warnings = parsed.warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    for (warning, expected) in warnings.iter().zip(["missing.id", "missing:id"]) {
        assert!(
            matches!(&warning.kind, WarningKind::UnresolvedReference { target } if target == expected)
        );
        assert_eq!(
            warning
                .source_location()
                .ok_or("missing source")?
                .location
                .start
                .line,
            3
        );
    }
    Ok(())
}

#[test]
fn forced_fragments_are_not_reinterpreted_as_source_paths() -> Result<(), Error> {
    let options = Options::builder()
        .with_base_dir("fixtures/preprocessor/xref_catalog")
        .with_safe_mode(SafeMode::Unsafe)
        .with_force_local_file_xrefs()
        .build()?;
    let parsed = parse(
        "xref:other.adoc#chapter.adoc[Literal ID]\n\ninclude::chapter.adoc[]\n",
        &options,
    )?;
    let xrefs = paragraph_xrefs(&parsed.document().blocks);
    let [xref] = xrefs.as_slice() else {
        return Err("missing xref".into());
    };
    assert_eq!(xref.target, "chapter.adoc");
    assert!(xref.target_is_local);
    assert!(!parsed.document().references.contains_key(""));
    assert!(
        matches!(parsed.warnings(), [warning] if matches!(&warning.kind, WarningKind::UnresolvedReference { target } if target == "chapter.adoc"))
    );
    Ok(())
}

#[test]
fn forcing_a_fragment_does_not_load_the_named_file() -> Result<(), Error> {
    let options = Options::builder()
        .with_base_dir("fixtures/tests")
        .with_safe_mode(SafeMode::Unsafe)
        .with_force_local_file_xrefs()
        .build()?;
    let parsed = parse("xref:xref_include_target.adoc#included-target[]", &options)?;
    assert!(parsed.document().references.is_empty());
    assert!(
        matches!(parsed.warnings(), [warning] if matches!(&warning.kind, WarningKind::UnresolvedReference { target } if target == "included-target"))
    );
    Ok(())
}

#[test]
fn forcing_partial_sources_also_redirects_external_file_references() -> Result<(), Error> {
    let source = "= Book\n\nxref:chapter.adoc#catalog-target[] xref:outside.adoc#catalog-target[Outside]\n\ninclude::chapter.adoc[lines=1..2]\n";
    for force_local in [false, true] {
        let mut builder = Options::builder()
            .with_base_dir("fixtures/preprocessor/xref_catalog")
            .with_safe_mode(SafeMode::Unsafe);
        if force_local {
            builder = builder.with_force_local_file_xrefs();
        }
        let parsed = parse(source, &builder.build()?)?;
        assert!(parsed.document().references.contains_key("catalog-target"));
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
        let targets = paragraph_xrefs(&parsed.document().blocks)
            .iter()
            .map(|xref| (xref.target, xref.target_is_local))
            .collect::<Vec<_>>();
        let expected = if force_local {
            [("catalog-target", true), ("catalog-target", true)]
        } else {
            [
                ("chapter.adoc#catalog-target", false),
                ("outside.adoc#catalog-target", false),
            ]
        };
        assert_eq!(targets, expected);
    }
    Ok(())
}

#[test]
fn prefixes_that_look_like_uri_schemes_are_preserved() -> Result<(), Error> {
    for target in [
        "guide:chapter.adoc#intro",
        "component:guide:chapter.adoc#intro",
    ] {
        for source in [
            format!("<<{target},Section>>"),
            format!("xref:{target}[Section]"),
        ] {
            assert_eq!(
                xref(&source, true)?,
                (target.to_string(), "Section".to_string())
            );
        }
    }
    Ok(())
}

#[test]
fn scoped_assembly_anchors_work_without_forcing_file_references() -> Result<(), Error> {
    let source = "xref:a:::intro[Alpha] xref:b:::intro[Beta]\n\n[[a:::intro]]\n== Alpha intro\n\n[[b:::intro]]\n== Beta intro\n";
    for force_local in [false, true] {
        let mut builder = Options::builder();
        if force_local {
            builder = builder.with_force_local_file_xrefs();
        }
        let parsed = parse(source, &builder.build()?)?;
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
        let targets = paragraph_xrefs(&parsed.document().blocks)
            .iter()
            .map(|xref| (xref.target, xref.target_is_local))
            .collect::<Vec<_>>();
        assert_eq!(targets, [("a:::intro", true), ("b:::intro", true)]);
    }
    Ok(())
}
