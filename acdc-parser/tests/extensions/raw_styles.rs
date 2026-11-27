use acdc_parser::{
    CompatibilityMode, ElementAttributes, Error, Options, SafeMode, Substitution, WarningKind,
    ast::{BlockKind, BlockRef, InlineKind},
    extensions::{
        BlockBuilder, BlockContext, BlockStyle, BlockStyleOutput, RawBlockContext, RawBlockInput,
        RegistrationError, Registry, TreeProcessorRegistration,
    },
};

use super::{ObserveLogo, TestResult};

fn sketch() -> BlockStyle {
    let mut defaults = ElementAttributes::default();
    defaults.set("format".into(), "png".into());
    defaults.set("theme".into(), "light".into());
    BlockStyle::raw("sketch", [RawBlockContext::Listing], || {
        let mut calls = 0;
        move |block: RawBlockInput<'_>| {
            calls += 1;
            assert_eq!(calls, 1);
            assert_eq!(block.context, RawBlockContext::Listing);
            assert_eq!(block.body, "  {label} *raw* <1>\n\nfootnote:[from body]\n");
            assert_eq!(block.metadata.style, Some("sketch"));
            assert_eq!(
                block.title.map(super::TitleSource::raw_source),
                Some("{label} *sketch*")
            );
            assert_eq!(
                block
                    .document_attributes
                    .get("label")
                    .and_then(acdc_parser::DocumentAttributeValue::text),
                Some("Early")
            );
            assert_eq!(block.location.location.start.line, 1);
            assert_eq!(block.body_location.file, block.location.file);
            assert_eq!(block.body_location.location.start.line, 4);
            assert!(block.substitutions().contains(&Substitution::Macros));
            for (name, expected) in [("target", "brand"), ("format", "svg"), ("theme", "light")] {
                assert_eq!(
                    block
                        .metadata
                        .attributes
                        .get(name)
                        .map(ToString::to_string)
                        .as_deref(),
                    Some(expected)
                );
            }
            Ok(Some(BlockBuilder::image("brand.svg")))
        }
    })
    .with_positional_attributes(["target", "format"])
    .with_defaults(defaults)
}

#[test]
fn style_builds_an_image_from_raw_included_source_before_tree_processing() -> TestResult {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("sketch.adoc");
    std::fs::write(
        &file,
        ".{label} *sketch*\n[sketch#brand.wide,brand,pdf,format=svg,subs=normal]\n----\n  {label} *raw* <1>\n\nfootnote:[from body]\n----",
    )?;
    let mut package = Registry::builder();
    package.register_block_style(sketch())?;
    let mut builder = Registry::builder();
    builder.extend(&package.build())?;
    builder.register_tree_processor(TreeProcessorRegistration::new("observe", || ObserveLogo))?;
    let registry = builder.build();
    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let options = Options::builder()
            .with_base_dir(dir.path())
            .with_safe_mode(SafeMode::Safe)
            .with_compatibility_mode(mode)
            .build()?;
        let result = registry.parse(
            ":label: Early\n\n====\ninclude::sketch.adoc[]\n====\n\n:label: Late",
            &options,
        )?;
        let view = result
            .query()
            .select(BlockContext::Image)
            .next()
            .ok_or("missing image")?;
        assert_eq!(view.output_id(), Some("brand"));
        assert_eq!(view.source_location().file.as_deref(), Some(file.as_path()));
        assert_eq!(view.source_location().location.end.line, 7);
        let BlockKind::Image(image) = view.block().kind() else {
            return Err("missing image".into());
        };
        assert!(image.roles().contains("wide"));
        assert_eq!(image.source().to_string(), "brand.svg");
        assert_eq!(
            view.title_source().map(super::TitleSource::raw_source),
            Some("{label} *sketch*")
        );
        let parsed = result.finish()?;
        assert_eq!(parsed.document().footnotes().count(), 0);
        assert_eq!(parsed.warnings(), []);
    }
    Ok(())
}

