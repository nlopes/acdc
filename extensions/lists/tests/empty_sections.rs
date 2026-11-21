//! Empty-list policy and the checked removal of its containing section.

use acdc_parser::{
    AttributeValue, CompatibilityMode, ElementAttributes, Options, WarningKind,
    ast::{BlockKind, CaptionRef, InlineKind},
    extensions::{
        BlockContext, BlockMacro, BlockMacroOutput, MacroExecutionContext, MacroInvocation,
        Registry,
    },
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn modes() -> [CompatibilityMode; 2] {
    [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft]
}

#[test]
fn hide_empty_section_follows_reference_string_truthiness_and_first_slot() -> TestResult {
    let cases = [
        ("", false),
        ("true", true),
        ("false", true),
        ("0", true),
        ("\"\"", true),
        ("hide_empty_section", true),
        ("hide_empty_section=true", true),
        ("hide_empty_section=false", true),
        ("hide_empty_section=0", true),
        ("hide_empty_section=\"\"", true),
        ("hide_empty_section=", true),
        ("hide_empty_section=nil", true),
        ("hide_empty_section!", true),
        (",", false),
        ("custom=x,", false),
        ("hide_empty_section=false,", true),
        ("\"\",hide_empty_section=false", true),
    ];
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (arguments, hidden) in cases {
            let source = format!(
                "= Document\n\n[#lists]\n== Lists\n\nlist-of::image[{arguments}]\n\n[#keep]\n== Keep\n\nBody."
            );
            let result = acdc_extensions_lists::registry()?.parse(&source, &options)?;
            let sections: Vec<_> = result
                .query()
                .select(BlockContext::Section)
                .filter_map(|view| view.output_id())
                .collect();
            assert_eq!(
                sections,
                if hidden {
                    vec!["keep"]
                } else {
                    vec!["lists", "keep"]
                },
                "{mode:?}: {arguments}"
            );
            assert_eq!(result.finish()?.warnings(), []);
        }
    }
    Ok(())
}

#[test]
fn matching_untitled_nodes_keep_the_section_even_when_no_entries_are_generated() -> TestResult {
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let result = acdc_extensions_lists::registry()?.parse("= Document\n\n[#lists]\n== Lists\n\nlist-of::image[hide_empty_section=true]\n\n== Targets\n\nimage::untitled.svg[]", &options)?;
        assert_eq!(result.query().select(BlockContext::Section).count(), 2);
        assert_eq!(result.query().select(BlockContext::Paragraph).count(), 0);
        assert_eq!(result.finish()?.warnings(), []);
    }
    Ok(())
}

#[test]
fn removal_cancels_pending_descendants_and_later_requests_observe_removed_targets() -> TestResult {
    let source = "= Document\n\n[#gone]\n== Gone\n\nlist-of::image[hide_empty_section=true]\n\nlist-of::unknown[]\n\n=== Child\n\n.Title\nKept only until its section disappears.\n\nlist-of::paragraph[]\n\n[#keep]\n== Keep\n\nlist-of::paragraph[]";
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let result = acdc_extensions_lists::registry()?.parse(source, &options)?;
        assert_eq!(result.query().blocks().count(), 1);
        assert_eq!(
            result
                .query()
                .blocks()
                .next()
                .ok_or("surviving section")?
                .output_id(),
            Some("keep")
        );
        assert_eq!(result.finish()?.warnings(), []);
    }
    Ok(())
}

#[test]
fn hiding_inside_nested_containers_removes_the_nearest_section_only() -> TestResult {
    let containers = [
        "--\nlist-of::image[true]\n--",
        "* Item\n+\nlist-of::image[true]",
        "[NOTE]\n====\nlist-of::image[true]\n====",
    ];
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for container in containers {
            let source = format!(
                "= Document\n\n[#parent]\n== Parent\n\nBody.\n\n[#child]\n=== Child\n\n{container}\n\n[#keep]\n== Keep\n\nBody."
            );
            let result = acdc_extensions_lists::registry()?.parse(&source, &options)?;
            let sections: Vec<_> = result
                .query()
                .select(BlockContext::Section)
                .filter_map(|view| view.output_id())
                .collect();
            assert_eq!(sections, ["parent", "keep"], "{mode:?}: {container}");
            assert_eq!(result.finish()?.warnings(), []);
        }
    }
    Ok(())
}

