//! Asciidoctor mode normalizes attribute names as Asciidoctor does: an entry's
//! name loses every character other than a letter, a digit, `_`, or `-`, and is
//! lowercased, and references, conditional directives, and caller attributes
//! are lowercased. Draft mode keeps names as written. Both modes warn about an
//! entry whose name isn't normalized.

use std::collections::BTreeMap;

use acdc_parser::{
    CompatibilityMode, Options, ParseResult, WarningKind,
    ast::{BlockKind, ContentRef, InlineKind, StructuralRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn options(mode: CompatibilityMode) -> Result<Options<'static>, Error> {
    Ok(Options::builder().with_compatibility_mode(mode).build()?)
}

/// The plain text of each paragraph.
fn paragraphs(parsed: &ParseResult) -> Vec<String> {
    parsed
        .document()
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::Block(block) = node
                && let BlockKind::Paragraph(paragraph) = block.kind()
            {
                Some(
                    paragraph
                        .content()
                        .inlines()
                        .filter_map(|inline| {
                            if let InlineKind::Plain(node) = inline.kind() {
                                Some(node.text())
                            } else {
                                None
                            }
                        })
                        .collect(),
                )
            } else {
                None
            }
        })
        .collect()
}

/// The name warnings as `(name, normalized, applied, line)`.
fn name_warnings(parsed: &ParseResult) -> Vec<(String, Option<String>, bool, Option<u32>)> {
    parsed
        .warnings()
        .iter()
        .filter_map(|warning| {
            if let WarningKind::AttributeNameNotNormalized {
                name,
                normalized,
                applied,
            } = &warning.kind
            {
                assert!(warning.advice().is_some());
                let line = warning
                    .source_location()
                    .map(|source| source.location.start.line);
                Some((name.clone(), normalized.clone(), *applied, line))
            } else {
                None
            }
        })
        .collect()
}

const PROBE: &str = "= Title\n:TOC:\n:First Name: Dan\n:my.attr: dotted\n:Mixed-Case_Name: mixed\n\nupper-ref={TOC}x first={FirstName}|{firstname} dotted={myattr}|{my.attr} mixed={mixed-case_name}|{MIXED-CASE_NAME}\n";

/// The output of Asciidoctor 2.0.26 for `PROBE`.
#[test]
fn asciidoctor_mode_normalizes_names_like_asciidoctor() -> Result<(), Error> {
    let parsed = parse(PROBE, &options(CompatibilityMode::Asciidoctor)?)?;
    assert_eq!(
        paragraphs(&parsed),
        ["upper-ref=x first=Dan|Dan dotted=dotted|{my.attr} mixed=mixed|mixed"]
    );
    let attributes = parsed.document().attributes();
    for name in ["toc", "firstname", "myattr", "mixed-case_name"] {
        assert!(attributes.assignment(name).is_some(), "{name}");
    }
    assert!(
        attributes
            .assignments()
            .all(|(name, _)| !name.chars().any(char::is_uppercase))
    );
    Ok(())
}

#[test]
fn draft_mode_keeps_names_as_written() -> Result<(), Error> {
    let parsed = parse(PROBE, &options(CompatibilityMode::Draft)?)?;
    assert_eq!(
        paragraphs(&parsed),
        [
            "upper-ref=x first={FirstName}|{firstname} dotted={myattr}|{my.attr} mixed={mixed-case_name}|{MIXED-CASE_NAME}"
        ]
    );
    let attributes = parsed.document().attributes();
    for name in ["TOC", "First Name", "my.attr", "Mixed-Case_Name"] {
        assert!(attributes.assignment(name).is_some(), "{name}");
    }
    assert!(attributes.assignment("toc").is_none());
    Ok(())
}

#[rstest::rstest]
fn entries_whose_names_are_not_normalized_warn(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("{PROBE}\n:Body-Attr: body\n:!Body.Attr:\n:ok_name-1: fine\n");
    let parsed = parse(&source, &options(mode)?)?;
    let applied = mode == CompatibilityMode::Asciidoctor;
    let expected = [
        ("TOC", "toc", 2),
        ("First Name", "firstname", 3),
        ("my.attr", "myattr", 4),
        ("Mixed-Case_Name", "mixed-case_name", 5),
        ("Body-Attr", "body-attr", 9),
        ("Body.Attr", "bodyattr", 10),
    ]
    .map(|(name, normalized, line)| {
        (
            name.to_owned(),
            Some(normalized.to_owned()),
            applied,
            Some(line),
        )
    });
    assert_eq!(name_warnings(&parsed), expected);
    Ok(())
}

