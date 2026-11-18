use super::{TestResult, support};
use acdc_parser::{
    CaptionKind, CompatibilityMode, Options, WarningKind,
    ast::{BlockKind, CaptionRef, InlineKind},
    extensions::{BlockBuilder, BlockContext, DocumentEditor, InlineSpec},
};

#[test]
fn extensions_generated_paragraph_has_explicit_located_line_breaks() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse("Before.\n\nReplace me.", &options)?;
        let target = result
            .query()
            .blocks()
            .last()
            .ok_or("target")?
            .target()
            .clone();
        let origin = result
            .query()
            .get(&target)
            .ok_or("target")?
            .source_location();
        DocumentEditor::new(&mut result).replace_block(
            &target,
            BlockBuilder::paragraph_inlines([
                InlineSpec::Text("*literal* <&>".into()),
                InlineSpec::LineBreak,
                InlineSpec::Text("Second".into()),
                InlineSpec::LineBreak,
            ]),
        )?;
        result.normalize()?;
        let parsed = result.finish()?;
        let BlockKind::Paragraph(paragraph) =
            parsed.document().blocks().last().ok_or("paragraph")?.kind()
        else {
            return Err("paragraph kind".into());
        };
        let nodes: Vec<_> = paragraph.content().inlines().collect();
        assert_eq!(nodes.len(), 4);
        assert!(
            matches!(nodes[0].kind(), InlineKind::Plain(text) if text.text() == "*literal* <&>")
        );
        for node in [nodes[1], nodes[3]] {
            assert!(matches!(node.kind(), InlineKind::LineBreak(_)));
            assert_eq!(
                acdc_parser::Location::from(node.location()),
                origin.location
            );
        }
    }
    Ok(())
}

#[test]
fn extensions_inline_sequences_preserve_paragraph_order_and_literal_text() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse("Replace me.", &options)?;
        let target = result
            .query()
            .blocks()
            .next()
            .ok_or("target")?
            .target()
            .clone();
        let collected: InlineSpec = [
            InlineSpec::Text("*literal* <&> {counter:x}".into()),
            InlineSpec::LineBreak,
        ]
        .into_iter()
        .collect();
        DocumentEditor::new(&mut result).replace_block(
            &target,
            BlockBuilder::paragraph_inlines([
                InlineSpec::from("Before "),
                InlineSpec::Sequence(vec![
                    InlineSpec::Sequence(Vec::new()),
                    InlineSpec::Sequence(vec![collected]),
                ]),
                InlineSpec::from([InlineSpec::from("after")]),
            ]),
        )?;
        let parsed = result.finish()?;
        assert_eq!(parsed.warnings(), []);
        let BlockKind::Paragraph(paragraph) =
            parsed.document().blocks().next().ok_or("output")?.kind()
        else {
            return Err("paragraph".into());
        };
        let mut content = paragraph.content().inlines();
        for expected in ["Before ", "*literal* <&> {counter:x}"] {
            assert!(
                matches!(content.next().ok_or("literal")?.kind(), InlineKind::Plain(text) if text.text() == expected)
            );
        }
        assert!(matches!(
            content.next().ok_or("break")?.kind(),
            InlineKind::LineBreak(_)
        ));
        assert!(
            matches!(content.next().ok_or("suffix")?.kind(), InlineKind::Plain(text) if text.text() == "after")
        );
        assert!(content.next().is_none());
    }
    Ok(())
}

