use super::{TestResult, support};
use acdc_parser::{
    CompatibilityMode, Options, WarningKind,
    ast::{BlockKind, InlineKind},
    extensions::{
        BlockBuilder, BlockContext, DisplayIntent, DisplayLabel, DocumentEditor, InlineSpec,
        PreparedDocument, TitleSource,
    },
};

fn modes() -> [CompatibilityMode; 2] {
    [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft]
}

#[test]
fn extensions_unordered_list_combines_literal_and_grouped_display_items() -> TestResult {
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse(
            ".A *title*\n[#image]\nimage::one.svg[]\n\nOutput.",
            &options,
        )?;
        let image = result
            .query()
            .select(BlockContext::Image)
            .next()
            .ok_or("image")?
            .target()
            .clone();
        let output = result
            .query()
            .select(BlockContext::Paragraph)
            .next()
            .ok_or("output")?
            .target()
            .clone();
        let owned = String::from("*literal* <&> {counter:x}");
        let mut editor = DocumentEditor::new(&mut result, None);
        let captured = editor.capture(&image)?;
        editor.replace_block(
            &output,
            BlockBuilder::unordered_list([
                InlineSpec::from(owned.clone()),
                InlineSpec::from(&owned),
                InlineSpec::from(owned.as_str()),
                InlineSpec::Sequence(vec![
                    InlineSpec::from("Before "),
                    InlineSpec::from(vec![captured.display(DisplayIntent::Title)]),
                    InlineSpec::from(" after"),
                ]),
                InlineSpec::from(Vec::new()),
            ]),
        )?;
        let parsed = result.finish()?;
        let prepared = PreparedDocument::new(parsed.document(), |_| DisplayLabel::Title)?;
        let BlockKind::UnorderedList(list) =
            prepared.document().blocks().last().ok_or("list")?.kind()
        else {
            return Err("list kind".into());
        };
        let mut items = list.items();
        for _ in 0..3 {
            let content = items.next().ok_or("literal item")?.principal();
            assert_eq!(content.inlines().len(), 1);
            assert!(
                matches!(content.inlines().next().ok_or("literal")?.kind(), InlineKind::Plain(text) if text.text() == owned)
            );
        }
        let mut content = items.next().ok_or("structured item")?.principal().inlines();
        assert!(
            matches!(content.next().ok_or("prefix")?.kind(), InlineKind::Plain(text) if text.text() == "Before ")
        );
        let InlineKind::CrossReference(xref) = content.next().ok_or("xref")?.kind() else {
            return Err("xref kind".into());
        };
        assert_eq!(xref.target(), "image");
        assert!(
            xref.text()
                .inlines()
                .any(|inline| matches!(inline.kind(), InlineKind::Bold(_)))
        );
        assert!(
            matches!(content.next().ok_or("suffix")?.kind(), InlineKind::Plain(text) if text.text() == " after")
        );
        assert!(content.next().is_none());
        assert!(items.next().ok_or("empty item")?.principal().is_empty());
        assert!(items.next().is_none());
    }
    Ok(())
}