/// Built-in attributes take effect under their normalized names: in
/// Asciidoctor mode, `:TOC:` and `:Description:` give the same attributes as
/// `:toc:` and `:description:`.
#[test]
fn built_in_attributes_are_set_by_their_normalized_names() -> Result<(), Error> {
    let written = "= Title\n:TOC: left\n:Description: Upper\n\n== Section\n\nText.\n";
    let normalized = "= Title\n:toc: left\n:description: Upper\n\n== Section\n\nText.\n";
    let effective = |source: &str, mode| -> Result<BTreeMap<String, String>, Error> {
        let parsed = parse(source, &options(mode)?)?;
        Ok(parsed
            .document()
            .attributes()
            .iter()
            .map(|(name, value)| (name.to_owned(), format!("{value:?}")))
            .collect())
    };
    let asciidoctor = effective(written, CompatibilityMode::Asciidoctor)?;
    assert!(asciidoctor.contains_key("toc"));
    assert!(asciidoctor.contains_key("description"));
    assert_eq!(
        asciidoctor,
        effective(normalized, CompatibilityMode::Asciidoctor)?
    );
    let draft = effective(written, CompatibilityMode::Draft)?;
    assert!(!draft.contains_key("toc"));
    assert!(!draft.contains_key("description"));
    Ok(())
}

#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor, &["body|body", "body|body"])]
#[case::draft(CompatibilityMode::Draft, &["{body-attr}|{BODY-ATTR}", "body|{body-attr}"])]
fn body_entries_set_their_normalized_names(
    #[case] mode: CompatibilityMode,
    #[case] expected: &[&str],
) -> Result<(), Error> {
    let parsed = parse(
        "= Title\n\n:Body-Attr: body\n{body-attr}|{BODY-ATTR}\n\n{Body-Attr}|{body-attr}\n",
        &options(mode)?,
    )?;
    let name = parsed
        .document()
        .blocks()
        .find_map(|block| {
            if let BlockKind::DocumentAttribute(attribute) = block.kind() {
                Some(attribute.name())
            } else {
                None
            }
        })
        .ok_or("attribute entry")?;
    let expected_name = if mode == CompatibilityMode::Asciidoctor {
        "body-attr"
    } else {
        "Body-Attr"
    };
    assert_eq!(name, expected_name);
    let text = paragraphs(&parsed);
    assert_eq!(text, expected);
    Ok(())
}

#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor, &["upper", "lower", "either", "eval"])]
#[case::draft(CompatibilityMode::Draft, &["not"])]
fn conditional_directives_ignore_case_in_asciidoctor_mode(
    #[case] mode: CompatibilityMode,
    #[case] expected: &[&str],
) -> Result<(), Error> {
    let source = "= Title\n:Lower-Me: yes\n\nifdef::LOWER-ME[]\nupper\nendif::[]\n\nifdef::lower-me[]\nlower\nendif::[]\n\nifndef::LOWER-ME[]\nnot\nendif::[]\n\nifdef::MISSING,LOWER-ME[]\neither\nendif::[]\n\nifeval::[\"{LOWER-ME}\" == \"yes\"]\neval\nendif::[]\n";
    let parsed = parse(source, &options(mode)?)?;
    assert_eq!(paragraphs(&parsed), expected);
    Ok(())
}

#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor, &["x", "{some-attr}", "{other-attr}"])]
#[case::draft(CompatibilityMode::Draft, &["{some-attr}", "{some-attr}", "{other-attr}"])]
fn unset_entries_unset_their_normalized_names(
    #[case] mode: CompatibilityMode,
    #[case] expected: &[&str],
) -> Result<(), Error> {
    let source = "= Title\n:Some-Attr: x\n:Other-Attr: y\n\n{some-attr}\n\n:!SOME-ATTR:\n{some-attr}\n\n:OTHER-ATTR!:\n{other-attr}\n";
    let parsed = parse(source, &options(mode)?)?;
    assert_eq!(paragraphs(&parsed), expected);
    Ok(())
}

/// Asciidoctor lowercases caller attribute names but doesn't remove characters.
#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor, &["v|v|{firstname}", "defined"])]
#[case::draft(CompatibilityMode::Draft, &["{my-attr}|{MY-ATTR}|{firstname}"])]
fn caller_attribute_names_are_lowercased_in_asciidoctor_mode(
    #[case] mode: CompatibilityMode,
    #[case] expected: &[&str],
) -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(mode)
        .with_attribute("My-Attr", "v")
        .with_attribute("First Name", "Dan")
        .build()?;
    let parsed = parse(
        "{my-attr}|{MY-ATTR}|{firstname}\n\nifdef::my-attr[]\ndefined\nendif::[]\n",
        &options,
    )?;
    assert_eq!(paragraphs(&parsed), expected);
    assert_eq!(name_warnings(&parsed), []);
    Ok(())
}