#[test]
fn style_warnings_keep_the_included_invocation_location_and_allow_fallback() -> TestResult {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("styles.adoc");
    std::fs::write(
        &file,
        "[panel]\n----\n.Fallback\n[sketch]\n....\n{x} *raw*\n....\n----",
    )?;
    let mut builder = Registry::builder();
    builder.register_block_style(BlockStyle::compound(
        "panel",
        [RawBlockContext::Listing],
        || {
            |mut input: RawBlockInput<'_>| {
                input.warn("parsing children");
                Ok(BlockStyleOutput::ParseChildren)
            }
        },
    ))?;
    builder.register_block_style(BlockStyle::raw(
        "sketch",
        [RawBlockContext::Literal],
        || {
            |mut input: RawBlockInput<'_>| {
                input.warn("using source");
                Ok(Some(BlockBuilder::literal(input.body)))
            }
        },
    ))?;
    let registry = builder.build();
    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let options = Options::builder()
            .with_base_dir(dir.path())
            .with_safe_mode(SafeMode::Safe)
            .with_compatibility_mode(mode)
            .build()?;
        let result = registry.parse("Before.\n\ninclude::styles.adoc[]\n\nAfter.", &options)?;
        assert_eq!(result.warnings().len(), 2);
        let parsed = result.finish()?;
        let warnings = parsed.warnings();
        assert_eq!(warnings.len(), 2);
        for (warning, (message, line)) in warnings
            .iter()
            .zip([("parsing children", 1), ("using source", 3)])
        {
            assert!(matches!(&warning.kind, WarningKind::Other(text) if text == message));
            let location = warning
                .source_location()
                .ok_or("missing warning location")?;
            assert_eq!(location.file.as_deref(), Some(file.as_path()));
            assert_eq!(location.location.start.line, line);
        }
        let mut blocks = parsed.document().blocks().map(BlockRef::kind);
        assert!(matches!(blocks.next(), Some(BlockKind::Paragraph(_))));
        let Some(BlockKind::DelimitedOpen(panel)) = blocks.next() else {
            return Err("missing panel".into());
        };
        assert!(matches!(panel.blocks().next().map(BlockRef::kind),
            Some(BlockKind::DelimitedLiteral(literal)) if literal.source_text() == Some("{x} *raw*\n")));
        assert!(matches!(blocks.next(), Some(BlockKind::Paragraph(_))));
    }
    Ok(())
}

#[test]
fn image_styles_resolve_authored_alt_before_builder_defaults() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_block_style(BlockStyle::raw(
        "picture",
        [RawBlockContext::Literal],
        || {
            |input: RawBlockInput<'_>| {
                let image = BlockBuilder::image("clock.svg");
                Ok(Some(match input.body.trim() {
                    "default" => image.with_default_attributes("alt=Default"),
                    "positional" => image.with_default_attributes("Default"),
                    "override" => image.with_attributes("alt=Override"),
                    "position-override" => image.with_attributes("Override"),
                    _ => image,
                }))
            }
        },
    ))?;
    let registry = builder.build();
    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (attribute, body, expected) in [
            ("", "plain", None),
            ("", "default", Some("Default")),
            (",alt={label}", "plain", Some("Clock")),
            (",alt={label}", "default", Some("Clock")),
            (",alt={label}", "positional", Some("Clock")),
            (",alt=\"\"", "default", Some("")),
            (",alt=Clock", "override", Some("Override")),
            (",alt=Clock", "position-override", Some("Override")),
        ] {
            let source = format!(":label: Clock\n\n[picture{attribute}]\n....\n{body}\n....");
            let parsed = registry.parse(&source, &options)?.finish()?;
            let Some(BlockKind::Image(image)) =
                parsed.document().blocks().last().map(BlockRef::kind)
            else {
                return Err("expected generated image".into());
            };
            let alt = image
                .alt()
                .map(|content| {
                    content
                        .inlines()
                        .map(|inline| {
                            if let InlineKind::Plain(value) = inline.kind() {
                                Ok(value.text())
                            } else if let InlineKind::Raw(value) = inline.kind() {
                                Ok(value.text())
                            } else {
                                Err("unexpected alt content")
                            }
                        })
                        .collect::<Result<String, _>>()
                })
                .transpose()?;
            assert_eq!(alt.as_deref(), expected, "{mode:?} {source}");
        }
    }
    Ok(())
}