#[test]
fn extensions_display_freezes_titles_and_defers_renamed_target_captions() -> TestResult {
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse(
            "= Document\n:label: Early\n\n.Original *{label}* <&>\n[#figure,reftext='Custom reference']\nimage::original.svg[]\n\n:label: Late\n\n[#output]\nOutput.", &options,
        )?;
        let target = result
            .query()
            .select(BlockContext::Image)
            .next()
            .ok_or("image")?
            .target()
            .clone();
        let output = result
            .query()
            .select(BlockContext::Paragraph)
            .next()
            .ok_or("paragraph")?
            .target()
            .clone();
        let mut editor = DocumentEditor::new(&mut result, None);
        let selected = editor.capture(&target)?;
        let second = editor.capture(&target)?;
        editor.replace_block(
            &output,
            BlockBuilder::unordered_list([
                vec![selected.display(DisplayIntent::Title)],
                vec![selected.display(DisplayIntent::CaptionAndTitle)],
                vec![second.display(DisplayIntent::Caption)],
            ]),
        )?;
        editor.replace_block(
            &target,
            BlockBuilder::listing("changed")
                .with_attributes("id=renamed,title='Changed *later*',caption='*Updated* '"),
        )?;
        let parsed = result.finish()?;
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
        let before = serde_json::to_value(parsed.document())?;
        let mut observed = 0;
        for _ in 0..2 {
            let prepared = PreparedDocument::new(parsed.document(), |request| {
                observed += 1;
                assert_eq!(request.target(), "renamed");
                assert!(!request.is_missing());
                assert_eq!(
                    request.title_source().map(TitleSource::raw_source),
                    Some("Original *{label}* <&>")
                );
                assert_eq!(
                    request
                        .title_source()
                        .and_then(|source| source.attributes().get("label"))
                        .and_then(|value| value.text()),
                    Some("Early")
                );
                if request.intent() == DisplayIntent::Title {
                    DisplayLabel::Title
                } else {
                    DisplayLabel::Caption { trim_end: true }
                }
            })?;
            assert!(!prepared.document().has_deferred_displays());
            let block = prepared.document().blocks().last().ok_or("list")?;
            let BlockKind::UnorderedList(list) = block.kind() else {
                return Err("list kind".into());
            };
            for item in list.items() {
                let mut nodes = item.principal().inlines();
                let InlineKind::CrossReference(xref) = nodes.next().ok_or("xref")?.kind() else {
                    return Err("xref kind".into());
                };
                assert_eq!(xref.target(), "renamed");
                assert!(xref.generated_display().is_none());
                assert!(
                    xref.text()
                        .inlines()
                        .any(|inline| matches!(inline.kind(), InlineKind::Bold(_))),
                    "{:?}",
                    xref.text()
                );
            }
            assert_eq!(serde_json::to_value(parsed.document())?, before);
        }
        assert_eq!(observed, 6);
    }
    Ok(())
}

#[test]
fn extensions_display_removed_identity_never_rebinds_to_reused_id() -> TestResult {
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse(
            "= Document\n\n[#gone]\n== Removed\n\n[#figure]\n.Original\nimage::old.svg[]\n\n[#keep]\n== Kept\n\n[#output]\nOutput.", &options,
        )?;
        let image = result
            .query()
            .select(BlockContext::Image)
            .next()
            .ok_or("image")?
            .target()
            .clone();
        let section = result
            .query()
            .select(BlockContext::Section)
            .next()
            .ok_or("section")?
            .target()
            .clone();
        let output = result
            .query()
            .select(BlockContext::Paragraph)
            .next()
            .ok_or("output")?
            .target()
            .clone();
        let origin = result
            .query()
            .get(&output)
            .ok_or("output view")?
            .source_location();
        let mut editor = DocumentEditor::new(&mut result, None);
        let captured = editor.capture(&image)?;
        editor.replace_block(
            &output,
            BlockBuilder::paragraph_inlines([captured.display(DisplayIntent::Title)]),
        )?;
        editor.remove_section(&section)?;
        editor.insert_before(
            &output,
            BlockBuilder::paragraph("Replacement").with_attributes("id=figure"),
        )?;
        let parsed = result.finish()?;
        assert!(parsed.document().references().get("figure").is_some());
        let warning = parsed.warnings().iter().find(|warning| matches!(&warning.kind, WarningKind::UnresolvedReference { target } if target == "figure")).ok_or("missing generated warning")?;
        assert_eq!(warning.location.as_ref(), Some(&origin));
        let prepared = PreparedDocument::new(parsed.document(), |_| {
            panic!("missing request must use fallback")
        })?;
        let section = prepared.document().blocks().next().ok_or("kept")?;
        let BlockKind::Section(section) = section.kind() else {
            return Err("section kind".into());
        };
        let block = section.blocks().last().ok_or("output")?;
        let BlockKind::Paragraph(paragraph) = block.kind() else {
            return Err("paragraph kind".into());
        };
        let InlineKind::CrossReference(xref) =
            paragraph.content().inlines().next().ok_or("xref")?.kind()
        else {
            return Err("xref kind".into());
        };
        assert_ne!(xref.target(), "figure");
        assert!(
            prepared
                .document()
                .references()
                .get(xref.target())
                .is_none()
        );
        let label = xref.text().inlines().next().ok_or("label")?;
        assert!(matches!(label.kind(), InlineKind::Plain(text) if text.text() == "[figure]"));
    }
    Ok(())
}