/// Asciidoctor reads an entry only when its name starts with a letter, a
/// digit, or `_`. Draft mode reads the entry and warns.
#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor, &[":.x: dot\n:-x: dash"], 0)]
#[case::draft(CompatibilityMode::Draft, &[], 2)]
fn names_must_start_with_a_word_character_in_asciidoctor_mode(
    #[case] mode: CompatibilityMode,
    #[case] expected: &[&str],
    #[case] warnings: usize,
) -> Result<(), Error> {
    let parsed = parse(":.x: dot\n:-x: dash\n", &options(mode)?)?;
    assert_eq!(paragraphs(&parsed), expected);
    let found = name_warnings(&parsed);
    assert_eq!(found.len(), warnings);
    assert!(
        found
            .iter()
            .all(|(_, normalized, applied, _)| normalized.is_none() && !applied)
    );
    Ok(())
}

/// Conditional directives see entries whose names have characters other than
/// letters, digits, `_`, and `-`: by the normalized name in Asciidoctor mode,
/// and by the name as written in Draft mode.
#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor, &["normalized-dotted", "normalized-spaced"])]
#[case::draft(CompatibilityMode::Draft, &["exact-dotted"])]
fn conditional_directives_see_entries_with_any_name_characters(
    #[case] mode: CompatibilityMode,
    #[case] expected: &[&str],
) -> Result<(), Error> {
    let source = ":my.attr: x\n:First Name: Dan\n\nifdef::my.attr[]\nexact-dotted\nendif::[]\n\nifdef::myattr[]\nnormalized-dotted\nendif::[]\n\nifdef::firstname[]\nnormalized-spaced\nendif::[]\n";
    let parsed = parse(source, &options(mode)?)?;
    assert_eq!(paragraphs(&parsed), expected);
    Ok(())
}

/// Reference names follow Asciidoctor in both modes: they start with a word
/// character (a letter, a mark, a decimal digit, or `_`) and continue with word
/// characters and `-`. The caller attributes `-x` and `x²` exist, but `{-x}` and
/// `{x²}` aren't references.
#[rstest::rstest]
#[case::asciidoctor(
    CompatibilityMode::Asciidoctor,
    "latte|latte|nfd|_|{-x}|{x²}|{a b}|{a bee"
)]
#[case::draft(CompatibilityMode::Draft, "latte|{CAFÉ}|nfd|_|{-x}|{x²}|{a b}|{a bee")]
fn reference_names_follow_asciidoctor(
    #[case] mode: CompatibilityMode,
    #[case] expected: &str,
) -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(mode)
        .with_attribute("-x", "dash")
        .with_attribute("x²", "sup")
        .build()?;
    let parsed = parse(
        ":café: latte\n:cafe\u{301}d: nfd\n:_u: _\n:b: bee\n\n{café}|{CAFÉ}|{cafe\u{301}d}|{_u}|{-x}|{x²}|{a b}|{a {b}\n",
        &options,
    )?;
    assert_eq!(paragraphs(&parsed), [expected]);
    Ok(())
}

/// The text of plain, raw, and verbatim inlines.
fn text(content: ContentRef<'_>) -> String {
    content
        .inlines()
        .filter_map(|inline| {
            if let InlineKind::Plain(node) = inline.kind() {
                Some(node.text())
            } else if let InlineKind::Raw(node) = inline.kind() {
                Some(node.text())
            } else if let InlineKind::Verbatim(node) = inline.kind() {
                Some(node.text())
            } else {
                None
            }
        })
        .collect()
}

