use acdc_parser::{
    CompatibilityMode, InlineMacro, InlineNode, Options, SafeMode, WarningKind,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef},
    parse, parse_file, parse_inline,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn standalone_ui_macros_follow_mode_and_caller_activation() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for activation in [None, Some(false), Some(true)] {
            let mut builder = Options::builder().with_compatibility_mode(mode);
            if let Some(value) = activation {
                builder = builder.with_attribute("experimental", value);
            }
            let parsed =
                parse_inline("kbd:[Ctrl+C] btn:[Save] menu:File[Open]", &builder.build()?)?;
            let parsed_inlines =
                acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
            let count = parsed_inlines
                .as_slice()
                .iter()
                .filter(|node| {
                    matches!(
                        node,
                        InlineNode::Macro(
                            InlineMacro::KeyboardShortcut(_)
                                | InlineMacro::Button(_)
                                | InlineMacro::MenuPath(_)
                        )
                    )
                })
                .count();
            assert_eq!(
                count,
                if mode == CompatibilityMode::Draft || activation == Some(true) {
                    3
                } else {
                    0
                }
            );
            assert_eq!(parsed.warnings(), []);
        }
    }
    Ok(())
}

#[test]
fn draft_preserves_legacy_control_and_warns_without_recovery() -> Result<(), Error> {
    let source = "= Guide\n:compat-mode: authored value\n\nBody.\n\n:compat-mode!:\n\n:compat-mode:\n\nEnd.\n";
    let options = Options::builder()
        .with_compatibility_mode(CompatibilityMode::Draft)
        .with_strict()
        .build()?;
    let parsed = parse(source, &options)?;
    assert_eq!(
        parsed
            .document()
            .attributes()
            .get("compat-mode")
            .and_then(|value| value.text()),
        Some("authored value")
    );
    let lines = parsed
        .warnings()
        .iter()
        .filter(|warning| matches!(warning.kind, WarningKind::UnsupportedCompatMode))
        .filter_map(|warning| {
            warning
                .source_location()
                .map(|source| source.location.start.line)
        })
        .collect::<Vec<_>>();
    assert_eq!(lines, [2, 8]);
    assert!(parsed.source_recovery().is_none());
    Ok(())
}

#[test]
fn caller_legacy_control_warns_once_and_document_locks_still_apply() -> Result<(), Error> {
    for enabled in [false, true] {
        let options = Options::builder()
            .with_compatibility_mode(CompatibilityMode::Draft)
            .with_attribute("compat-mode", enabled)
            .build()?;
        let parsed = parse(
            "= Guide\n:compat-mode:\n\n:compat-mode!:\n\nBody.\n",
            &options,
        )?;
        let warnings = parsed
            .warnings()
            .iter()
            .filter(|warning| matches!(warning.kind, WarningKind::UnsupportedCompatMode))
            .collect::<Vec<_>>();
        assert_eq!(warnings.len(), usize::from(enabled));
        assert_eq!(
            parsed.document().attributes().contains("compat-mode"),
            enabled
        );
        if let Some(warning) = warnings.first() {
            assert!(warning.source_location().is_none());
        }
        let inline = parse_inline("link:https://example.org[Label,title=Tip]", &options)?;
        let inline_inlines = acdc_parser::ContentAttribute::from(inline.content()).into_inlines();
        assert_eq!(
            inline
                .warnings()
                .iter()
                .filter(|warning| matches!(warning.kind, WarningKind::UnsupportedCompatMode))
                .count(),
            usize::from(enabled)
        );
        let Some(InlineNode::Macro(InlineMacro::Link(link))) = inline_inlines.as_slice().first()
        else {
            return Err("expected hyperlink".into());
        };
        assert!(link.clone().take_tooltip().is_some());
    }
    Ok(())
}

#[test]
fn included_control_warning_keeps_original_source_location() -> Result<(), Error> {
    let root = tempfile::tempdir()?;
    let child = root.path().join("child.adoc");
    let main = root.path().join("main.adoc");
    std::fs::write(&child, "Excluded.\n:compat-mode: λ\n\nIncluded body.\n")?;
    std::fs::write(&main, "= Main\n\ninclude::child.adoc[lines=2..4]\n")?;
    let options = Options::builder()
        .with_compatibility_mode(CompatibilityMode::Draft)
        .with_safe_mode(SafeMode::Unsafe)
        .build()?;
    let parsed = parse_file(&main, &options)?;
    let warning = parsed
        .warnings()
        .iter()
        .find(|warning| matches!(warning.kind, WarningKind::UnsupportedCompatMode))
        .ok_or("missing control warning")?;
    let location = warning.source_location().ok_or("missing source location")?;
    assert_eq!(location.file.as_ref(), Some(&child));
    assert_eq!(location.location.start.line, 2);
    assert_eq!(location.location.start.column, 1);
    Ok(())
}

#[test]
fn uri_fallback_roles_follow_the_fixed_mode() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?;
        let parsed = parse(
            "= Guide\n:compat-mode:\n\ninclude::https://example.invalid/blocked.adoc[]\n",
            &options,
        )?;
        let Some(BlockKind::Paragraph(paragraph)) =
            parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("expected fallback paragraph".into());
        };
        let Some(InlineKind::Link(link)) =
            paragraph.content().inlines().next().map(InlineRef::kind)
        else {
            return Err("expected fallback link".into());
        };
        assert_eq!(
            link.roles().iter().any(|role| role == "include"),
            mode == CompatibilityMode::Draft
        );
    }
    Ok(())
}
