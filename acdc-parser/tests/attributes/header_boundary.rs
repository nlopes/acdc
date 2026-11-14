//! Where the document header ends in each compatibility mode.
//!
//! Asciidoctor mode follows asciidoctor: empty lines before the document
//! title are ignored, so attribute entries above the title, or in a document
//! without one, stay in the header. Draft mode follows the language docs: the
//! first empty line ends the header, with or without a title. After the title,
//! the first empty line ends the header in both modes.

use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, BlockRef, ContentRef, ElementIdRef, HeaderRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

const ASCIIDOCTOR: CompatibilityMode = CompatibilityMode::Asciidoctor;
const DRAFT: CompatibilityMode = CompatibilityMode::Draft;

fn options(mode: CompatibilityMode) -> Result<Options<'static>, Error> {
    Ok(Options::builder().with_compatibility_mode(mode).build()?)
}

/// The observable header boundary: the document title, the attribute entries
/// in the end-of-header snapshot, the body attribute entries in source order,
/// and the first content block.
#[derive(Debug, PartialEq)]
struct Boundary {
    title: Option<String>,
    header: Vec<String>,
    body: Vec<String>,
    first: Option<String>,
}

fn expected(title: Option<&str>, header: &[&str], body: &[&str], first: Option<&str>) -> Boundary {
    Boundary {
        title: title.map(str::to_owned),
        header: header.iter().map(|name| (*name).to_owned()).collect(),
        body: body.iter().map(|name| (*name).to_owned()).collect(),
        first: first.map(str::to_owned),
    }
}

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    let mut result = String::new();
    for inline in content.inlines() {
        let InlineKind::Plain(node) = inline.kind() else {
            return Err("unexpected title content".into());
        };
        result.push_str(node.text());
    }
    Ok(result)
}

fn label(block: BlockRef<'_>) -> Result<String, Error> {
    let kind = block.kind();
    Ok(if let BlockKind::Section(section) = kind {
        format!(
            "section {}: {}",
            section.level(),
            text(section.title().ok_or("section title")?)?
        )
    } else if matches!(kind, BlockKind::Paragraph(_)) {
        "paragraph".to_owned()
    } else if matches!(kind, BlockKind::Comment(_)) {
        "comment".to_owned()
    } else {
        "other".to_owned()
    })
}

/// Read the boundary, considering only the `names` the case declares.
fn boundary(source: &str, mode: CompatibilityMode, names: &[&str]) -> Result<Boundary, Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document();
    let title = document
        .header()
        .and_then(HeaderRef::title)
        .map(text)
        .transpose()?;
    let snapshot = document.attributes();
    let header = names
        .iter()
        .filter(|name| snapshot.is_explicit(name))
        .map(|name| (*name).to_owned())
        .collect();
    let mut body = Vec::new();
    let mut first = None;
    for block in document.blocks() {
        if let BlockKind::DocumentAttribute(attribute) = block.kind() {
            body.push(attribute.name().to_owned());
        } else if first.is_none() {
            first = Some(label(block)?);
        }
    }
    Ok(Boundary {
        title,
        header,
        body,
        first,
    })
}

fn assert_boundary(
    source: &str,
    names: &[&str],
    asciidoctor: &Boundary,
    draft: &Boundary,
) -> Result<(), Error> {
    assert_eq!(
        &boundary(source, ASCIIDOCTOR, names)?,
        asciidoctor,
        "asciidoctor mode"
    );
    assert_eq!(&boundary(source, DRAFT, names)?, draft, "draft mode");
    Ok(())
}

#[test]
fn attribute_entry_before_title_keeps_the_title_only_in_asciidoctor_mode() -> Result<(), Error> {
    assert_boundary(
        ":foo: bar\n\n= Title\n\nBody\n",
        &["foo"],
        &expected(Some("Title"), &["foo"], &[], Some("paragraph")),
        &expected(None, &["foo"], &[], Some("section 0: Title")),
    )
}

#[test]
fn untitled_header_spans_one_empty_line_only_in_asciidoctor_mode() -> Result<(), Error> {
    assert_boundary(
        ":a: 1\n\n:c: 3\n\nPara\n",
        &["a", "c"],
        &expected(None, &["a", "c"], &[], Some("paragraph")),
        &expected(None, &["a"], &["c"], Some("paragraph")),
    )
}

#[test]
fn untitled_header_spans_two_empty_lines_only_in_asciidoctor_mode() -> Result<(), Error> {
    assert_boundary(
        ":a: 1\n\n\n:c: 3\n\nPara\n",
        &["a", "c"],
        &expected(None, &["a", "c"], &[], Some("paragraph")),
        &expected(None, &["a"], &["c"], Some("paragraph")),
    )
}

