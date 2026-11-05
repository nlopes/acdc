use serde_json::{Map, Value};

use crate::{
    ContentAttribute, CrossReference, ElementAttributes, InlineMacro, InlineMetadata, InlineNode,
    Mailto, XrefCaptionLabel, XrefSignifier, XrefStyle,
};

use super::{Error, Profile, metadata, pointer, projection::inlines, target, unsupported};

type Node = Map<String, Value>;

pub(super) fn project(
    value: &InlineMacro<'_>,
    path: &str,
    profile: Profile,
) -> Result<Node, Error> {
    match value {
        InlineMacro::Link(link) => {
            let target = target(&link.target, path)?;
            with_metadata(
                reference(
                    "link",
                    &target,
                    &link.text,
                    Some(display_target(&target, link.hides_uri_scheme())),
                    path,
                    profile,
                )?,
                &link.metadata,
                link.attributes(),
                link.options(),
                link.tooltip.as_ref(),
                path,
                profile,
            )
        }
        InlineMacro::Url(link) => {
            let target = target(&link.target, path)?;
            with_metadata(
                reference(
                    "link",
                    &target,
                    &link.text,
                    Some(display_target(&target, link.hides_uri_scheme())),
                    path,
                    profile,
                )?,
                &link.metadata,
                link.attributes(),
                link.options(),
                link.tooltip.as_ref(),
                path,
                profile,
            )
        }
        InlineMacro::Mailto(link) => {
            let original = target(&link.target, path)?;
            with_metadata(
                reference(
                    "link",
                    &mailto_target(link, &original),
                    &link.text,
                    Some(mailto_address(&original).unwrap_or(&original)),
                    path,
                    profile,
                )?,
                &link.metadata,
                link.attributes(),
                link.options(),
                link.tooltip.as_ref(),
                path,
                profile,
            )
        }
        InlineMacro::Autolink(link) => {
            let target = target(&link.url, path)?;
            let display = mailto_address(&target)
                .unwrap_or_else(|| display_target(&target, link.hides_uri_scheme()));
            reference("link", &target, &[], Some(display), path, profile)
        }
        InlineMacro::CrossReference(xref) => cross_reference(xref, path, profile),
        InlineMacro::Footnote(_)
        | InlineMacro::Icon(_)
        | InlineMacro::Image(_)
        | InlineMacro::Keyboard(_)
        | InlineMacro::Button(_)
        | InlineMacro::Menu(_)
        | InlineMacro::Pass(_)
        | InlineMacro::Stem(_)
        | InlineMacro::IndexTerm(_) => Err(unsupported(
            path,
            "this inline macro has no supported upstream mapping",
        )),
    }
}

fn cross_reference(xref: &CrossReference<'_>, path: &str, profile: Profile) -> Result<Node, Error> {
    for (present, property) in [
        (xref.xrefstyle != XrefStyle::Default, "xrefstyle"),
        (
            xref.caption_label != XrefCaptionLabel::AtTarget,
            "caption_label",
        ),
        (!default_signifier(xref), "signifier"),
        (
            xref.target_is_local != CrossReference::is_local_target(xref.target),
            "target_is_local",
        ),
    ] {
        if present {
            return Err(unsupported(
                &pointer(path, property),
                "this reference setting has no supported upstream mapping",
            ));
        }
    }
    with_metadata(
        reference("xref", xref.target, &xref.text, None, path, profile)?,
        &xref.metadata,
        xref.attributes(),
        xref.options(),
        None,
        path,
        profile,
    )
}

fn default_signifier(xref: &CrossReference<'_>) -> bool {
    if xref.signifier() == XrefSignifier::Standard {
        return true;
    }
    let defaults = crate::model::default_document_attribute_values();
    xref.section_signifiers.is_some_and(|signifiers| {
        signifiers.contains(&xref.signifier())
            && signifiers
                .iter()
                .zip(crate::model::REFSIG_ATTRIBUTES)
                .all(|(actual, (_, key))| {
                    match defaults
                        .get(key)
                        .and_then(crate::DocumentAttributeValue::as_str)
                    {
                        Some(expected) => *actual == XrefSignifier::AtReference(expected),
                        None => *actual == XrefSignifier::Standard,
                    }
                })
    })
}

