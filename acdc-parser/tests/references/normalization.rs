//! Semantic reference extensions share the parser's identity and diagnostic rules.

use acdc_parser::{
    CompatibilityMode, Options, SafeMode, SourceLocation, WarningKind,
    ast::{BlockKind, BlockRef, InlineKind},
    extensions::{RegistrationError, Registry},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn missing_targets(warnings: &[acdc_parser::Warning]) -> Vec<&str> {
    warnings
        .iter()
        .filter_map(|warning| {
            let WarningKind::UnresolvedReference { target } = &warning.kind else {
                return None;
            };
            Some(target.as_str())
        })
        .collect()
}

#[test]
fn normalized_targets_use_normal_resolution_and_preserve_authored_labels() -> TestResult {
    let mut builder = Registry::builder();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    builder.register_reference_normalizer("aliases", move || {
        let counter = Arc::clone(&counter);
        move |target: &str, _: &SourceLocation| {
            counter.fetch_add(1, Ordering::Relaxed);
            Ok(Some(
                match target {
                    "old#intro" => "step-two",
                    "elsewhere" => "other.adoc#intro",
                    "gone" => "missing",
                    other => return Err(format!("unexpected target {other}")),
                }
                .to_owned(),
            ))
        }
    })?;
    builder.register_reference_normalizer("anchors", || {
        |target: &str, _: &SourceLocation| Ok((target == "step-two").then(|| "intro".to_owned()))
    })?;
    let registry = builder.build();
    let source = r"See <<old#++intro++,Explicit>> and xref:old#intro[].
xref:elsewhere[] <<gone>> xref:https://example.org/pass:[a]#b[Remote] <<//host/a#b,Remote>>.
\xref:escaped[] and \<<escaped>>.

[[intro]]
== Introduction";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let initial = calls.load(Ordering::Relaxed);
        let ordinary = Registry::default().parse(source, &options)?.finish()?;
        let Some(BlockKind::Paragraph(p)) = ordinary.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("expected introductory paragraph".into());
        };
        assert!(p.content().inlines().any(|n| matches!(n.kind(), InlineKind::CrossReference(x) if x.target()=="old#intro" && !x.target_is_local())));
        let mut result = registry.parse(source, &options)?;
        result.normalize()?;
        result.normalize()?;
        let parsed = result.finish()?;
        assert_eq!(calls.load(Ordering::Relaxed) - initial, 4);
        let Some(BlockKind::Paragraph(p)) = parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("expected introductory paragraph".into());
        };
        let targets: Vec<_> = p
            .content()
            .inlines()
            .filter_map(|node| {
                let InlineKind::CrossReference(xref) = node.kind() else {
                    return None;
                };
                let label = xref.text().inlines().next().and_then(|node| {
                    let InlineKind::Plain(text) = node.kind() else {
                        return None;
                    };
                    Some(text.text())
                });
                Some((xref.target(), xref.target_is_local(), label))
            })
            .collect();
        assert_eq!(
            targets,
            [
                ("intro", true, Some("Explicit")),
                ("intro", true, None),
                ("other.adoc#intro", false, None),
                ("missing", true, None),
                ("https://example.org/a#b", false, Some("Remote")),
                ("//host/a#b", false, Some("Remote"))
            ]
        );
        assert_eq!(missing_targets(parsed.warnings()), ["missing"]);
    }
    Ok(())
}

#[test]
fn normalizer_failure_points_to_the_original_included_reference() -> TestResult {
    let temp = tempfile::tempdir()?;
    let main = temp.path().join("main.adoc");
    let child = temp.path().join("chapter.adoc");
    std::fs::write(&main, "= Test\n\ninclude::chapter.adoc[]\n")?;
    std::fs::write(&child, "Text.\n\nSee xref:old#pass:[bad][Label].\n")?;
    let mut builder = Registry::builder();
    builder.register_reference_normalizer("reject", || {
        |target: &str, _: &SourceLocation| Err(format!("rejected {target}"))
    })?;
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .build()?;
    let Err(acdc_parser::Error::Extension(source, message)) =
        builder.build().parse_file(&main, &options)
    else {
        return Err("expected a located extension error".into());
    };
    assert_eq!(message, "rejected old#bad");
    assert_eq!(source.file.as_deref(), Some(child.as_path()));
    assert_eq!(
        (source.location.start.line, source.location.start.column),
        (3, 5)
    );
    Ok(())
}

#[test]
fn registration_is_atomic_and_scoped_to_each_parse() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_reference_normalizer("ordered", || {
        let mut ordinal = 0;
        move |_: &str, _: &SourceLocation| {
            ordinal += 1;
            Ok(Some(format!("part.adoc#entry-{ordinal}")))
        }
    })?;
    let registry = builder.build();
    assert!(!registry.is_empty());
    let mut selected = Registry::builder();
    selected.extend(&registry)?;
    assert!(
        matches!(selected.extend(&registry),Err(RegistrationError::DuplicateReferenceNormalizer(name)) if name=="ordered")
    );
    let options = Options::builder().with_force_local_file_xrefs().build()?;
    let selected = selected.build();
    // A section's title also appears in the TOC and reference catalog.
    for _ in 0..2 {
        let parsed = selected
            .parse("== See <<one>>\n\nxref:two[]", &options)?
            .finish()?;
        assert_eq!(missing_targets(parsed.warnings()), ["entry-1", "entry-2"]);
        let Some(BlockKind::Section(section)) =
            parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("expected a section".into());
        };
        assert_eq!(
            section.id().ok_or("missing section ID")?.as_str(),
            "_see_entry_1"
        );
        let toc = parsed
            .document()
            .toc_entries()
            .next()
            .ok_or("missing TOC entry")?;
        assert_eq!(toc.title(), section.title());
        assert!(
            section
                .title()
                .ok_or("missing section title")?
                .inlines()
                .any(|n| matches!(n.kind(),InlineKind::CrossReference(x) if x.target()=="entry-1"))
        );
    }
    Ok(())
}