#[test]
fn extensions_foreign_display_is_rejected_atomically() -> TestResult {
    let registry = support::registry();
    let mut first = registry.parse(".Title\nimage::one.svg[]", &Options::default())?;
    let image = first
        .query()
        .select(BlockContext::Image)
        .next()
        .ok_or("image")?
        .target()
        .clone();
    let captured = DocumentEditor::new(&mut first, None).capture(&image)?;
    let mut second = registry.parse("Paragraph.", &Options::default())?;
    let output = second
        .query()
        .blocks()
        .next()
        .ok_or("paragraph")?
        .target()
        .clone();
    let before = serde_json::to_value(second.parsed.document())?;
    let error = DocumentEditor::new(&mut second, None).replace_block(
        &output,
        BlockBuilder::paragraph_inlines([
            InlineSpec::Text("Before the rejected request".into()),
            InlineSpec::Sequence(vec![
                InlineSpec::LineBreak,
                InlineSpec::Sequence(vec![captured.display(DisplayIntent::Title)]),
            ]),
        ]),
    );
    assert!(error.is_err());
    assert_eq!(serde_json::to_value(second.parsed.document())?, before);
    assert!(!second.finish()?.document().has_deferred_displays());
    Ok(())
}

#[test]
fn extensions_display_distinguishes_absent_and_present_empty_titles() -> TestResult {
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (prefix, expected) in [("", "[renamed]"), (":empty:\n\n.{empty}\n", "")] {
            let mut result = support::registry().parse(
                &format!("{prefix}[#target,reftext='Generic']\nParagraph."),
                &options,
            )?;
            let target = result
                .query()
                .select(BlockContext::Paragraph)
                .next()
                .ok_or("paragraph")?
                .target()
                .clone();
            let mut editor = DocumentEditor::new(&mut result, None);
            let captured = editor.capture(&target)?;
            editor.insert_before(
                &target,
                BlockBuilder::paragraph_inlines([captured.display(DisplayIntent::Title)]),
            )?;
            editor.replace_block(
                &target,
                BlockBuilder::paragraph("Changed").with_attributes("id=renamed,title='Changed'"),
            )?;
            let parsed = result.finish()?;
            let prepared = PreparedDocument::new(parsed.document(), |_| DisplayLabel::Title)?;
            let BlockKind::Paragraph(paragraph) =
                prepared.document().blocks().next().ok_or("output")?.kind()
            else {
                return Err("paragraph kind".into());
            };
            let InlineKind::CrossReference(xref) =
                paragraph.content().inlines().next().ok_or("xref")?.kind()
            else {
                return Err("xref kind".into());
            };
            assert_eq!(xref.text().inlines().len(), 1);
            assert!(
                matches!(xref.text().inlines().next().ok_or("label")?.kind(), InlineKind::Plain(text) if text.text() == expected)
            );
        }
    }
    Ok(())
}

#[cfg(feature = "asg")]
#[test]
fn extensions_symbolic_display_requires_lowering_before_checked_asg() -> TestResult {
    fn require_send(_: impl Send) {}
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut result = support::registry().parse("[#image]\nParagraph.", &options)?;
        let target = result
            .query()
            .blocks()
            .next()
            .ok_or("image")?
            .target()
            .clone();
        let mut editor = DocumentEditor::new(&mut result, None);
        let captured = editor.capture(&target)?;
        editor.insert_before(
            &target,
            BlockBuilder::paragraph_inlines([captured.display(DisplayIntent::Title)]),
        )?;
        result.normalize()?;
        let parsed = result.finish()?;
        for profile in [crate::asg::Profile::Upstream, crate::asg::Profile::Acdc] {
            let error = crate::asg::export_document_with_profile(parsed.document(), profile)
                .expect_err("symbolic display");
            assert!(
                matches!(error, crate::asg::Error::Unsupported { path, .. } if path.ends_with("/generated_display"))
            );
            let prepared = PreparedDocument::new(parsed.document(), |_| DisplayLabel::Title)?;
            crate::asg::export_document_with_profile(prepared.document(), profile)?;
        }
        require_send(parsed);
    }
    Ok(())
}
