use std::io::Cursor;

use acdc_parser::{
    Block, CrossReference, InlineMacro, InlineNode, Options, WarningKind, parse, parse_file,
    parse_from_reader,
};

type Error = Box<dyn std::error::Error>;

// Fixture JSON omits target classification, reference catalogs, and diagnostics.
fn cross_references<'d, 'a>(blocks: &'d [Block<'a>]) -> Vec<&'d CrossReference<'a>> {
    let mut found = Vec::new();
    for block in blocks {
        if let Block::Paragraph(paragraph) = block {
            for inline in &paragraph.content {
                if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline {
                    found.push(xref);
                }
            }
        } else if let Block::Section(section) = block {
            found.extend(cross_references(&section.content));
        }
    }
    found
}

#[test]
fn included_references_report_only_missing_local_fragments() -> Result<(), Error> {
    let parsed = parse_file(
        "fixtures/tests/xref_included_sources.adoc",
        &Options::default(),
    )?;
    let targets = cross_references(&parsed.document().blocks);
    assert_eq!(
        targets.iter().filter(|xref| xref.target_is_local).count(),
        7
    );
    let [warning] = parsed.warnings() else {
        return Err(format!("unexpected warnings: {:?}", parsed.warnings()).into());
    };
    assert!(
        matches!(&warning.kind, WarningKind::UnresolvedReference { target } if target == "missing-target")
    );
    let source = warning.source_location().ok_or("missing source")?;
    assert_eq!(source.location.start.line, 17);
    Ok(())
}

#[test]
fn qualified_references_do_not_load_other_sources() -> Result<(), Error> {
    let options = Options::builder().with_base_dir("fixtures/tests").build()?;
    let source = "xref:xref_include_target.adoc#included-target[]";
    for parsed in [
        parse(source, &options)?,
        parse_from_reader(Cursor::new(source), &options)?,
    ] {
        assert!(parsed.document().references.is_empty());
        assert!(parsed.warnings().is_empty());
        let targets = cross_references(&parsed.document().blocks);
        let [xref] = targets.as_slice() else {
            return Err("missing xref".into());
        };
        assert!(!xref.target_is_local);
    }
    Ok(())
}

#[test]
fn included_reference_classification_preserves_punctuation_and_syntax() -> Result<(), Error> {
    let options = Options::builder()
        .with_base_dir("fixtures/preprocessor/xref_catalog")
        .build()?;
    let parsed = parse(
        "<<chapter.adoc#missing.id>> xref:chapter.adoc#missing:id[]\n\n<<chapter.txt#catalog-target>> xref:chapter.txt#catalog-target[]\n\ninclude::chapter.adoc[]\n",
        &options,
    )?;
    let targets = cross_references(&parsed.document().blocks);
    assert_eq!(
        targets
            .iter()
            .map(|xref| (xref.target, xref.target_is_local))
            .collect::<Vec<_>>(),
        [
            ("missing.id", true),
            ("missing:id", true),
            ("catalog-target", true),
            ("chapter.txt#catalog-target", false),
        ]
    );
    let missing = parsed
        .warnings()
        .iter()
        .filter_map(|warning| {
            if let WarningKind::UnresolvedReference { target } = &warning.kind {
                Some(target.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(missing, ["missing.id", "missing:id"]);
    Ok(())
}

#[test]
fn partial_sources_keep_qualified_references_external() -> Result<(), Error> {
    let options = Options::builder()
        .with_base_dir("fixtures/preprocessor/xref_catalog")
        .build()?;
    for selector in ["lines=1..6", "tags=body", "opts=partial"] {
        let source =
            format!("xref:chapter.adoc#catalog-target[]\n\ninclude::chapter.adoc[{selector}]\n");
        let parsed = parse(&source, &options)?;
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
        assert!(
            cross_references(&parsed.document().blocks)
                .iter()
                .all(|xref| !xref.target_is_local)
        );
    }
    Ok(())
}

#[test]
fn document_top_references_work_with_empty_includes() -> Result<(), Error> {
    let options = Options::builder()
        .with_base_dir("fixtures/preprocessor/xref_catalog")
        .build()?;
    for header in [
        "",
        "= Main: Subtitle\n\n",
        "[reftext=Custom]\n= Main\n\n",
        "= Main\n:reftext: *Custom*\n\n",
        ":reftext: *Custom*\n\n",
    ] {
        let source =
            format!("{header}xref:empty.adoc[] xref:empty.adoc#[]\n\ninclude::empty.adoc[]\n");
        let parsed = parse(&source, &options)?;
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
        assert!(
            cross_references(&parsed.document().blocks)
                .iter()
                .all(|xref| xref.target_is_local && xref.target.is_empty())
        );
        let top = parsed
            .document()
            .references
            .get("")
            .ok_or("missing document top")?;
        assert_eq!(top.title.is_some(), header.contains("= Main"));
        assert_eq!(top.xreflabel.is_some(), header.contains("reftext"));
        if header.contains("*Custom*") {
            assert!(top.xreflabel.as_ref().is_some_and(|label| {
                label
                    .iter()
                    .any(|inline| matches!(inline, InlineNode::BoldText(_)))
            }));
        }
    }
    Ok(())
}

#[test]
fn restored_source_targets_keep_their_destination_classification() -> Result<(), Error> {
    let options = Options::builder()
        .with_base_dir("fixtures/preprocessor/xref_catalog")
        .build()?;
    let parsed = parse(
        "xref:++chapter.adoc++#catalog-target[] xref:++other.adoc++#catalog-target[]\n\ninclude::chapter.adoc[]\n",
        &options,
    )?;
    let targets = cross_references(&parsed.document().blocks);
    assert_eq!(
        targets
            .iter()
            .map(|xref| (xref.target, xref.target_is_local))
            .collect::<Vec<_>>(),
        [
            ("catalog-target", true),
            ("other.adoc#catalog-target", false)
        ]
    );
    Ok(())
}