#[test]
fn only_registered_delimiters_are_consumed_and_empty_bodies_are_located() -> TestResult {
    let style = BlockStyle::raw("consume", [RawBlockContext::Literal], || {
        |block: RawBlockInput<'_>| {
            assert_eq!(block.body, "");
            assert_eq!(block.body_location.location.start.line, 3);
            Ok(None)
        }
    });
    let mut builder = Registry::builder();
    builder.register_block_style(style.clone())?;
    assert!(
        matches!(builder.register_block_style(style), Err(RegistrationError::DuplicateBlockStyle(name)) if name == "consume")
    );
    let registry = builder.build();
    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let opts = Options::builder().with_compatibility_mode(mode).build()?;
        let result = registry.parse("[consume]\n....\n....\n\n[consume]\n----\nstill a listing\n----\n\n[other]\n....\nstill literal\n....", &opts)?.finish()?;
        let kinds: Vec<_> = result.document().blocks().map(BlockRef::kind).collect();
        assert!(matches!(
            kinds.as_slice(),
            [
                BlockKind::DelimitedListing(_),
                BlockKind::DelimitedLiteral(_)
            ]
        ));
    }
    Ok(())
}

#[test]
fn unterminated_style_runs_once_and_handler_failures_keep_the_block_location() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_block_style(BlockStyle::raw("check", [RawBlockContext::Listing], || {
        |block: RawBlockInput<'_>| {
            if block.body == "invalid" {
                return Err("invalid sketch".into());
            }
            Ok(Some(BlockBuilder::literal(block.body)))
        }
    }))?;
    let registry = builder.build();
    let result = registry
        .parse("[check]\n----\nvalid", &Options::default())?
        .finish()?;
    assert_eq!(result.warnings().len(), 1);
    assert!(matches!(
        result.document().blocks().next().map(BlockRef::kind),
        Some(BlockKind::DelimitedLiteral(_))
    ));
    let failed = registry.parse("Intro.\n\n[check]\n----\ninvalid", &Options::default());
    assert!(
        matches!(failed, Err(Error::Extension(origin, msg)) if msg == "invalid sketch" && origin.location.start.line == 3)
    );
    Ok(())
}

#[test]
fn style_selection_and_inactive_styles_match_across_entry_points() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_block_style(BlockStyle::raw("stamp", [RawBlockContext::Literal], || {
        |raw: RawBlockInput<'_>| Ok(Some(BlockBuilder::paragraph(raw.body.trim_end())))
    }))?;
    let registry = builder.build();
    let options = Options::default();
    let source = ":selected: stamp\n\n[{selected}]\n....\nSeal\n....";
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("entry.adoc");
    std::fs::write(&path, source)?;
    for parsed in [
        registry.parse(source, &options)?,
        registry.parse_file(&path, &options)?,
        registry.parse_from_reader(source.as_bytes(), &options)?,
    ] {
        let parsed = parsed.finish()?;
        assert!(
            parsed
                .document()
                .blocks()
                .any(|block| matches!(block.kind(), BlockKind::Paragraph(_)))
        );
    }

    let ordinary = ".Code\n[source,rust]\n----\nfn main() {}\n----\n\n[other]\n....\nliteral\n....";
    let native = acdc_parser::parse(ordinary, &options)?;
    let inactive = registry.parse(ordinary, &options)?.finish()?;
    assert_eq!(
        serde_json::to_value(inactive.document())?,
        serde_json::to_value(native.document())?
    );
    assert_eq!(inactive.warnings(), native.warnings());
    Ok(())
}