#[test]
fn extensions_located_warnings_survive_edits_and_reject_foreign_targets() -> TestResult {
    let options = Options::default();
    let mut result = support::registry().parse("Before.\n\n== Remove\n\nBody.", &options)?;
    let other = support::registry().parse("Foreign.", &options)?;
    let foreign = other
        .query()
        .blocks()
        .next()
        .ok_or("foreign")?
        .target()
        .clone();
    let target = result
        .query()
        .select(BlockContext::Section)
        .next()
        .ok_or("section")?
        .target()
        .clone();
    let origin = result
        .query()
        .get(&target)
        .ok_or("target")?
        .source_location();
    let mut editor = DocumentEditor::new(&mut result);
    assert!(editor.warn(&foreign, "foreign warning").is_err());
    editor.warn(&target, "recoverable extension problem")?;
    editor.warn(&target, "recoverable extension problem")?;
    editor.remove_section(&target)?;
    assert!(editor.warn(&target, "removed warning").is_err());
    result.normalize()?;
    let parsed = result.finish()?;
    assert_eq!(parsed.warnings().len(), 1);
    assert_eq!(parsed.warnings()[0].source_location(), Some(&origin));
    assert!(
        matches!(&parsed.warnings()[0].kind, WarningKind::Other(message) if message == "recoverable extension problem")
    );
    Ok(())
}

#[test]
fn extensions_image_replacement_preserves_identity_title_and_source_context() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let source = "= Document\n:label: Early\n:figure-caption: Picture\n\n.Original *{label}*\n[#diagram.keep%nowrap,reftext='{label} *label*']\n[source,rust]\n----\noriginal\n----\n\n:label: Late\n:figure-caption: Later\n\n.Existing\nimage::existing.svg[]";
        let mut result = support::registry().parse(source, &options)?;
        let target = result
            .query()
            .select(BlockContext::Listing)
            .next()
            .ok_or("listing")?
            .target()
            .clone();
        let before = result.query().get(&target).ok_or("original")?;
        let title = serde_json::to_value(before.title())?;
        let location = before.source_location();
        let mut editor = DocumentEditor::new(&mut result);
        editor.replace_block(
            &target,
            BlockBuilder::image("generated.svg")
                .with_default_attributes("'Default alt',100,200,title='Default',role=weak")
                .with_attributes("'Chosen alt',320,240,width=640,link=target.html"),
        )?;
        let query = editor.query();
        let view = query.get(&target).ok_or("replacement")?;
        assert_eq!(view.output_id(), Some("diagram"));
        assert_eq!(view.source_location(), location);
        assert_eq!(serde_json::to_value(view.title())?, title);
        assert!(
            matches!(view.caption(), Some(CaptionRef::Numbered { label: "Picture", number: Some(number), .. }) if number.get() == 2)
        );
        let BlockKind::Image(image) = view.block().kind() else {
            return Err("image".into());
        };
        assert!(image.roles().iter().any(|role| role == "keep"));
        assert!(image.options().iter().any(|option| option == "nowrap"));
        let stored = view.storage().metadata().ok_or("metadata")?;
        assert_eq!(
            stored
                .attributes
                .get("width")
                .map(ToString::to_string)
                .as_deref(),
            Some("640")
        );
        assert_eq!(
            stored
                .attributes
                .get("height")
                .map(ToString::to_string)
                .as_deref(),
            Some("240")
        );
        let parsed = result.finish()?;
        let label = parsed
            .document()
            .references()
            .get("diagram")
            .and_then(crate::ast::ReferenceTargetRef::xreflabel)
            .ok_or("reference label")?;
        assert!(
            label
                .inlines()
                .any(|inline| matches!(inline.kind(), InlineKind::Bold(_)))
        );
    }
    Ok(())
}