#[test]
fn first_empty_line_after_title_ends_the_header_in_both_modes() -> Result<(), Error> {
    let both = expected(Some("Title"), &["a"], &["c"], Some("paragraph"));
    assert_boundary(
        "= Title\n:a: 1\n\n:c: 3\n\nPara\n",
        &["a", "c"],
        &both,
        &both,
    )?;
    // Leading attribute entries and an author line do not move the boundary.
    assert_boundary(
        ":a: 1\n\n= Title\nJane Doe <jane@example.org>\n:b: 2\n\n:c: 3\n\nPara\n",
        &["a", "b", "c"],
        &expected(Some("Title"), &["a", "b"], &["c"], Some("paragraph")),
        &expected(None, &["a"], &[], Some("section 0: Title")),
    )
}

#[test]
fn untitled_attribute_entry_followed_by_paragraph() -> Result<(), Error> {
    let both = expected(None, &["a"], &[], Some("paragraph"));
    assert_boundary(":a: 1\n\nPara\n", &["a"], &both, &both)
}

const GROUP_NAMES: &[&str] = &[
    "description",
    "doctype",
    "toc",
    "sectnums",
    "icons",
    "source-highlighter",
];

#[test]
fn grouped_attributes_after_title_leave_the_header_in_both_modes() -> Result<(), Error> {
    let both = expected(
        Some("User Guide"),
        &["description"],
        &["doctype", "toc", "sectnums", "icons", "source-highlighter"],
        Some("section 1: Install"),
    );
    assert_boundary(
        "= User Guide\n:description: How to install the product.\n\n:doctype: book\n:toc: left\n:sectnums:\n\n:icons: font\n:source-highlighter: rouge\n\n== Install\n\nText.\n",
        GROUP_NAMES,
        &both,
        &both,
    )
}

#[test]
fn grouped_attributes_with_comment_lines_stay_in_the_header_in_both_modes() -> Result<(), Error> {
    let both = expected(
        Some("User Guide"),
        GROUP_NAMES,
        &[],
        Some("section 1: Install"),
    );
    assert_boundary(
        "= User Guide\n:description: How to install the product.\n// layout\n:doctype: book\n:toc: left\n:sectnums:\n// layout\n:icons: font\n:source-highlighter: rouge\n\n== Install\n\nText.\n",
        GROUP_NAMES,
        &both,
        &both,
    )
}

#[test]
fn grouped_attributes_above_title_stay_in_the_header_only_in_asciidoctor_mode() -> Result<(), Error>
{
    assert_boundary(
        ":description: How to install the product.\n\n:doctype: book\n:toc: left\n\n= User Guide\n\n== Install\n\nText.\n",
        &["description", "doctype", "toc"],
        &expected(
            Some("User Guide"),
            &["description", "doctype", "toc"],
            &[],
            Some("section 1: Install"),
        ),
        &expected(
            None,
            &["description"],
            &["doctype", "toc"],
            Some("section 0: User Guide"),
        ),
    )
}

#[test]
fn comments_and_empty_lines_before_title() -> Result<(), Error> {
    assert_boundary(
        ":a: 1\n\n// note\n\n////\nblock\n\nmore\n////\n\n:b: 2\n\n= Title\n\nBody\n",
        &["a", "b"],
        &expected(Some("Title"), &["a", "b"], &[], Some("paragraph")),
        &expected(None, &["a"], &["b"], Some("comment")),
    )
}

#[test]
fn block_metadata_and_empty_lines_before_title() -> Result<(), Error> {
    let source = "[[top]]\n\n:a: 1\n\n[role=lead]\n\n= Title\n\nBody\n";
    assert_boundary(
        source,
        &["a"],
        &expected(Some("Title"), &["a"], &[], Some("paragraph")),
        &expected(None, &[], &["a"], Some("section 0: Title")),
    )?;
    let parsed = parse(source, &Options::default())?;
    let header = parsed.document().header().ok_or("header")?;
    assert_eq!(header.id().map(ElementIdRef::as_str), Some("top"));
    assert!(header.roles().contains("lead"));
    Ok(())
}

#[test]
fn attribute_entry_between_anchor_and_title() -> Result<(), Error> {
    assert_boundary(
        "[[top]]\n:a: 1\n= Title\n\nBody\n",
        &["a"],
        &expected(Some("Title"), &["a"], &[], Some("paragraph")),
        &expected(None, &[], &["a"], Some("section 0: Title")),
    )
}

#[test]
fn block_title_above_title_is_not_a_document_title() -> Result<(), Error> {
    // asciidoctor carries the block title to the body instead.
    let both = expected(None, &["a"], &[], Some("section 0: Title"));
    assert_boundary(":a: 1\n\n.Foo\n= Title\n\nBody\n", &["a"], &both, &both)
}

