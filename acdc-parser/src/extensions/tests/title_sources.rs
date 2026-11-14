use acdc_parser::{
    CompatibilityMode, DocumentAttributeValue, Options, SafeMode, Substitution,
    extensions::{BlockBuilder, BlockContext, BlockView, DocumentEditor, Registry},
};

use super::{TestResult, support};

#[test]
fn extensions_title_sources_retain_input_and_pre_substitution_attributes() -> TestResult {
    let source = "= Document\n:label: Early\n:empty:\n\n== Section *{label}*\n\n.{counter:sequence} *{label}*\n[subs=none]\nimage::one.svg[]\n\n:label: Late\n\n[title='{label} *attribute*']\nParagraph.\n\n.{empty}\nimage::empty.svg[]\n\n[discrete]\n=== Discrete {label}";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for registry in [Registry::default(), support::registry()] {
            let parsed = registry.parse(source, &options)?;
            let query = parsed.query();
            let titles = query
                .blocks()
                .filter_map(|view| view.title_source())
                .collect::<Vec<_>>();
            assert_eq!(
                titles
                    .iter()
                    .map(|title| title.raw_source())
                    .collect::<Vec<_>>(),
                [
                    "Section *{label}*",
                    "{counter:sequence} *{label}*",
                    "{label} *attribute*",
                    "{empty}",
                    "Discrete {label}",
                ]
            );
            for (title, expected) in titles
                .iter()
                .zip(["Early", "Early", "Late", "Late", "Late"])
            {
                assert_eq!(
                    title
                        .attributes()
                        .get("label")
                        .and_then(DocumentAttributeValue::as_str),
                    Some(expected)
                );
                assert_ne!(title.substitutions(), []);
                let location = &title.source_location().location;
                let end = location.absolute_start + title.raw_source().len();
                assert_eq!(
                    source.get(location.absolute_start..end),
                    Some(title.raw_source())
                );
            }
            // Capturing the source must precede the title's counter side effect.
            assert!(
                titles
                    .get(1)
                    .ok_or("image title")?
                    .attributes()
                    .get("sequence")
                    .is_none()
            );
            let first = query.select(BlockContext::Image).next().ok_or("image")?;
            let title = first.title().ok_or("parsed title")?;
            assert!(
                title
                    .inlines()
                    .any(|inline| matches!(inline.kind(), crate::ast::InlineKind::Bold(_)))
            );
            assert!(
                first
                    .title_source()
                    .ok_or("source")?
                    .substitutions()
                    .contains(&Substitution::Quotes)
            );
            let empty = query
                .select(BlockContext::Image)
                .nth(1)
                .ok_or("empty image")?;
            assert!(empty.title().is_some_and(crate::ast::ContentRef::is_empty));
            assert_eq!(
                empty
                    .title_source()
                    .map(crate::extensions::TitleSource::raw_source),
                Some("{empty}")
            );
        }
    }
    Ok(())
}

#[test]
fn extensions_title_sources_follow_committed_macros_includes_and_cell_scopes() -> TestResult {
    let directory = tempfile::tempdir()?;
    let included = directory.path().join("title.adoc");
    std::fs::write(&included, ".Included *{label}*\nimage::included.svg[]\n")?;
    let source = "= Document\n:label: Header\n\nconfigure::label[Outer]\n\ninclude::title.adoc[]\n\n[cols=a]\n|===\n|configure::cell-label[Cell]\n\n.Cell {cell-label}\nimage::cell.svg[]\n|===\n\n.Outside {label}\nimage::outside.svg[]";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .with_base_dir(directory.path())
            .build()?;
        let parsed = support::registry().parse(source, &options)?;
        let query = parsed.query();
        let images = query.select(BlockContext::Image).collect::<Vec<_>>();
        assert_eq!(images.len(), 3);
        for image in &images {
            assert_eq!(
                image
                    .title_source()
                    .ok_or("title source")?
                    .attributes()
                    .get("label")
                    .and_then(DocumentAttributeValue::as_str),
                Some("Outer")
            );
        }
        assert_eq!(
            images
                .get(1)
                .and_then(BlockView::title_source)
                .ok_or("cell")?
                .attributes()
                .get("cell-label")
                .and_then(DocumentAttributeValue::as_str),
            Some("Cell")
        );
        assert!(
            !images
                .get(2)
                .and_then(BlockView::title_source)
                .ok_or("outside")?
                .attributes()
                .contains("cell-label")
        );
        let title = images
            .first()
            .and_then(BlockView::title_source)
            .ok_or("included title")?;
        assert_eq!(title.raw_source(), "Included *{label}*");
        assert_eq!(title.source_location().file.as_ref(), Some(&included));
        assert_eq!(title.source_location().location.start.line, 1);
    }
    Ok(())
}

