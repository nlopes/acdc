use acdc_parser::{
    Block, BlockMetadata, CompatibilityMode, InlineNode, Location, NORMAL, Options, Paragraph,
    Substitution, SubstitutionOp, SubstitutionSpec, VERBATIM, parse,
};
use serde_json::json;

type Error = Box<dyn std::error::Error>;

#[test]
fn substitution_metadata_preserves_absent_empty_and_populated_values() -> Result<(), Error> {
    let absent = BlockMetadata::default();
    assert!(absent.substitutions.is_none());
    assert!(absent.is_default());
    assert_eq!(serde_json::to_value(&absent)?, json!({}));

    for (spec, expected) in [
        (SubstitutionSpec::Explicit(Vec::new()), json!([])),
        (SubstitutionSpec::Source(Vec::new()), json!([])),
        (SubstitutionSpec::Modifiers(Vec::new()), json!([])),
        (
            SubstitutionSpec::Source(vec!["n".into(), "-macros".into(), "q".into()]),
            json!(["n", "-macros", "q"]),
        ),
        (
            SubstitutionSpec::Explicit(vec![Substitution::SpecialChars, Substitution::Quotes]),
            json!(["special_chars", "quotes"]),
        ),
        (
            SubstitutionSpec::Modifiers(vec![
                SubstitutionOp::Prepend(Substitution::Attributes),
                SubstitutionOp::Remove(Substitution::Callouts),
                SubstitutionOp::Append(Substitution::Quotes),
            ]),
            json!(["attributes+", "-callouts", "+quotes"]),
        ),
    ] {
        let mut metadata = absent.clone();
        metadata.substitutions = Some(spec);
        assert_ne!(metadata, absent);
        assert_eq!(metadata, metadata.clone());
        assert!(!metadata.is_default());
        assert_eq!(
            serde_json::to_value(&metadata)?,
            json!({"substitutions": expected})
        );
        let paragraph = Paragraph::new(Vec::new(), Location::default()).with_metadata(metadata);
        assert_eq!(
            serde_json::to_value(paragraph)?.pointer("/metadata/substitutions"),
            Some(&expected)
        );
    }
    Ok(())
}

#[test]
fn public_substitution_resolution_is_available_in_every_build() {
    let operations = vec![
        SubstitutionOp::Remove(Substitution::Callouts),
        SubstitutionOp::Prepend(Substitution::Attributes),
        SubstitutionOp::Append(Substitution::Quotes),
    ];
    let expected = vec![
        Substitution::Attributes,
        Substitution::SpecialChars,
        Substitution::Quotes,
    ];
    assert_eq!(
        SubstitutionSpec::apply_modifiers(&operations, VERBATIM),
        expected
    );
    assert_eq!(
        SubstitutionSpec::Modifiers(operations).resolve(VERBATIM),
        expected
    );
    assert_eq!(
        SubstitutionSpec::Source(vec![
            "verbatim".into(),
            "-callouts".into(),
            "attributes+".into(),
            "q".into()
        ])
        .resolve(NORMAL),
        expected
    );
    assert_eq!(SubstitutionSpec::Explicit(Vec::new()).resolve(NORMAL), []);
    assert_eq!(
        SubstitutionSpec::Modifiers(Vec::new()).resolve(NORMAL),
        NORMAL
    );
}

#[test]
fn block_substitution_interpretation_keeps_the_existing_feature_contract() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "[subs=none]\n*Bold*",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
            return Err("expected a paragraph".into());
        };
        assert_eq!(
            matches!(block.content.as_slice(), [InlineNode::BoldText(_)]),
            !cfg!(feature = "pre-spec-subs")
        );
        assert_eq!(
            block.metadata.substitutions,
            cfg!(feature = "pre-spec-subs").then(|| SubstitutionSpec::Explicit(Vec::new()))
        );
        assert_eq!(
            parsed.source_recovery().is_some(),
            !cfg!(feature = "pre-spec-subs")
        );
        let mut metadata = BlockMetadata::default();
        metadata.substitutions = Some(SubstitutionSpec::Explicit(Vec::new()));
        assert_eq!(
            metadata.uses_substitution(&Substitution::Quotes, NORMAL),
            !cfg!(feature = "pre-spec-subs")
        );
    }
    Ok(())
}