#[test]
fn calls_without_a_section_and_in_cells_preserve_the_surrounding_document() -> TestResult {
    let cases = [
        ("list-of::image[true]\n\nKeep.", 0, 1),
        ("--\nlist-of::image[true]\n--\n\nKeep.", 0, 1),
        (
            "[#outer]\n== Outer\n\n[cols=a]\n|===\n|list-of::image[true]\n|===\n\nKeep.",
            1,
            1,
        ),
    ];
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (body, sections, paragraphs) in cases {
            let result = acdc_extensions_lists::registry()?
                .parse(&format!("= Document\n\n{body}"), &options)?;
            assert_eq!(
                result.query().select(BlockContext::Section).count(),
                sections,
                "{body}"
            );
            assert_eq!(
                result.query().select(BlockContext::Paragraph).count(),
                paragraphs,
                "{body}"
            );
            assert_eq!(result.finish()?.warnings(), []);
        }
    }
    Ok(())
}

#[test]
fn actual_false_and_nil_attribute_defaults_are_disabled() -> TestResult {
    for value in [AttributeValue::Bool(false), AttributeValue::None] {
        let mut defaults = ElementAttributes::default();
        defaults.set("hide_empty_section".into(), value);
        let mut builder = Registry::builder();
        builder.extend(&acdc_extensions_lists::registry()?)?;
        builder.replace(
            BlockMacro::new("list-of", || {
                |_: &MacroInvocation<'_>, _: &mut MacroExecutionContext| {
                    Ok(BlockMacroOutput::Deferred)
                }
            })
            .with_defaults(defaults),
        )?;
        let result = builder.build().parse(
            "= Document\n\n== Lists\n\nlist-of::image[]",
            &Options::default(),
        )?;
        assert_eq!(result.query().select(BlockContext::Section).count(), 1);
        assert_eq!(result.finish()?.warnings(), []);
    }
    Ok(())
}

#[test]
fn removed_section_repairs_catalogs_footnotes_and_preserves_surviving_ordinals() -> TestResult {
    let source = "= Document\n:sectnums:\n\n[#gone]\n== Gone\n\nRemoved footnote:gone[Removed definition].\n\n.Removed figure\n[#gone-figure]\nimage::gone.svg[]\n\nlist-of::video[true]\n\n[#keep]\n== Keep\n\nSurviving footnote:kept[Surviving definition].\n\n.Surviving figure\n[#kept-figure]\nimage::kept.svg[]\n\nlist-of::image[]";
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let result = acdc_extensions_lists::registry()?.parse(source, &options)?;
        let image = result
            .query()
            .select(BlockContext::Image)
            .next()
            .ok_or("image")?;
        assert!(
            matches!(image.caption(), Some(CaptionRef::Numbered {number: Some(number), ..}) if number.get() == 2)
        );
        let parsed = result.finish()?;
        let document = parsed.document();
        assert!(document.references().get("gone").is_none());
        assert!(document.references().get("gone-figure").is_none());
        assert!(document.references().get("kept-figure").is_some());
        let toc: Vec<_> = document
            .toc_entries()
            .map(|entry| (entry.id(), entry.number()))
            .collect();
        assert_eq!(toc, [("keep", Some("2"))]);
        assert_eq!(document.footnotes().count(), 1);
        assert_eq!(document.footnotes().next().ok_or("footnote")?.number(), 1);
        assert_eq!(parsed.warnings(), []);
    }
    Ok(())
}

#[test]
fn earlier_lists_keep_captured_titles_when_a_later_request_removes_the_target() -> TestResult {
    let source = "= Document\n:label: Original\n\nlist-of::paragraph[]\n\n== Gone\n\n.Frozen *{label}*\n[#target]\nSelected paragraph.\n\nlist-of::image[true]\n\n== Keep\n\n:label: Later\n";
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = acdc_extensions_lists::registry()?
            .parse(source, &options)?
            .finish()?;
        assert_eq!(parsed.warnings().len(), 1);
        assert!(
            matches!(&parsed.warnings().first().ok_or("warning")?.kind, WarningKind::UnresolvedReference {target} if target == "target")
        );
        let BlockKind::Paragraph(paragraph) =
            parsed.document().blocks().next().ok_or("list")?.kind()
        else {
            return Err("paragraph".into());
        };
        let InlineKind::CrossReference(xref) =
            paragraph.content().inlines().next().ok_or("link")?.kind()
        else {
            return Err("xref".into());
        };
        let display = xref.generated_display().ok_or("captured display")?;
        assert!(display.is_missing());
        assert_eq!(
            display.title_source().ok_or("source")?.raw_source(),
            "Frozen *{label}*"
        );
        assert_eq!(
            display
                .title_source()
                .ok_or("attributes")?
                .attributes()
                .get("label")
                .and_then(acdc_parser::DocumentAttributeValue::as_str),
            Some("Original")
        );
        assert!(
            display
                .title()
                .ok_or("title")?
                .inlines()
                .any(|inline| matches!(inline.kind(), InlineKind::Bold(_)))
        );
    }
    Ok(())
}