#[test]
fn extensions_attribute_title_sources_record_their_own_substitution_policy() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = Registry::default().parse(
            "= Document\n:label: Early\n\n[title=\"{label} *double*\"]\nParagraph.\n\n[title='{label} *single*']\nParagraph.\n\n[title=]\nParagraph.",
            &options,
        )?;
        let query = parsed.query();
        let sources = query
            .blocks()
            .filter_map(|view| view.title_source())
            .collect::<Vec<_>>();
        assert_eq!(sources.len(), 3);
        assert_eq!(
            sources.first().ok_or("double")?.raw_source(),
            "{label} *double*"
        );
        let double = sources.first().ok_or("double")?.substitutions();
        if mode == CompatibilityMode::Draft {
            assert_eq!(double, [Substitution::SpecialChars]);
        } else {
            assert!(double.contains(&Substitution::Quotes));
        }
        let single = sources.get(1).ok_or("single")?.substitutions();
        assert!(single.contains(&Substitution::Quotes));
        assert_eq!(
            single.contains(&Substitution::Attributes),
            mode == CompatibilityMode::Asciidoctor
        );
        assert_eq!(sources.get(2).ok_or("empty")?.raw_source(), "");
    }
    Ok(())
}

#[test]
fn extensions_setext_titles_retain_source_and_ordinary_parsing_stays_equivalent() -> TestResult {
    let source =
        "= Document\n:label: Early\n\nSection {label}\n---------------\n\n.A title\nParagraph.";
    let native = acdc_parser::parse(source, &Options::default())?;
    let extended = Registry::default().parse(source, &Options::default())?;
    let query = extended.query();
    let heading = query
        .select(BlockContext::Section)
        .next()
        .ok_or("heading")?;
    assert_eq!(
        heading
            .title_source()
            .map(crate::extensions::TitleSource::raw_source),
        Some("Section {label}")
    );
    assert_eq!(
        native.document_storage(),
        extended.parsed.document_storage()
    );
    assert!(
        native
            .document_storage()
            .blocks
            .iter()
            .filter_map(crate::Block::title)
            .all(|title| title.source().is_none())
    );
    Ok(())
}

#[test]
fn extensions_title_sources_survive_placeholder_replacement_and_preserve_caption_context()
-> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let source = "= Document\n:label: Early\n:example-caption: Before\n\n.Pending {label}\n[example]\ndefer::[]\n\n.Original {label}\n====\n:example-caption: Inside\n\nBody.\n====\n\n:label: Late";
        let mut result = support::registry().parse(source, &options)?;
        let target = result
            .deferred_blocks()
            .first()
            .ok_or("request")?
            .target()
            .clone();
        let id = result.ensure_id(&target)?.to_owned();
        let mut editor = DocumentEditor::new(&mut result);
        editor.replace_placeholder(&target, BlockBuilder::paragraph("Replaced"))?;
        let query = editor.query();
        let replacement = query.get(&target).ok_or("replacement")?;
        assert_eq!(replacement.context(), BlockContext::Example);
        assert_eq!(replacement.output_id(), Some(id.as_str()));
        let retained = replacement.title_source().ok_or("retained")?;
        assert_eq!(retained.raw_source(), "Pending {label}");
        assert_eq!(
            retained
                .attributes()
                .get("label")
                .and_then(DocumentAttributeValue::as_str),
            Some("Early")
        );
        assert_eq!(retained.source_location().location.start.line, 5);
        let original = query
            .select(BlockContext::Example)
            .nth(1)
            .ok_or("original")?;
        assert_eq!(
            original
                .title_source()
                .ok_or("source")?
                .attributes()
                .get("example-caption")
                .and_then(DocumentAttributeValue::as_str),
            Some("Before")
        );
        assert!(matches!(
            original.caption(),
            Some(crate::ast::CaptionRef::Numbered {
                label: "Inside",
                ..
            })
        ));
    }
    Ok(())
}