fn reference(
    variant: &str,
    target: &str,
    label: &[InlineNode<'_>],
    fallback: Option<&str>,
    path: &str,
    profile: Profile,
) -> Result<Node, Error> {
    let content = if label.is_empty() {
        Value::Array(
            fallback
                .filter(|text| !text.is_empty())
                .map_or_else(Vec::new, |text| {
                    vec![serde_json::json!({"name":"text", "type":"string", "value":text})]
                }),
        )
    } else {
        inlines(label, &pointer(path, "inlines"), profile)?
    };
    Ok(Map::from_iter([
        ("name".into(), "ref".into()),
        ("type".into(), "inline".into()),
        ("variant".into(), variant.into()),
        ("target".into(), target.into()),
        ("inlines".into(), content),
    ]))
}

fn with_metadata(
    mut node: Node,
    value: &InlineMetadata<'_>,
    attributes: &ElementAttributes<'_>,
    options: &[&str],
    tooltip: Option<&ContentAttribute<'_>>,
    path: &str,
    profile: Profile,
) -> Result<Node, Error> {
    if profile == Profile::Upstream {
        check(value, attributes, options, tooltip, path)?;
    } else {
        metadata::apply_inline(&mut node, value, attributes, options, path)?;
        if let Some(tooltip) = tooltip {
            node.insert(
                "tooltip".into(),
                inlines(tooltip.as_inlines(), &pointer(path, "tooltip"), profile)?,
            );
        }
    }
    Ok(node)
}

fn check(
    metadata: &InlineMetadata<'_>,
    attributes: &ElementAttributes<'_>,
    options: &[&str],
    tooltip: Option<&ContentAttribute<'_>>,
    path: &str,
) -> Result<(), Error> {
    for (present, property) in [
        (metadata.id.is_some(), "metadata/id"),
        (!metadata.roles.is_empty(), "metadata/roles"),
        (!options.is_empty(), "metadata/options"),
        (tooltip.is_some(), "tooltip"),
    ] {
        if present {
            return Err(unsupported(
                &format!("{path}/{property}"),
                "upstream references have no field for this property",
            ));
        }
    }
    if let Some((name, _)) = attributes.iter().next() {
        return Err(unsupported(
            &pointer(&format!("{path}/metadata/attributes"), name),
            "upstream references have no scalar attribute fields",
        ));
    }
    Ok(())
}

pub(super) fn mailto_address(target: &str) -> Option<&str> {
    target
        .get(..7)
        .filter(|scheme| scheme.eq_ignore_ascii_case("mailto:"))
        .and_then(|_| target.get(7..))
}

fn display_target(target: &str, hide_scheme: bool) -> &str {
    let Some((scheme, rest)) = target.split_once(':').filter(|_| hide_scheme) else {
        return target;
    };
    let mut bytes = scheme.bytes();
    if scheme.len() < 2
        || !bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
    {
        return target;
    }
    rest.strip_prefix("//")
        .or_else(|| rest.strip_prefix('/'))
        .unwrap_or(rest)
}

fn mailto_target(link: &Mailto<'_>, original: &str) -> String {
    let mut target = if mailto_address(original).is_some() {
        original.into()
    } else {
        format!("mailto:{original}")
    };
    if link.subject.is_none() && link.body.is_none() {
        return target;
    }
    if let Some((address, query)) = target.split_once('?') {
        let query = query
            .split('&')
            .filter(|field| {
                let name = field.split_once('=').map_or(*field, |(name, _)| name);
                let decoded = url::form_urlencoded::parse(name.as_bytes())
                    .next()
                    .map(|(name, _)| name);
                !field.is_empty()
                    && !decoded.is_some_and(|name| {
                        link.subject.is_some() && name.eq_ignore_ascii_case("subject")
                            || link.body.is_some() && name.eq_ignore_ascii_case("body")
                    })
            })
            .collect::<Vec<_>>()
            .join("&");
        target = if query.is_empty() {
            address.into()
        } else {
            format!("{address}?{query}")
        };
    }
    for (name, value) in [("subject", link.subject), ("body", link.body)] {
        if let Some(value) = value {
            let normalized;
            let value = if name == "body" {
                normalized = value.replace("\r\n", "\n").replace(['\r', '\n'], "\r\n");
                &normalized
            } else {
                value
            };
            let encoded = url::form_urlencoded::byte_serialize(value.as_bytes())
                .collect::<String>()
                .replace('+', "%20");
            target.push(if target.contains('?') { '&' } else { '?' });
            target.push_str(name);
            target.push('=');
            target.push_str(&encoded);
        }
    }
    target
}
