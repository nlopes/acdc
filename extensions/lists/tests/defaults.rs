//! Library behavior through the public query, editor, and display contracts.

use acdc_parser::{
    CompatibilityMode, Error, Options, SafeMode, WarningKind,
    ast::{BlockKind, InlineKind},
    extensions::{
        BlockBuilder, BlockContext, DisplayIntent, DocumentEditor, ExtensionParseResult, Registry,
        TreeProcessor, TreeProcessorRegistration,
    },
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn modes() -> [CompatibilityMode; 2] {
    [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft]
}

fn links(result: &ExtensionParseResult) -> Vec<Vec<(String, DisplayIntent)>> {
    result
        .query()
        .select(BlockContext::Paragraph)
        .filter_map(|view| {
            let BlockKind::Paragraph(paragraph) = view.block().kind() else {
                return None;
            };
            let links: Vec<_> = paragraph
                .content()
                .inlines()
                .filter_map(|inline| {
                    let InlineKind::CrossReference(xref) = inline.kind() else {
                        return None;
                    };
                    Some((xref.target().to_owned(), xref.generated_display()?.intent()))
                })
                .collect();
            (!links.is_empty()).then_some(links)
        })
        .collect()
}

#[test]
fn requests_before_targets_reuse_collision_safe_ids_and_preserve_authored_ids() -> TestResult {
    let source = "= Document\n\nlist-of::image[]\n\nlist-of::image[]\n\n.First\nimage::first.svg[]\n\n.Second\n[#authored,reftext='Custom reference']\nimage::second.svg[]\n\nimage::untitled.svg[]\n\n[#_acdc_3]\nReserved. See <<_acdc_3_2>>.";
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let registry = acdc_extensions_lists::registry()?;
        let first = registry.parse(source, &options)?;
        let second = registry.parse(source, &options)?;
        assert_eq!(links(&first), links(&second));
        let expected = vec![
            ("_acdc_3_3".into(), DisplayIntent::CaptionAndTitle),
            ("authored".into(), DisplayIntent::CaptionAndTitle),
        ];
        assert_eq!(links(&first), vec![expected.clone(), expected]);
        assert_eq!(
            first
                .query()
                .select(BlockContext::Image)
                .last()
                .ok_or("untitled")?
                .output_id(),
            None
        );
        let parsed = first.finish()?;
        assert_eq!(parsed.warnings().len(), 1);
        assert!(
            matches!(&parsed.warnings().first().ok_or("warning")?.kind, WarningKind::UnresolvedReference { target } if target == "_acdc_3_2")
        );
    }
    Ok(())
}

#[test]
fn supported_elements_select_semantic_contexts() -> TestResult {
    let cases = [
        ("image", ".Title\nimage::one.svg[]"),
        ("table", ".Title\n|===\n|Cell\n|==="),
        ("listing", ".Title\n[source,rust]\nCode."),
        ("literal", ".Title\n....\nLiteral.\n...."),
        ("example", ".Title\n====\nExample.\n===="),
        ("quote", ".Title\n[quote]\nQuoted."),
        ("verse", ".Title\n[verse]\nA verse."),
        ("sidebar", ".Title\n****\nSidebar.\n****"),
        ("open", ".Title\n--\nOpen.\n--"),
        ("pass", ".Title\n++++\nPassthrough.\n++++"),
        ("stem", ".Title\n[stem]\n++++\nx+y\n++++"),
        ("audio", ".Title\naudio::one.ogg[]"),
        ("video", ".Title\nvideo::one.mp4[]"),
        ("admonition", ".Title\nNOTE: Note."),
        ("paragraph", ".Title\nParagraph."),
        ("section", "== Title\n\nBody."),
        ("olist", ".Title\n. One\n. Two"),
        ("ulist", ".Title\n* One\n* Two"),
        ("dlist", ".Title\nTerm:: Definition"),
        ("colist", "----\nCode <1>\n----\n.Title\n<1> Explanation"),
    ];
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (element, target) in cases {
            let source = format!("= Document\n\nlist-of::{element}[]\n\n{target}");
            let result = acdc_extensions_lists::registry()?.parse(&source, &options)?;
            assert_eq!(
                links(&result).iter().map(Vec::len).sum::<usize>(),
                1,
                "{mode:?}: {element}"
            );
            assert!(
                result.finish()?.warnings().is_empty(),
                "{mode:?}: {element}"
            );
        }
    }
    Ok(())
}