/// Paragraphs, escaped passthroughs, `pass:a` entry values, block titles,
/// block attribute lists, and verbatim blocks with `subs=attributes` read the
/// same reference names, as in Asciidoctor.
#[rstest::rstest]
fn every_reference_context_reads_the_same_names(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(mode)
        .with_attribute("-x", "dash")
        .build()?;
    let source = ":café: latte\n:b: bee\n:v: pass:a[{café} {a b} {-x} \\{café}]\n\n{v}\n\n\\+{café} {-x} {a {b}+\n\n.Title {café} {-x}\n[source,role={café},subs=attributes+]\n----\nraw {café} {-x} {a {b}\n----\n";
    let parsed = parse(source, &options)?;
    assert_eq!(
        paragraphs(&parsed),
        ["latte {a b} {-x} {café}", "+latte {-x} {a bee+"]
    );
    let listing = parsed
        .document()
        .blocks()
        .find_map(|block| {
            if let BlockKind::DelimitedListing(listing) = block.kind() {
                Some(listing)
            } else {
                None
            }
        })
        .ok_or("listing")?;
    assert_eq!(text(listing.title().ok_or("title")?), "Title latte {-x}");
    assert!(listing.roles().contains("latte"));
    assert_eq!(text(listing.content()), "raw latte {-x} {a bee");
    Ok(())
}

/// Like Asciidoctor, Asciidoctor mode doesn't count the zero-width non-joiner
/// (U+200C) and zero-width joiner (U+200D) as word characters: an entry drops
/// them from its name, and a reference that contains one is text. Draft mode
/// counts them in reference names.
#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor, &["{a\u{200C}b}|plain|{\u{200D}x}"])]
#[case::draft(CompatibilityMode::Draft, &["nonjoiner|plain|joiner", "defined"])]
fn join_controls_are_word_characters_only_in_draft_mode(
    #[case] mode: CompatibilityMode,
    #[case] expected: &[&str],
) -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(mode)
        .with_attribute("\u{200D}x", "joiner")
        .build()?;
    let parsed = parse(
        ":a\u{200C}b: nonjoiner\n:ab: plain\n\n{a\u{200C}b}|{ab}|{\u{200D}x}\n\nifdef::a\u{200C}b[]\ndefined\nendif::[]\n",
        &options,
    )?;
    assert_eq!(paragraphs(&parsed), expected);
    assert_eq!(
        name_warnings(&parsed),
        [(
            "a\u{200C}b".to_owned(),
            Some("ab".to_owned()),
            mode == CompatibilityMode::Asciidoctor,
            Some(1)
        )]
    );
    assert_eq!(parsed.document().attributes().compatibility_mode(), mode);
    Ok(())
}

/// Every reference context reads join controls by the same rule.
#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor, "{a\u{200C}b}")]
#[case::draft(CompatibilityMode::Draft, "v")]
fn every_reference_context_reads_join_controls_by_mode(
    #[case] mode: CompatibilityMode,
    #[case] value: &str,
) -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(mode)
        .with_attribute("a\u{200C}b", "v")
        .build()?;
    let source = ":p: pass:a[{a\u{200C}b}]\n\n{p}\n\n\\+{a\u{200C}b}+\n\n.Title {a\u{200C}b}\n[source,role={a\u{200C}b},subs=attributes+]\n----\nraw {a\u{200C}b}\n----\n";
    let parsed = parse(source, &options)?;
    assert_eq!(
        paragraphs(&parsed),
        [value.to_owned(), format!("+{value}+")]
    );
    let listing = parsed
        .document()
        .blocks()
        .find_map(|block| {
            if let BlockKind::DelimitedListing(listing) = block.kind() {
                Some(listing)
            } else {
                None
            }
        })
        .ok_or("listing")?;
    assert_eq!(
        text(listing.title().ok_or("title")?),
        format!("Title {value}")
    );
    assert!(listing.roles().contains(value));
    assert_eq!(text(listing.content()), format!("raw {value}"));
    Ok(())
}

/// A document expands references by its own compatibility mode, with values
/// from the caller's lookup.
#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor, "{a\u{200C}b}|v|{-x}|{missing}")]
#[case::draft(CompatibilityMode::Draft, "v|v|{-x}|{missing}")]
fn documents_expand_references_by_their_mode(
    #[case] mode: CompatibilityMode,
    #[case] expected: &str,
) -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(mode)
        .with_attribute("a\u{200C}b", "v")
        .with_attribute("ab", "v")
        .with_attribute("-x", "dash")
        .build()?;
    let parsed = parse("Text.\n", &options)?;
    let attributes = parsed.document().attributes();
    let mut names = Vec::new();
    let expanded = attributes.substitute_attributes("{a\u{200C}b}|{ab}|{-x}|{missing}", |name| {
        names.push(name.to_owned());
        attributes.get(name)
    });
    assert_eq!(expanded, expected);
    let mut expected_names = vec!["ab", "missing"];
    if mode == CompatibilityMode::Draft {
        expected_names.insert(0, "a\u{200C}b");
    }
    assert_eq!(names, expected_names);
    Ok(())
}
