use acdc_parser::{
    CompatibilityMode, NORMAL, Options, Substitution, SubstitutionOp, SubstitutionSpec, VERBATIM,
    ast::{AttributeValueRef, BlockKind, SubstitutionSpecRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn typed_resolution_preserves_empty_forms_and_owned_results() {
    for defaults in [
        NORMAL,
        VERBATIM,
        &[Substitution::Macros, Substitution::Quotes],
    ] {
        for (spec, expected) in [
            (SubstitutionSpec::Source(Vec::new()), Vec::new()),
            (SubstitutionSpec::Explicit(Vec::new()), Vec::new()),
            (SubstitutionSpec::Modifiers(Vec::new()), defaults.to_vec()),
            (
                SubstitutionSpec::Explicit(vec![
                    Substitution::Normal,
                    Substitution::Quotes,
                    Substitution::Quotes,
                ]),
                vec![
                    Substitution::Normal,
                    Substitution::Quotes,
                    Substitution::Quotes,
                ],
            ),
        ] {
            let resolved = SubstitutionSpecRef::from(&spec).resolve(defaults);
            drop(spec);
            assert_eq!(resolved, expected);
        }
    }
}

#[test]
fn typed_resolution_keeps_group_and_modifier_order_without_rewriting_entries() {
    let source = SubstitutionSpec::Source(vec![
        " +verbatim ".into(),
        "quotes+".into(),
        "-callouts".into(),
        "unknown".into(),
        "+quotes".into(),
    ]);
    let original = source.clone();
    assert_eq!(
        SubstitutionSpecRef::from(&source).resolve(&[Substitution::Macros]),
        [
            Substitution::Quotes,
            Substitution::Macros,
            Substitution::SpecialChars
        ],
    );
    assert_eq!(source, original);
    let modifiers = SubstitutionSpec::Modifiers(vec![
        SubstitutionOp::Append(Substitution::Verbatim),
        SubstitutionOp::Prepend(Substitution::Quotes),
        SubstitutionOp::Remove(Substitution::Callouts),
        SubstitutionOp::Append(Substitution::Quotes),
    ]);
    assert_eq!(
        SubstitutionSpecRef::from(&modifiers).resolve(&[Substitution::Macros]),
        [
            Substitution::Quotes,
            Substitution::Macros,
            Substitution::SpecialChars
        ],
    );
}

#[rstest::rstest]
fn parsed_specs_resolve_without_an_owner_escape_hatch(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let parsed = parse("[subs=\"+quotes,-callouts\"]\nText", &options)?;
    let BlockKind::Paragraph(paragraph) = parsed
        .document_ref()
        .blocks()
        .next()
        .ok_or("paragraph")?
        .kind()
    else {
        return Err("expected paragraph".into());
    };
    let Some(AttributeValueRef::Substitutions(spec)) = paragraph.attributes().get("subs") else {
        return Err("expected typed substitution settings".into());
    };
    assert_eq!(
        spec.resolve(VERBATIM),
        [Substitution::SpecialChars, Substitution::Quotes]
    );
    Ok(())
}