#[test]
fn extensions_literal_diagrams_keep_custom_disabled_and_generated_captions() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse("= Document\n\n.Custom\n[caption='*Custom* ']\ndefer::one[]\n\n:figure-caption!:\n\n.Disabled\ndefer::two[]\n\n:figure-caption: Later\n\n.Numbered\ndefer::three[]", &options)?;
        let targets: Vec<_> = result
            .deferred_blocks()
            .iter()
            .map(|request| request.target().clone())
            .collect();
        let mut editor = DocumentEditor::new(&mut result);
        for (position, target) in targets.iter().enumerate() {
            let literal = BlockBuilder::literal("*literal* <&>\n  spaces");
            editor.replace_placeholder(
                target,
                if position == 0 {
                    literal
                } else {
                    literal.with_caption_kind(CaptionKind::Figure)
                },
            )?;
        }
        let query = editor.query();
        let first = query.get(targets.first().ok_or("first")?).ok_or("custom")?;
        assert!(matches!(
            first.caption(),
            Some(CaptionRef::CustomContent(_))
        ));
        assert!(matches!(
            query
                .get(targets.get(1).ok_or("second")?)
                .and_then(|view| view.caption()),
            Some(CaptionRef::Unnumbered)
        ));
        assert!(
            matches!(query.get(targets.get(2).ok_or("third")?).and_then(|view| view.caption()), Some(CaptionRef::Numbered { label: "Later", number: Some(number), .. }) if number.get() == 1)
        );
        assert!(result.deferred_blocks().is_empty());
        let _ = result.finish()?;
    }
    Ok(())
}

#[test]
fn extensions_generated_captions_respect_local_counter_seeds_without_reusing_numbers() -> TestResult
{
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse(
            "= Document\n:figure-number: 8\n\n.First\ndefer::first[]\n\n:figure-number: 40\n\n.Second\ndefer::second[]\n\n:figure-number: 2\n\n.Third\ndefer::third[]",
            &options,
        )?;
        let targets: Vec<_> = result
            .deferred_blocks()
            .iter()
            .map(|request| request.target().clone())
            .collect();
        let mut editor = DocumentEditor::new(&mut result);
        for target in &targets {
            editor.replace_placeholder(target, BlockBuilder::image("generated.svg"))?;
        }
        for (target, expected) in targets.iter().zip([9, 41, 42]) {
            assert!(
                matches!(editor.query().get(target).and_then(|view| view.caption()), Some(CaptionRef::Numbered {number: Some(number), ..}) if number.get() == expected)
            );
        }
        let before = serde_json::to_value(result.parsed.document())?;
        result.normalize()?;
        assert_eq!(serde_json::to_value(result.parsed.document())?, before);
        let _ = result.finish()?;
    }
    Ok(())
}

#[test]
fn extensions_repair_failure_does_not_publish_edits_or_consume_caption_numbers() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse(
            "= Document\n:sectnums:\n\n== Section\n\n.Existing\nimage::existing.svg[]\n\n[#body]\nOriginal footnote:cycle[Original definition].",
            &options,
        )?;
        let target = result
            .query()
            .select(BlockContext::Paragraph)
            .next()
            .ok_or("body")?
            .target()
            .clone();
        let before = serde_json::to_value(result.parsed.document())?;
        let warnings = format!("{:?}", result.warnings());
        let mut editor = DocumentEditor::new(&mut result);
        let error = editor.replace_block(
            &target,
            BlockBuilder::image("new.svg")
                .with_attributes("title='footnote:cycle[footnote:cycle[]]'"),
        );
        assert!(error.is_err(), "cyclic retained footnote must fail repair");
        assert_eq!(serde_json::to_value(editor.document_storage())?, before);
        assert_eq!(
            editor.query().get(&target).ok_or("unchanged")?.context(),
            BlockContext::Paragraph
        );
        editor.replace_block(
            &target,
            BlockBuilder::image("new.svg").with_attributes("title='Valid'"),
        )?;
        assert!(
            matches!(editor.query().get(&target).and_then(|view| view.caption()), Some(CaptionRef::Numbered {number: Some(number), ..}) if number.get() == 2)
        );
        assert_eq!(format!("{:?}", result.warnings()), warnings);
        let _ = result.finish()?;
    }
    Ok(())
}