#[test]
fn selection_reaches_nested_containers_lists_and_asciidoc_cells() -> TestResult {
    let containers = [
        "== Section\n\n.Target\nimage::one.svg[]",
        "--\n.Target\nimage::one.svg[]\n--",
        "====\n.Target\nimage::one.svg[]\n====",
        "****\n.Target\nimage::one.svg[]\n****",
        "[quote]\n____\n.Target\nimage::one.svg[]\n____",
        "[NOTE]\n====\n.Target\nimage::one.svg[]\n====",
        "* Item\n+\n.Target\nimage::one.svg[]",
        ". Item\n+\n.Target\nimage::one.svg[]",
        "Term:: Definition\n+\n.Target\nimage::one.svg[]",
        "----\nCode <1>\n----\n<1> Explanation\n+\n.Target\nimage::one.svg[]",
        "[cols=a]\n|===\n|\n.Target\nimage::one.svg[]\n|===",
    ];
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for body in containers {
            let source = format!("= Document\n\nlist-of::image[]\n\n{body}");
            let result = acdc_extensions_lists::registry()?.parse(&source, &options)?;
            assert_eq!(
                links(&result).iter().map(Vec::len).sum::<usize>(),
                1,
                "{mode:?}: {body}"
            );
            assert!(result.finish()?.warnings().is_empty(), "{mode:?}: {body}");
        }
    }
    Ok(())
}

#[test]
fn empty_lists_keep_the_section_and_unknown_elements_keep_readable_text() -> TestResult {
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let source = "= Document\n\n== Lists\n\nlist-of::image[]\n\nlist-of::unknown[]\n\n== Targets\n\nimage::untitled.svg[]";
        let result = acdc_extensions_lists::registry()?.parse(source, &options)?;
        assert_eq!(result.query().select(BlockContext::Section).count(), 2);
        let generated = links(&result);
        assert!(generated.is_empty(), "unexpected links: {generated:?}");
        assert_eq!(result.query().select(BlockContext::Paragraph).count(), 1);
        let parsed = result.finish()?;
        assert_eq!(parsed.warnings().len(), 1);
        let warning = parsed.warnings().first().ok_or("warning")?;
        assert!(
            matches!(&warning.kind, WarningKind::Other(message) if message == "unknown list-of element: unknown")
        );
        assert_eq!(
            warning
                .source_location()
                .ok_or("source")?
                .location
                .start
                .line,
            7
        );
        let unknown = parsed.document().blocks().next().ok_or("lists section")?;
        let BlockKind::Section(section) = unknown.kind() else {
            return Err("section".into());
        };
        let BlockKind::Paragraph(paragraph) =
            section.blocks().next().ok_or("readable call")?.kind()
        else {
            return Err("paragraph".into());
        };
        assert!(
            matches!(paragraph.content().inlines().next().ok_or("call text")?.kind(), InlineKind::Plain(text) if text.text() == "list-of::unknown[]")
        );
    }
    Ok(())
}

#[test]
fn unknown_element_warning_retains_the_included_source_location() -> TestResult {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for mode in modes() {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?;
        let parsed = acdc_extensions_lists::registry()?
            .parse_file(fixtures.join("include_warning.adoc"), &options)?
            .finish()?;
        assert_eq!(parsed.warnings().len(), 1);
        let location = parsed
            .warnings()
            .first()
            .ok_or("warning")?
            .source_location()
            .ok_or("location")?;
        assert_eq!(
            location.file.as_ref(),
            Some(&fixtures.join("includes/unknown.adoc"))
        );
        assert_eq!(location.location.start.line, 1);
    }
    Ok(())
}

#[test]
fn unknown_elements_preserve_the_call_arguments_and_unexpanded_target() -> TestResult {
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for call in [
            r#"list-of::not-a-context[custom="keep me"]"#,
            r#"list-of::{kind}[custom='a,b',other="<&> *literal*"]"#,
        ] {
            let result = acdc_extensions_lists::registry()?
                .parse(
                    &format!("= Document\n:kind: not-a-context\n\n{call}"),
                    &options,
                )?
                .finish()?;
            assert_eq!(result.warnings().len(), 1);
            let warning = result.warnings().first().ok_or("warning")?;
            assert!(
                matches!(&warning.kind, WarningKind::Other(message) if message == "unknown list-of element: not-a-context")
            );
            assert_eq!(
                warning
                    .source_location()
                    .ok_or("source")?
                    .location
                    .start
                    .line,
                4
            );
            let BlockKind::Paragraph(paragraph) =
                result.document().blocks().next().ok_or("output")?.kind()
            else {
                return Err("paragraph".into());
            };
            let mut content = paragraph.content().inlines();
            assert!(
                matches!(content.next().ok_or("call")?.kind(), InlineKind::Plain(text) if text.text() == call)
            );
            assert!(content.next().is_none());
        }
    }
    Ok(())
}