#[test]
fn attribute_references_resolve_in_source_order_before_title() -> Result<(), Error> {
    assert_boundary(
        ":a: 1\n:b: {a}2\n\n:c: {b}3\n\n= T {c}\n\nPara\n",
        &["a", "b", "c"],
        &expected(Some("T 123"), &["a", "b", "c"], &[], Some("paragraph")),
        &expected(None, &["a", "b"], &["c"], Some("section 0: T 123")),
    )
}

#[test]
fn whitespace_only_line_counts_as_empty() -> Result<(), Error> {
    assert_boundary(
        ":a: 1\n   \n:b: 2\n\nPara\n",
        &["a", "b"],
        &expected(None, &["a", "b"], &[], Some("paragraph")),
        &expected(None, &["a"], &["b"], Some("paragraph")),
    )
}

#[test]
fn untitled_header_comment_after_empty_line() -> Result<(), Error> {
    // Comments are part of the header in Asciidoctor mode and are not retained.
    assert_boundary(
        ":a: 1\n\n// about the paragraph\nPara\n",
        &["a"],
        &expected(None, &["a"], &[], Some("paragraph")),
        &expected(None, &["a"], &[], Some("comment")),
    )
}

#[test]
fn untitled_header_without_body() -> Result<(), Error> {
    for source in [":a: 1\n\n:b: 2", ":a: 1\n\n:b: 2\n\n\n"] {
        assert_boundary(
            source,
            &["a", "b"],
            &expected(None, &["a", "b"], &[], None),
            &expected(None, &["a"], &["b"], None),
        )?;
    }
    Ok(())
}

#[test]
fn untitled_header_before_level_one_section() -> Result<(), Error> {
    assert_boundary(
        ":toc:\n\n:sectnums:\n\n== One\n\nText\n",
        &["toc", "sectnums"],
        &expected(None, &["toc", "sectnums"], &[], Some("section 1: One")),
        &expected(None, &["toc"], &["sectnums"], Some("section 1: One")),
    )
}

#[test]
fn book_title_above_part_after_empty_line() -> Result<(), Error> {
    assert_boundary(
        ":doctype: book\n\n= Book\n\n= Part One\n\n== Chapter\n\nText\n",
        &["doctype"],
        &expected(Some("Book"), &["doctype"], &[], Some("section 0: Part One")),
        &expected(None, &["doctype"], &[], Some("section 0: Book")),
    )
}

#[test]
fn conditional_entries_before_title() -> Result<(), Error> {
    assert_boundary(
        ":a: 1\n\nifdef::a[]\n:b: 2\nendif::[]\n\n= Title\n\nPara\n",
        &["a", "b"],
        &expected(Some("Title"), &["a", "b"], &[], Some("paragraph")),
        &expected(None, &["a"], &["b"], Some("section 0: Title")),
    )?;
    // An excluded region removes its empty line before the header boundary applies.
    let both = expected(None, &["a", "b"], &[], Some("paragraph"));
    assert_boundary(
        ":a: 1\nifdef::nope[]\n\nendif::[]\n:b: 2\n\nPara\n",
        &["a", "b"],
        &both,
        &both,
    )
}

/// Asciidoctor keeps an attribute entry that follows block metadata in the
/// header of a document without a title. acdc applies it in the body in both
/// modes. This records the current behavior.
#[test]
fn untitled_attribute_entry_after_block_metadata_stays_in_body() -> Result<(), Error> {
    let both = expected(None, &["a"], &["b"], Some("paragraph"));
    assert_boundary(":a: 1\n\n[[x]]\n:b: 2\n\nPara\n", &["a", "b"], &both, &both)
}

#[test]
fn title_footnote_is_registered_once() -> Result<(), Error> {
    for source in [
        "= Title footnote:[Header note.]\n\nBody\n",
        "[[top]]\n= Title footnote:[Header note.]\n\nBody\n",
        ":a: 1\n\n[[top]]\n\n= Title footnote:[Header note.]\n\nBody\n",
    ] {
        for mode in [ASCIIDOCTOR, DRAFT] {
            let parsed = parse(
                source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            assert_eq!(parsed.document().footnotes().count(), 1, "{mode}: {source}");
        }
    }
    Ok(())
}

#[rstest::rstest]
fn empty_lines_before_title_follow_mode(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(1, 2, 5)] blanks: usize,
    #[values(":a: 1", "[[top]]", "[#top.role]", ":a: 1\n// comment")] prefix: &str,
) -> Result<(), Error> {
    let source = format!("{prefix}\n{}= Title\n\nPara\n", "\n".repeat(blanks));
    let parsed = parse(&source, &options(mode)?)?;
    let doc = parsed.document();
    let compatible = mode == CompatibilityMode::Asciidoctor;
    assert_eq!(doc.header().is_some(), compatible, "{source}");
    if compatible {
        assert!(matches!(
            doc.blocks().next().ok_or("body")?.kind(),
            BlockKind::Paragraph(_)
        ));
        if prefix.contains("top") {
            assert_eq!(
                doc.header().ok_or("header")?.id().ok_or("id")?.as_str(),
                "top"
            );
        }
        assert_eq!(doc.header_location(), doc.header().map(HeaderRef::location));
    } else {
        assert!(
            doc.blocks()
                .any(|block| matches!(block.kind(), BlockKind::Section(_)))
        );
        assert_eq!(doc.header_location().is_some(), prefix.starts_with(':'));
    }
    Ok(())
}