#[test]
fn extensions_reference_repair_failure_restores_each_splice() -> TestResult {
    let mut result = support::registry().parse(
        "= Document\n\n== Section\n\nInside.\n\ndefer::[]",
        &Options::default(),
    )?;
    let target = result
        .deferred_blocks()
        .first()
        .ok_or("placeholder")?
        .target()
        .clone();
    // Simulate a prepared section whose reference membership cannot be rebuilt.
    result.parsed.with_document_mut(|_, document| {
        if let Some(acdc_parser::Block::Section(section)) = document.blocks.first_mut() {
            section.id = None;
        }
    });
    let before = serde_json::to_value(result.parsed.document())?;
    let mut editor = DocumentEditor::new(&mut result);
    assert!(
        editor
            .insert_before(&target, BlockBuilder::paragraph("insert"))
            .is_err()
    );
    assert_eq!(serde_json::to_value(editor.document_storage())?, before);
    assert!(
        editor
            .replace_placeholder(&target, BlockBuilder::paragraph("replace"))
            .is_err()
    );
    assert_eq!(serde_json::to_value(editor.document_storage())?, before);
    assert!(editor.discard_placeholder(&target).is_err());
    assert_eq!(serde_json::to_value(editor.document_storage())?, before);
    assert_eq!(
        editor
            .deferred_blocks()
            .first()
            .ok_or("pending placeholder")?
            .target(),
        &target
    );
    assert!(editor.query().get(&target).is_some());
    Ok(())
}

#[test]
fn extensions_verbatim_styles_follow_ordinary_caption_rules() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (content, numbered) in [
            (
                BlockBuilder::literal("raw").with_attributes("source,rust"),
                true,
            ),
            (
                BlockBuilder::listing("raw").with_attributes("literal"),
                false,
            ),
            (BlockBuilder::listing("raw"), true),
        ] {
            let mut result = support::registry().parse(
                "= Document\n:listing-caption: Code\n\n.Title\ndefer::[]",
                &options,
            )?;
            let target = result
                .deferred_blocks()
                .first()
                .ok_or("request")?
                .target()
                .clone();
            DocumentEditor::new(&mut result).replace_placeholder(&target, content)?;
            assert_eq!(
                matches!(
                    result.query().get(&target).and_then(|view| view.caption()),
                    Some(CaptionRef::Numbered {
                        kind: CaptionKind::Listing,
                        number: Some(_),
                        ..
                    })
                ),
                numbered
            );
        }
    }
    Ok(())
}

#[test]
fn extensions_caption_overflow_rejects_the_edit_without_consuming_the_request() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse(
            "= Document\n:figure-number: 4294967295\n\n.Title\ndefer::[]",
            &options,
        )?;
        let target = result
            .deferred_blocks()
            .first()
            .ok_or("overflow request")?
            .target()
            .clone();
        let before = serde_json::to_value(result.parsed.document())?;
        assert!(
            DocumentEditor::new(&mut result)
                .replace_placeholder(&target, BlockBuilder::image("x.svg"))
                .is_err()
        );
        assert_eq!(serde_json::to_value(result.parsed.document())?, before);
        assert_eq!(result.deferred_blocks().len(), 1);
    }
    Ok(())
}

