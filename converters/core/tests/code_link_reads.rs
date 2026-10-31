//! Link labels, destinations, and registered IDs inside source code.

use acdc_converters_core::code::{CodeLinkTarget, resolve_code_link};
use acdc_parser::{Block, CompatibilityMode, Options, ast::InlineRef, parse, parse_inline};

type Error = Box<dyn std::error::Error>;

type Target<'a> = Option<(&'a str, &'a str)>;

const LINK_CASES: &[(&str, &str, &str, Target<'_>)] = &[
    (
        "",
        "link:https://example.org[Été & 日本語]",
        "Été & 日本語",
        Some(("external", "https://example.org")),
    ),
    (
        "",
        "https://example.org[*Bold*]",
        "Bold",
        Some(("external", "https://example.org")),
    ),
    (
        "",
        "link:https://example.org[]",
        "https://example.org",
        Some(("external", "https://example.org")),
    ),
    (
        ":hide-uri-scheme:\n",
        "link:https://example.org[]",
        "example.org",
        Some(("external", "https://example.org")),
    ),
    (
        "",
        "mailto:team@example.org[,Hello & there,Line two]",
        "team@example.org",
        Some((
            "external",
            "mailto:team@example.org?subject=Hello%20%26%20there&body=Line%20two",
        )),
    ),
    (
        "",
        "https://example.org",
        "https://example.org",
        Some(("external", "https://example.org")),
    ),
    (
        ":hide-uri-scheme:\n",
        "https://example.org",
        "example.org",
        Some(("external", "https://example.org")),
    ),
    (
        "",
        "<team@example.org>",
        "<team@example.org>",
        Some(("external", "mailto:team@example.org")),
    ),
    (
        "",
        "xref:target[]",
        "Destination",
        Some(("internal", "target")),
    ),
    (
        "",
        "xref:target[*Explicit*]",
        "Explicit",
        Some(("internal", "target")),
    ),
    ("", "xref:missing[]", "[missing]", None),
    (
        "",
        "xref:other.adoc#target[]",
        "other.html",
        Some(("external", "other.html#target")),
    ),
    (
        "",
        "xref:other.adoc#target[Explicit]",
        "Explicit",
        Some(("external", "other.html#target")),
    ),
    (
        ":xrefstyle: full\n",
        "xref:figure[]",
        "Figure 1, “Picture”",
        Some(("internal", "figure")),
    ),
    ("", "xref:#[]", "Document", Some(("top", ""))),
];

#[test]
fn code_link_display_and_destination_are_preserved() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for &(attributes, input, text, target) in LINK_CASES {
            check_code_link(attributes, input, text, target, mode)?;
        }
    }
    Ok(())
}

fn check_code_link(
    attributes: &str,
    input: &str,
    expected_text: &str,
    expected_target: Option<(&str, &str)>,
    mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(
        "= Document\n{attributes}\n{input}\n\n[[target]]\n== *Destination*\n\n[#figure]\n.Picture\nimage::bird.svg[]\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Block::Paragraph(paragraph) = parsed.document().blocks.first().ok_or("paragraph")? else {
        return Err("paragraph".into());
    };
    let links: Vec<_> = paragraph
        .content
        .iter()
        .flat_map(|node| {
            [
                resolve_code_link(node, &parsed.document().references, "html"),
                resolve_code_link(InlineRef::from(node), &parsed.document().references, "html"),
            ]
        })
        .flatten()
        .collect();
    assert_eq!(links.len(), 2, "{input}");
    for link in links {
        assert_eq!(link.text, expected_text);
        let target = link.target.as_ref().map(|target| match target {
            CodeLinkTarget::External(value) => ("external", value.as_str()),
            CodeLinkTarget::Internal(value) => ("internal", value.as_str()),
            CodeLinkTarget::DocumentTop => ("top", ""),
        });
        assert_eq!(target, expected_target);
        assert!(link.anchor.is_none());
    }
    Ok(())
}

#[test]
fn code_link_ids_belong_only_to_the_registered_occurrence() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        check_registered_occurrence(mode)?;
    }
    Ok(())
}

fn check_registered_occurrence(mode: CompatibilityMode) -> Result<(), Error> {
    let parsed = parse(
        "link:https://example.org/first[First,id=shared] link:https://example.org/second[Second,id=shared]",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Block::Paragraph(paragraph) = parsed.document().blocks.first().ok_or("paragraph")? else {
        return Err("paragraph".into());
    };
    let links: Vec<_> = paragraph
        .content
        .iter()
        .flat_map(|node| {
            [
                resolve_code_link(node, &parsed.document().references, "html"),
                resolve_code_link(InlineRef::from(node), &parsed.document().references, "html"),
            ]
        })
        .flatten()
        .collect();
    assert_eq!(links.len(), 4);
    let [first, typed_first, second, typed_second] = links.as_slice() else {
        return Err("two code links".into());
    };
    assert_eq!(first.anchor.as_deref(), Some("shared"));
    assert_eq!(typed_first.anchor, first.anchor);
    assert_eq!(second.anchor, None);
    assert_eq!(typed_second.anchor, None);
    Ok(())
}

#[test]
fn code_link_results_outlive_temporary_inline_inputs() -> Result<(), Error> {
    let document = parse("[[target]]\n== Destination\n", &Options::default())?;
    let resolved = {
        let input = String::from("xref:target[*Temporary*]");
        let parsed = parse_inline(&input, &Options::default())?;
        let node = parsed.content().inlines().next().ok_or("temporary xref")?;
        resolve_code_link(node, &document.document().references, "pdf").ok_or("code link")?
    };
    assert_eq!(resolved.text, "Temporary");
    assert!(
        matches!(resolved.target, Some(CodeLinkTarget::Internal(target)) if target == "target")
    );
    Ok(())
}