#[rstest::rstest]
fn untitled_attributes_stop_at_mode_boundary(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(0, 1, 2, 5)] blanks: usize,
) -> Result<(), Error> {
    let source = format!(
        ":a: 1\n{}:c: 3\n\nPara\n\n:later: body\n",
        "\n".repeat(blanks)
    );
    let parsed = parse(&source, &options(mode)?)?;
    let doc = parsed.document();
    let c_in_header = blanks == 0 || mode == CompatibilityMode::Asciidoctor;
    assert!(doc.header().is_none());
    assert_eq!(doc.attributes().contains("c"), c_in_header);
    assert!(!doc.attributes().contains("later"));
    assert_eq!(
        doc.blocks()
            .filter(|block| matches!(block.kind(), BlockKind::DocumentAttribute(_)))
            .count(),
        if c_in_header { 1 } else { 2 }
    );
    let location = doc.header_location().ok_or("header span")?;
    assert_eq!(location.absolute_start(), 0);
    assert_eq!(
        location.absolute_end(),
        if c_in_header {
            source.find("3\n").ok_or("c")?
        } else {
            4
        }
    );
    assert!(
        doc.blocks()
            .any(|block| matches!(block.kind(), BlockKind::Paragraph(_)))
    );
    Ok(())
}

#[rstest::rstest]
fn titled_header_stops_at_first_empty_line(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(1, 2, 5)] blanks: usize,
) -> Result<(), Error> {
    let source = format!("= Title\n:a: 1\n{}:c: 3\n\nPara\n", "\n".repeat(blanks));
    let parsed = parse(&source, &options(mode)?)?;
    let doc = parsed.document();
    assert!(doc.header().is_some());
    assert!(doc.attributes().contains("a"));
    assert!(!doc.attributes().contains("c"));
    assert_eq!(doc.header_location().ok_or("span")?.absolute_end(), 12);
    assert!(matches!(
        doc.blocks().next().ok_or("body")?.kind(),
        BlockKind::DocumentAttribute(_)
    ));
    Ok(())
}

#[rstest::rstest]
fn atomic_comments_and_continuations_keep_header_open(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("", "= Title\n")] title: &str,
) -> Result<(), Error> {
    let source =
        format!("{title}:a: first \\\ncontinued\n//// \n:fake: hidden\n\n//// \n:b: two\n\nPara\n");
    let parsed = parse(&source, &options(mode)?)?;
    let doc = parsed.document();
    assert!(doc.attributes().contains("a"));
    assert!(doc.attributes().contains("b"));
    assert!(!doc.attributes().contains("fake"));
    assert_eq!(
        doc.header_location().ok_or("span")?.end().line(),
        if title.is_empty() { 7 } else { 8 }
    );
    Ok(())
}

#[rstest::rstest]
fn whitespace_only_lines_end_only_draft_pretitle_headers(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(" ", "\t", " \t ")] gap: &str,
) -> Result<(), Error> {
    let source = format!(":a: 1\n{gap}\n= Title\n\nPara\n");
    let parsed = parse(&source, &options(mode)?)?;
    assert_eq!(
        parsed.document().header().is_some(),
        mode == CompatibilityMode::Asciidoctor
    );
    Ok(())
}

#[rstest::rstest]
fn header_termination_preserves_author_and_revision_slots(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for prefix in [":a: 1\n", ":a: 1\n\n"] {
        let source = format!(
            "{prefix}= Title\n:before-author: yes\n// author follows\nAda Lovelace\n:before-revision: yes\nv1.2, 2026-11-14: Stable\n:after-revision: yes\n\n:body: yes\n\nPara\n"
        );
        let parsed = parse(&source, &options(mode)?)?;
        let doc = parsed.document();
        if mode == CompatibilityMode::Asciidoctor || !prefix.ends_with("\n\n") {
            assert_eq!(
                doc.header()
                    .ok_or("header")?
                    .authors()
                    .next()
                    .ok_or("author")?
                    .first_name(),
                "Ada"
            );
            for name in [
                "before-author",
                "before-revision",
                "after-revision",
                "revnumber",
            ] {
                assert!(doc.attributes().contains(name), "{name}");
            }
            assert!(!doc.attributes().contains("body"));
        } else {
            assert!(doc.header().is_none());
            assert!(!doc.attributes().contains("revnumber"));
        }
    }
    Ok(())
}
