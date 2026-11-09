use acdc_parser::{
    CompatibilityMode, InlineMetadata, Link, Location, Options, Source, TokenAssignment,
    ast::{AttributeValueRef, AttributesRef, BlockKind, InlineKind, LinkRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn check(attributes: AttributesRef<'_>, name: &str, expected: Option<&[&str]>) {
    let alias = if name == "role" { "roles" } else { "options" };
    assert_eq!(attributes.contains(name), expected.is_some());
    assert_eq!(attributes.get(name), attributes.get(alias));
    assert_eq!(attributes.contains(name), attributes.contains(alias));
    assert_eq!(
        attributes.iter().filter(|(key, _)| *key == name).count(),
        usize::from(expected.is_some())
    );
    assert!(!attributes.iter().any(|(key, _)| key == alias));
    if let Some(AttributeValueRef::Tokens(tokens)) = attributes.get(name) {
        assert!(tokens.is_present());
        assert_eq!(Some(tokens.iter().collect::<Vec<_>>()).as_deref(), expected);
        assert_eq!(tokens.is_empty(), expected.is_some_and(<[_]>::is_empty));
    } else {
        assert_eq!(attributes.get(name), None);
        assert_eq!(expected, None);
    }
}

#[rstest::rstest]
#[case("", None, None)]
#[case("[role=\"\",opts=\"\"]\n", Some(&[][..]), Some(&[][..]))]
#[case("[roles=\"one two\",options=\"first,second\"]\n", Some(&["one", "two"][..]), Some(&["first", "second"][..]))]
fn block_token_presence_is_visible_through_both_read_paths(
    #[case] prefix: &str,
    #[case] roles: Option<&[&str]>,
    #[case] options: Option<&[&str]>,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("{prefix}Text.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::Paragraph(node) = parsed.document().blocks().next().ok_or("paragraph")?.kind()
    else {
        return Err("paragraph".into());
    };
    check(node.attributes(), "role", roles);
    check(node.attributes(), "opts", options);
    assert_eq!(node.roles().is_present(), roles.is_some());
    assert_eq!(node.options().is_present(), options.is_some());
    let native = serde_json::to_value(parsed.document())?;
    assert_eq!(
        native.pointer("/blocks/0/metadata/roles"),
        roles.map(serde_json::to_value).transpose()?.as_ref()
    );
    assert_eq!(
        native.pointer("/blocks/0/metadata/options"),
        options.map(serde_json::to_value).transpose()?.as_ref()
    );
    Ok(())
}

#[rstest::rstest]
#[case("[.earlier%first]\n[role=\"\",opts=\"\"]\n", &[], &["earlier"])]
#[case("[role=earlier,role=\"\",opts=first,opts=\"\"]\n", &[], &["earlier"])]
#[case("[.earlier%first]\n[]\n", &["earlier"], &["earlier"])]
fn empty_assignments_keep_each_modes_merge_rules(
    #[case] prefix: &str,
    #[case] asciidoctor: &[&str],
    #[case] draft: &[&str],
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("{prefix}Text.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::Paragraph(node) = parsed.document().blocks().next().ok_or("paragraph")?.kind()
    else {
        return Err("paragraph".into());
    };
    check(
        node.attributes(),
        "role",
        Some(if mode == CompatibilityMode::Asciidoctor {
            asciidoctor
        } else {
            draft
        }),
    );
    check(node.attributes(), "opts", Some(&["first"]));
    Ok(())
}

#[rstest::rstest]
#[case("image:bird.png[{attributes}]")]
#[case("icon:star[{attributes}]")]
#[case("link:page.html[Label,{attributes}]")]
#[case("https://example.org[Label,{attributes}]")]
#[case("mailto:me@example.org[Label,{attributes}]")]
fn inline_macro_token_presence_survives_parsing(
    #[case] template: &str,
    #[values("", "role=\"\",opts=\"\"", "roles=one,options=first")] attributes: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = template.replace("{attributes}", attributes);
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::Paragraph(paragraph) =
        parsed.document().blocks().next().ok_or("paragraph")?.kind()
    else {
        return Err("paragraph".into());
    };
    let node = paragraph.content().inlines().next().ok_or("inline")?;
    let attributes_ref = if let InlineKind::Image(node) = node.kind() {
        node.attributes()
    } else if let InlineKind::Icon(node) = node.kind() {
        node.attributes()
    } else if let InlineKind::Link(node) = node.kind() {
        node.attributes()
    } else if let InlineKind::UrlMacro(node) = node.kind() {
        node.attributes()
    } else if let InlineKind::Mailto(node) = node.kind() {
        node.attributes()
    } else {
        return Err(format!("unexpected inline: {source}").into());
    };
    let (roles, options): (Option<&[&str]>, Option<&[&str]>) = if attributes.is_empty() {
        (None, None)
    } else if attributes.starts_with("role=") {
        (Some(&[]), Some(&[]))
    } else {
        (Some(&["one"]), Some(&["first"]))
    };
    check(attributes_ref, "role", roles);
    check(attributes_ref, "opts", options);
    Ok(())
}

#[test]
fn prepared_transfers_keep_empty_assignments_and_reset_the_previous_owner() {
    let mut node = Link::new(Source::Name("target"), Location::default());
    assert!(node.replace_options(Vec::new()).is_absent());
    let mut metadata = InlineMetadata::default();
    metadata.roles = TokenAssignment::new(Vec::new());
    node.set_metadata(metadata);
    let read = LinkRef::from(&node);
    check(read.attributes(), "role", Some(&[]));
    check(read.attributes(), "opts", Some(&[]));
    let copied = node.clone_metadata();
    assert_eq!(copied, node.take_metadata());
    let options = node.take_options();
    assert_eq!(options.into_tokens(), Some(Vec::new()));
    let read = LinkRef::from(&node);
    check(read.attributes(), "role", None);
    check(read.attributes(), "opts", None);
    assert!(!read.roles().is_present());
    assert!(!read.options().is_present());
}

#[cfg(feature = "asg")]
#[rstest::rstest]
fn asg_keeps_block_token_presence_in_both_profiles(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    use acdc_parser::asg::{self, Profile};

    for (attributes, expected) in [
        ("", None),
        ("[role=\"\",opts=\"\"]\n", Some(serde_json::json!([]))),
        ("[role=one,opts=one]\n", Some(serde_json::json!(["one"]))),
    ] {
        let source = format!("{attributes}Text.");
        let parsed = parse(
            &source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        for profile in [Profile::Upstream, Profile::Acdc] {
            let value = asg::export_document_with_profile(parsed.document(), profile)?;
            for field in ["roles", "options"] {
                assert_eq!(
                    value.pointer(&format!("/blocks/0/metadata/{field}")),
                    expected.as_ref()
                );
            }
        }
    }
    Ok(())
}

#[cfg(feature = "asg")]
#[rstest::rstest]
#[case("role=\"\"", "roles")]
#[case("opts=\"\"", "options")]
fn asg_preserves_empty_inline_tokens_or_reports_the_unsupported_path(
    #[case] attributes: &str,
    #[case] field: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    use acdc_parser::asg::{self, Profile};

    let source = format!("link:page.html[Label,{attributes}]");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let path = format!("/blocks/0/inlines/0/metadata/{field}");
    let before = serde_json::to_value(parsed.document())?;
    assert_eq!(before.pointer(&path), Some(&serde_json::json!([])));
    let value = asg::export_document_with_profile(parsed.document(), Profile::Acdc)?;
    assert_eq!(value.pointer(&path), Some(&serde_json::json!([])));
    assert!(
        matches!(asg::export_document(parsed.document()), Err(asg::Error::Unsupported { path: rejected, .. }) if rejected == path)
    );
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[cfg(feature = "asg")]
#[test]
fn empty_span_roles_stay_distinct_from_absence_in_asg() -> Result<(), Error> {
    use acdc_parser::{
        Bold, ContentAttribute, FormattingForm, InlineNode,
        asg::{self, Profile},
        ast::ContentRef,
    };

    let mut span = Bold::new(Vec::new(), FormattingForm::Constrained, Location::default());
    span.replace_role(Some(""));
    let content = ContentAttribute::new(vec![InlineNode::BoldText(span)]);
    let value = asg::export_inlines_with_profile(ContentRef::from(&content), Profile::Acdc)?;
    assert_eq!(
        value.pointer("/0/metadata/roles"),
        Some(&serde_json::json!([]))
    );
    assert!(
        matches!(asg::export_inlines(ContentRef::from(&content)), Err(asg::Error::Unsupported { path, .. }) if path == "/0/role")
    );
    Ok(())
}

#[cfg(feature = "asg")]
#[test]
fn asg_rejects_empty_passthrough_roles_without_a_mapping() {
    use acdc_parser::{
        ContentAttribute, InlineNode, Raw,
        asg::{self, Profile},
        ast::ContentRef,
    };

    let mut raw = Raw::new("text", Vec::new(), Location::default());
    let mut metadata = InlineMetadata::default();
    metadata.roles = Vec::new().into();
    raw.set_metadata(Some(metadata));
    let content = ContentAttribute::new(vec![InlineNode::RawText(raw)]);
    for profile in [Profile::Upstream, Profile::Acdc] {
        assert!(
            matches!(asg::export_inlines_with_profile(ContentRef::from(&content), profile), Err(asg::Error::Unsupported { path, .. }) if path == "/0/metadata/roles")
        );
    }
}