#[test]
fn extensions_source_replacement_uses_local_substitutions_and_repairs_references() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse("= Document\n:label: Early\n\n[#body]\nOld footnote:old[Old definition] <<missing>>.\n\n[subs=attributes]\n----\nOld\n----\n\n:label: Late\n\n[#later]\nLater.", &options)?;
        let paragraph = result
            .query()
            .select(BlockContext::Paragraph)
            .next()
            .ok_or("paragraph")?
            .target()
            .clone();
        let listing = result
            .query()
            .select(BlockContext::Listing)
            .next()
            .ok_or("listing")?
            .target()
            .clone();
        assert!(
            result
                .warnings()
                .iter()
                .any(|warning| matches!(warning.kind, WarningKind::UnresolvedReference { .. }))
        );
        let mut editor = DocumentEditor::new(&mut result);
        editor.replace_source(
            &paragraph,
            "New *{label}* footnote:new[New definition] <<later>>.",
        )?;
        editor.replace_source(&listing, "*{label}* <&>\n  spaces")?;
        let query = editor.query();
        let view = query.get(&paragraph).ok_or("paragraph")?;
        let BlockKind::Paragraph(block) = view.block().kind() else {
            return Err("paragraph".into());
        };
        assert_eq!(
            block.source_text(),
            Some("New *{label}* footnote:new[New definition] <<later>>.")
        );
        assert!(
            block
                .content()
                .inlines()
                .any(|inline| matches!(inline.kind(), InlineKind::Bold(_)))
        );
        let before = serde_json::to_value(result.parsed.document())?;
        let warnings = format!("{:?}", result.warnings());
        result.normalize()?;
        result.normalize()?;
        assert_eq!(serde_json::to_value(result.parsed.document())?, before);
        assert_eq!(format!("{:?}", result.warnings()), warnings);
        assert!(
            !result
                .warnings()
                .iter()
                .any(|warning| matches!(warning.kind, WarningKind::UnresolvedReference { .. }))
        );
        assert_eq!(result.parsed.document().footnotes().count(), 1);
        let _ = result.finish()?;
    }
    Ok(())
}

#[test]
fn extensions_invalid_construction_is_atomic_and_removes_replaced_descendants() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse(
            "[#container]\n--\n.Child\n[#child]\ndefer::[]\n--\n\n[#duplicate]\nOther.",
            &options,
        )?;
        let target = result
            .query()
            .select(BlockContext::Open)
            .next()
            .ok_or("open")?
            .target()
            .clone();
        let child = result
            .deferred_blocks()
            .first()
            .ok_or("child")?
            .target()
            .clone();
        let before = serde_json::to_value(result.parsed.document())?;
        let warnings = format!("{:?}", result.warnings());
        let mut editor = DocumentEditor::new(&mut result);
        assert!(
            editor
                .replace_block(&target, BlockBuilder::image("x.svg").with_id("duplicate"))
                .is_err()
        );
        assert!(
            editor
                .replace_block(&target, BlockBuilder::image("http://[invalid"))
                .is_err()
        );
        assert!(
            editor
                .replace_block(
                    &target,
                    BlockBuilder::paragraph("x").with_attributes("role=one\nrole=two")
                )
                .is_err()
        );
        assert_eq!(serde_json::to_value(editor.document_storage())?, before);
        assert!(editor.query().get(&child).is_some());
        editor.replace_block(&target, BlockBuilder::literal("Replacement"))?;
        assert!(editor.query().get(&child).is_none());
        assert!(editor.ensure_id(&child).is_err());
        assert!(editor.deferred_blocks().is_empty());
        assert_eq!(format!("{:?}", result.warnings()), warnings);
        assert!(result.parsed.document().references().get("child").is_none());
        let _ = result.finish()?;
    }
    Ok(())
}

