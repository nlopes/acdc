use acdc_parser::{Block, CompatibilityMode, Options, WarningKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case::plain("[discrete]", Some("discrete"), Some("discrete"))]
#[case::legacy("[float]", Some("float"), Some("float"))]
#[case::stacked_named("[discrete]\n[style=normal]", Some("discrete"), Some("normal"))]
#[case::stacked_positional("[style=normal]\n[discrete]", Some("discrete"), Some("discrete"))]
#[case::same_list_named("[discrete,style=normal]", Some("discrete"), Some("normal"))]
#[case::same_list_positional("[normal,style=discrete]", Some("normal"), Some("discrete"))]
#[case::named_discrete("[style=discrete]", None, Some("discrete"))]
#[case::named_float("[style=float]", None, Some("float"))]
#[case::named_role("[discrete]\n[role=example]", Some("discrete"), Some("discrete"))]
#[case::shorthand_role("[discrete]\n[.example]", Some("discrete"), Some("discrete"))]
#[case::empty_list("[discrete]\n[]", Some("discrete"), Some("discrete"))]
#[case::later_positional("[discrete]\n[normal]", Some("normal"), Some("normal"))]
#[case::role_after_named(
    "[discrete]\n[style=normal]\n[.example]",
    Some("discrete"),
    Some("normal")
)]
#[case::legacy_override("[float]\n[style=normal]", Some("float"), Some("normal"))]
fn heading_styles_keep_their_selected_meaning(
    #[case] attributes: &str,
    #[case] compatible: Option<&str>,
    #[case] draft: Option<&str>,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("{attributes}\n== Heading\n\nText.\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let expected = if mode == CompatibilityMode::Asciidoctor {
        compatible
    } else {
        draft
    };
    let heading = parsed.document().blocks.first().ok_or("missing heading")?;
    assert_eq!(
        heading.metadata().ok_or("missing metadata")?.style,
        expected
    );
    assert_eq!(
        matches!(heading, Block::DiscreteHeader(_)),
        matches!(expected, Some("discrete" | "float"))
    );
    assert_eq!(
        parsed
            .warnings()
            .iter()
            .filter(|warning| matches!(warning.kind, WarningKind::LegacyFloatDiscreteHeading))
            .count(),
        usize::from(expected == Some("float"))
    );
    Ok(())
}

#[rstest::rstest]
fn discrete_boundaries_preserve_nesting_and_register_heading_content_once(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/compatibility_discrete_contexts.adoc");
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().footnotes.len(), 1);
    let Some(Block::Section(parent)) = parsed.document().blocks.first() else {
        return Err("missing parent section".into());
    };
    let Some(Block::Section(child)) = parent.content.first() else {
        return Err("missing child section".into());
    };
    if mode == CompatibilityMode::Asciidoctor {
        assert!(matches!(
            child.content.first(),
            Some(Block::DiscreteHeader(_))
        ));
        assert!(matches!(child.content.get(1), Some(Block::Paragraph(_))));
        assert_eq!(parsed.document().blocks.len(), 2);
    } else {
        assert_eq!(child.content, Vec::new());
        assert_eq!(parsed.document().blocks.len(), 4);
    }
    Ok(())
}
