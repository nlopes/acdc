use acdc_parser::{CompatibilityMode, Options, parse};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn decorated_passthroughs_preserve_raw_and_protected_meaning_by_mode() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "Before [#sample.red.blue]++<b>*literal*</b>++ after.",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let value = serde_json::to_value(parsed.document_ref())?;
        let nodes = value
            .pointer("/blocks/0/inlines")
            .and_then(Value::as_array)
            .ok_or("inlines")?;
        let raw = nodes
            .iter()
            .find(|node| node.get("name") == Some(&json!("raw")))
            .ok_or("raw")?;
        assert_eq!(raw.get("value"), Some(&json!("<b>*literal*</b>")));
        assert_eq!(raw.get("substitutions"), Some(&json!(["special_chars"])));
        if mode == CompatibilityMode::Asciidoctor {
            assert_eq!(raw.pointer("/metadata/id/id"), Some(&json!("sample")));
            assert_eq!(
                raw.pointer("/metadata/roles"),
                Some(&json!(["red", "blue"]))
            );
            assert_eq!(
                nodes.first().and_then(|node| node.get("value")),
                Some(&json!("Before "))
            );
            assert!(parsed.document_ref().references().contains("sample"));
        } else {
            assert!(raw.get("metadata").is_none());
            assert_eq!(
                nodes.first().and_then(|node| node.get("value")),
                Some(&json!("Before [#sample.red.blue]"))
            );
            assert!(!parsed.document_ref().references().contains("sample"));
        }
    }
    Ok(())
}

fn raw_nodes(parsed: &acdc_parser::ParseResult) -> Vec<acdc_parser::ast::RawRef<'_>> {
    fn visit<'doc>(
        content: acdc_parser::ast::ContentRef<'doc>,
        result: &mut Vec<acdc_parser::ast::RawRef<'doc>>,
    ) {
        for node in content.inlines() {
            if let acdc_parser::ast::InlineKind::Raw(raw) = node.kind() {
                result.push(raw);
            }
            for field in node.content_fields() {
                visit(field.content(), result);
            }
        }
    }
    let mut result = Vec::new();
    for owner in parsed.document_ref().descendants() {
        for field in owner.content_fields() {
            visit(field.content(), &mut result);
        }
    }
    result
}

#[rstest::rstest]
#[case("[.red]+word+", true)]
#[case("x[.red]+word+", false)]
#[case("x[.red]++word++y", true)]
#[case("x[.red]+++word+++y", true)]
#[case("\\[.red]++word++", false)]
#[case("[.red]pass:[word]", false)]
#[case("[.red]pass:c[word]", false)]
fn passthrough_prefixes_follow_form_boundaries(
    #[case] input: &str,
    #[case] attributed: bool,
) -> Result<(), Error> {
    let source = format!("Before {input} after.");
    let parsed = parse(&source, &Options::default())?;
    let raw = raw_nodes(&parsed).into_iter().next().ok_or("raw")?;
    assert_eq!(raw.roles().iter().eq(["red"]), attributed);
    Ok(())
}

#[rstest::rstest]
#[case("macros", true)]
#[case("-quotes", true)]
#[case("quotes", false)]
#[case("none", false)]
fn passthrough_prefixes_require_macros_not_quotes(
    #[case] subs: &str,
    #[case] attributed: bool,
) -> Result<(), Error> {
    let source = format!("[subs={subs}]\nBefore [.red]++word++ after.");
    let parsed = parse(&source, &Options::default())?;
    assert_eq!(
        raw_nodes(&parsed).iter().any(|raw| !raw.roles().is_empty()),
        attributed
    );
    Ok(())
}

#[rstest::rstest]
#[case("target")]
#[case("é😀")]
#[case("{identifier}")]
fn passthrough_ids_map_to_the_authored_token_once(#[case] spelling: &str) -> Result<(), Error> {
    for context in [
        "Before {} after.",
        "Before *{}* after.",
        "[cols=a]\n|===\na|Before {} after.\n|===",
    ] {
        let body = context.replace("{}", &format!("[#old.red#{spelling}]++literal Ω++"));
        let source = format!(":identifier: expanded\n\nEarlier.\n\n{body}\n");
        let parsed = parse(&source, &Options::default())?;
        let before = serde_json::to_value(parsed.document_ref())?;
        let raws = raw_nodes(&parsed);
        let [raw] = raws.as_slice() else {
            return Err("one raw node".into());
        };
        let id = raw.id().ok_or("id")?;
        assert_eq!(
            id.as_str(),
            if spelling == "{identifier}" {
                "expanded"
            } else {
                spelling
            }
        );
        let start = id.location().absolute_start();
        let end = id.location().absolute_end();
        let end_width = source
            .get(end..)
            .and_then(|text| text.chars().next())
            .ok_or("end character")?
            .len_utf8();
        assert_eq!(source.get(start..end + end_width), Some(spelling));
        let owned = acdc_parser::InlineNode::RawText(acdc_parser::Raw::from(*raw));
        let node = acdc_parser::ast::InlineRef::from(&owned);
        assert_eq!(
            node.id().map(acdc_parser::ast::ElementIdRef::as_str),
            Some(id.as_str())
        );
        assert_eq!(
            raw.attributes().get("id"),
            Some(acdc_parser::ast::AttributeValueRef::Text(id.as_str()))
        );
        assert_eq!(raw.attributes().iter().count(), 2);
        assert_eq!(serde_json::to_value(parsed.document_ref())?, before);
    }
    Ok(())
}

#[cfg(feature = "asg")]
#[test]
fn asg_reports_passthrough_identity_instead_of_dropping_it() -> Result<(), Error> {
    for profile in [
        acdc_parser::asg::Profile::Upstream,
        acdc_parser::asg::Profile::Acdc,
    ] {
        for (prefix, field) in [("[#target]", "id"), ("[.red]", "roles")] {
            let source = format!("Before {prefix}++literal++.");
            let parsed = parse(&source, &Options::default())?;
            let raw = raw_nodes(&parsed).into_iter().next().ok_or("raw")?;
            let node = acdc_parser::InlineNode::RawText(acdc_parser::Raw::from(raw));
            let error = acdc_parser::asg::inlines_with_profile(
                acdc_parser::ast::ContentRef::from(&acdc_parser::ContentAttribute::new(vec![node])),
                profile,
            )
            .err()
            .ok_or("metadata must not disappear")?;
            let acdc_parser::asg::Error::Unsupported { path, .. } = error else {
                return Err("unsupported metadata".into());
            };
            assert_eq!(path, format!("/0/metadata/{field}"));
        }
    }
    Ok(())
}

#[test]
fn passthrough_id_locations_retain_the_include_chain() -> Result<(), Error> {
    let parsed = acdc_parser::parse_file(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/tests/inline_passthrough_metadata.adoc"),
        &Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .build()?,
    )?;
    let raw = raw_nodes(&parsed)
        .into_iter()
        .find(|raw| raw.id().is_some_and(|id| id.as_str() == "included"))
        .ok_or("included passthrough")?;
    let id = raw.id().ok_or("included ID")?;
    let location = id.location();
    assert_eq!((location.start().line(), location.start().column()), (2, 9));
    assert_eq!((location.end().line(), location.end().column()), (2, 16));
    assert_eq!(
        location
            .start()
            .include_chain()
            .ok_or("include chain")?
            .collect::<Vec<_>>(),
        ["includes/passthrough_metadata.adoc"]
    );
    Ok(())
}