#[test]
fn extensions_generated_metadata_distinguishes_empty_values_and_unsets() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse(
            ".Original\n[#image.keep%nowrap,reftext='Label']\nimage::old.svg['Original alt',100,200]",
            &options,
        )?;
        let target = result
            .query()
            .select(BlockContext::Image)
            .next()
            .ok_or("image")?
            .target()
            .clone();
        let mut editor = DocumentEditor::new(&mut result);
        editor.replace_block(
            &target,
            BlockBuilder::image("new.svg")
                .with_default_attributes("'Weak alt',width=10,height=20")
                .with_attributes("alt='',width='',role='',opts='',title=''"),
        )?;
        let query = editor.query();
        let view = query.get(&target).ok_or("empty image")?;
        assert!(view.title().is_some());
        let metadata = view.storage().metadata().ok_or("metadata")?;
        assert_eq!(
            metadata.roles.is_empty(),
            mode == CompatibilityMode::Asciidoctor
        );
        assert!(metadata.options.contains(&"nowrap"));
        assert_eq!(
            metadata
                .attributes
                .get("width")
                .map(ToString::to_string)
                .as_deref(),
            Some("")
        );
        assert_eq!(
            metadata
                .attributes
                .get("height")
                .map(ToString::to_string)
                .as_deref(),
            Some("200")
        );
        let BlockKind::Image(image) = view.block().kind() else {
            return Err("image".into());
        };
        assert!(
            image
                .alt()
                .is_some_and(|alt| alt.inlines().next().is_none())
        );
        editor.replace_block(
            &target,
            BlockBuilder::image("last.svg")
                .without_attribute("id")
                .without_attribute("title")
                .without_attribute("reftext")
                .without_attribute("roles")
                .without_attribute("options")
                .without_attribute("alt")
                .without_attribute("height"),
        )?;
        let query = editor.query();
        let view = query.get(&target).ok_or("unset image")?;
        assert_eq!(view.output_id(), None);
        assert!(view.title().is_none());
        let metadata = view.storage().metadata().ok_or("metadata")?;
        assert!(metadata.reftext.is_none());
        assert!(metadata.roles.is_absent());
        assert!(metadata.options.is_absent());
        assert!(!metadata.attributes.contains_key("height"));
        let BlockKind::Image(image) = view.block().kind() else {
            return Err("image".into());
        };
        assert!(image.alt().is_none());
    }
    Ok(())
}

#[test]
fn extensions_source_replacement_reuses_scoped_notes_without_number_collisions() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse(
            "Original footnote:[Anonymous] footnote:shared[Named].\n\n[#body]\nReplace.\n\n[cols=a]\n|===\n|Cell footnote:shared[Cell named].\n\n[#cell]\nReplace cell.\n|===",
            &options,
        )?;
        let targets: Vec<_> = result
            .query()
            .select(BlockContext::Paragraph)
            .filter(|view| matches!(view.output_id(), Some("body" | "cell")))
            .map(|view| view.target().clone())
            .collect();
        let mut editor = DocumentEditor::new(&mut result);
        for target in targets {
            editor.replace_source(&target, "New footnote:[New anonymous] footnote:shared[].")?;
        }
        result.normalize()?;
        assert!(result.warnings().is_empty(), "{:?}", result.warnings());
        let original = result
            .query()
            .select(BlockContext::Paragraph)
            .next()
            .ok_or("original")?
            .target()
            .clone();
        DocumentEditor::new(&mut result)
            .replace_source(&original, "Updated footnote:shared[Updated named].")?;
        let document = result.finish()?;
        assert_eq!(document.document().footnotes().count(), 2);
        assert!(serde_json::to_string(&document.document())?.contains("Updated named"));
    }
    Ok(())
}

#[test]
fn extensions_normalize_preserves_unedited_numbers_and_clears_stale_duplicate_warnings()
-> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse(
            "= Document\n:sectnums:\n\n== First\n\n[#body]\n[[duplicate]] Original.\n\n== Second\n\n[[duplicate]] Other.",
            &options,
        )?;
        let before = serde_json::to_value(result.parsed.document())?;
        result.normalize()?;
        result.normalize()?;
        assert_eq!(serde_json::to_value(result.parsed.document())?, before);
        assert!(
            result
                .warnings()
                .iter()
                .any(|warning| matches!(warning.kind, WarningKind::DuplicateId { .. }))
        );
        let target = result
            .query()
            .select(BlockContext::Paragraph)
            .find(|view| view.output_id() == Some("body"))
            .ok_or("body")?
            .target()
            .clone();
        DocumentEditor::new(&mut result).replace_source(&target, "Replacement.")?;
        assert!(
            !result
                .warnings()
                .iter()
                .any(|warning| matches!(warning.kind, WarningKind::DuplicateId { .. }))
        );
        DocumentEditor::new(&mut result).replace_source(&target, "[[fresh]] One [[fresh]] two.")?;
        assert_eq!(
            result
                .warnings()
                .iter()
                .filter(|warning| matches!(warning.kind, WarningKind::DuplicateId { .. }))
                .count(),
            1
        );
    }
    Ok(())
}