#[test]
fn later_requests_observe_earlier_generated_paragraphs() -> TestResult {
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let result = acdc_extensions_lists::registry()?.parse("= Document\n\n.Generated title\n[#generated]\nlist-of::image[]\n\nlist-of::paragraph[]\n\n.Target\nimage::one.svg[]", &options)?;
        let observed = links(&result);
        assert_eq!(observed.len(), 2);
        assert_eq!(
            observed.last(),
            Some(&vec![("generated".into(), DisplayIntent::Title)])
        );
        assert_eq!(result.finish()?.warnings(), []);
    }
    Ok(())
}

#[test]
fn requests_inside_asciidoc_cells_and_list_continuations_are_replaced() -> TestResult {
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let result = acdc_extensions_lists::registry()?.parse("= Document\n\n* Item\n+\nlist-of::image[]\n\n[cols=a]\n|===\n|list-of::image[]\n|===\n\n.Target\nimage::one.svg[]", &options)?;
        let observed = links(&result);
        assert_eq!(observed.len(), 2);
        assert_eq!(observed.first(), observed.last());
        assert_eq!(result.finish()?.warnings(), []);
    }
    Ok(())
}

#[test]
fn unsupported_options_fail_at_the_macro_instead_of_being_ignored() -> TestResult {
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (arguments, message) in [
            ("enhanced_rendering=true", "title formatting is automatic"),
            ("enhanced_rendering=false", "title formatting is automatic"),
            ("enhanced_rendering=0", "title formatting is automatic"),
            ("enhanced_rendering=nil", "title formatting is automatic"),
            ("enhanced_rendering=\"\"", "title formatting is automatic"),
            ("enhanced_rendering=", "title formatting is automatic"),
            (",true", "one positional argument"),
            ("true,false", "one positional argument"),
        ] {
            let source = format!("= Document\n\nlist-of::image[{arguments}]");
            let error = acdc_extensions_lists::registry()?
                .parse(&source, &options)
                .err()
                .ok_or("expected option error")?;
            assert!(
                matches!(error, Error::Extension(origin, detail) if origin.location.start.line == 3 && detail.contains(message)),
                "{mode:?}: {arguments}"
            );
        }
    }
    Ok(())
}

struct Rename;

impl TreeProcessor for Rename {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let targets: Vec<_> = editor
            .query()
            .select(BlockContext::Image)
            .map(|view| view.target().clone())
            .collect();
        for target in targets {
            editor.replace_block(
                &target,
                BlockBuilder::image("changed.svg")
                    .with_attributes("id=renamed,title='Later title',caption='Updated '"),
            )?;
        }
        Ok(())
    }
}

#[test]
fn later_processors_rename_targets_without_refreshing_captured_titles_or_membership() -> TestResult
{
    for mode in modes() {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut builder = Registry::builder();
        builder.extend(&acdc_extensions_lists::registry()?)?;
        builder.register_tree_processor(TreeProcessorRegistration::new("rename", || Rename))?;
        let result = builder.build().parse("= Document\n:label: Early\n\nlist-of::image[]\n\n.Original *{label}*\nimage::one.svg[]\n\n:label: Late", &options)?;
        let parsed = result.finish()?;
        let output = parsed.document().blocks().next().ok_or("output")?;
        let BlockKind::Paragraph(paragraph) = output.kind() else {
            return Err("paragraph".into());
        };
        let InlineKind::CrossReference(xref) =
            paragraph.content().inlines().next().ok_or("link")?.kind()
        else {
            return Err("xref".into());
        };
        let display = xref.generated_display().ok_or("display")?;
        assert_eq!(display.target(), "renamed");
        assert_eq!(
            display.title_source().ok_or("title")?.raw_source(),
            "Original *{label}*"
        );
        assert_eq!(
            display
                .title_source()
                .ok_or("context")?
                .attributes()
                .get("label")
                .and_then(acdc_parser::DocumentAttributeValue::as_str),
            Some("Early")
        );
        assert!(
            display
                .title()
                .ok_or("captured title")?
                .inlines()
                .any(|inline| matches!(inline.kind(), InlineKind::Bold(_)))
        );
        assert_eq!(parsed.warnings(), []);
    }
    Ok(())
}
