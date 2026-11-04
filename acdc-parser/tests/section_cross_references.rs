//! What the parser records so that `xrefstyle` can reach numbered sections:
//! each section's cross-reference name and number in the catalog, and on each
//! reference the `<name>-refsig` word in effect where it is written. Also what
//! an `xref:` macro's attribute list sets for its one reference.
//!
//! The expectations follow Asciidoctor, whose `Section#xreftext` builds
//! `Section 1.1` and `Chapter 2, _Title_` from exactly these parts.

use acdc_parser::{
    Options, XrefSignifier, XrefStyle,
    ast::{BlockKind, BlockRef, CrossReferenceRef, InlineKind, ReferenceRef, ReferencesRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

/// Every cross-reference in the document's paragraphs, in source order.
fn cross_references<'a>(blocks: impl Iterator<Item = BlockRef<'a>>) -> Vec<CrossReferenceRef<'a>> {
    let mut found = Vec::new();
    for block in blocks {
        if let BlockKind::Paragraph(paragraph) = block.kind() {
            for node in paragraph.content().inlines() {
                if let InlineKind::CrossReference(xref) = node.kind() {
                    found.push(xref);
                }
            }
        } else if let BlockKind::Section(section) = block.kind() {
            found.extend(cross_references(section.blocks()));
        }
    }
    found
}

fn section_reference<'a>(
    references: ReferencesRef<'a>,
    id: &str,
) -> Result<ReferenceRef<'a>, Error> {
    references
        .get(id)
        .filter(|reference| reference.section_name().is_some())
        .ok_or_else(|| format!("no section reference for {id}").into())
}

#[test]
fn a_section_reference_carries_its_name_and_number() -> Result<(), Error> {
    let source = "= Book\n:doctype: book\n:sectnums:\n:partnums:\n\n[preface]\n== Preface\n\n\
                  [[p1]]\n= Part One\n\n[[ch1]]\n== Chapter\n\n[[s11]]\n=== Section\n\nx\n\n\
                  [appendix]\n[[app]]\n== Appendix\n\n[[appsub]]\n=== Appendix Sub\n\nx\n";
    let parsed = parse(source, &Options::default())?;
    let references = parsed.document_ref().references();

    for (id, name, number) in [
        ("p1", "part", "I"),
        ("ch1", "chapter", "1"),
        ("s11", "section", "1.1"),
        ("app", "appendix", "A"),
        ("appsub", "section", "A.1"),
    ] {
        let section = section_reference(references, id)?;
        assert_eq!(section.section_name(), Some(name), "{id}");
        assert_eq!(section.section_number(), Some(number), "{id}");
    }
    Ok(())
}

#[test]
fn an_article_level_one_section_is_a_section_not_a_chapter() -> Result<(), Error> {
    let parsed = parse(
        "= A\n:sectnums:\n\n[[s1]]\n== One\n\nx\n",
        &Options::default(),
    )?;
    let section = section_reference(parsed.document_ref().references(), "s1")?;
    assert_eq!(section.section_name(), Some("section"));
    Ok(())
}

#[test]
fn an_unnumbered_section_has_no_reference_number() -> Result<(), Error> {
    // Without sectnums, and after sectnums is turned off, a reference falls
    // back to the title, so there is no number to quote.
    let source = "= A\n\n[[plain]]\n== Plain\n\nx\n\n:sectnums:\n\n[[numbered]]\n== Numbered\n\nx\n\n\
                  :sectnums!:\n\n[[off]]\n== Off Again\n\nx\n";
    let parsed = parse(source, &Options::default())?;
    let references = parsed.document_ref().references();
    assert_eq!(
        section_reference(references, "plain")?.section_number(),
        None
    );
    assert_eq!(
        section_reference(references, "numbered")?.section_number(),
        Some("1")
    );
    assert_eq!(section_reference(references, "off")?.section_number(), None);
    Ok(())
}

#[test]
fn a_part_is_numbered_only_with_partnums() -> Result<(), Error> {
    let source = "= B\n:doctype: book\n:sectnums:\n\n[preface]\n== P\n\n[[p1]]\n= Part One\n\n\
                  [[c1]]\n== C1\n\nx\n";
    let parsed = parse(source, &Options::default())?;
    let references = parsed.document_ref().references();
    assert_eq!(section_reference(references, "p1")?.section_number(), None);
    assert_eq!(
        section_reference(references, "c1")?.section_number(),
        Some("1")
    );
    Ok(())
}

#[test]
fn a_reference_is_numbered_past_sectnumlevels_though_the_heading_is_not() -> Result<(), Error> {
    // Asciidoctor prints the level-2 heading bare but still references it as
    // `Section 1.1`; the table of contents follows the heading.
    let source = "= A\n:sectnums:\n:sectnumlevels: 1\n\n[[s1]]\n== S1\n\n[[s11]]\n=== S11\n\nx\n";
    let parsed = parse(source, &Options::default())?;
    let document = parsed.document_ref();

    assert_eq!(
        section_reference(document.references(), "s11")?.section_number(),
        Some("1.1")
    );
    let toc_entry = document
        .toc_entries()
        .find(|entry| entry.id() == "s11")
        .ok_or("no TOC entry for s11")?;
    assert_eq!(toc_entry.number(), None);
    Ok(())
}

#[test]
fn a_numbered_special_section_takes_its_style_as_its_name() -> Result<(), Error> {
    let source = "= B\n:doctype: book\n:sectnums: all\n\n[preface]\n[[pre]]\n== Preface\n\nx\n\n\
                  [[ch]]\n== Chapter\n\nx\n";
    let parsed = parse(source, &Options::default())?;
    let preface = section_reference(parsed.document_ref().references(), "pre")?;
    assert_eq!(preface.section_name(), Some("preface"));
    assert_eq!(preface.section_number(), Some("1"));
    Ok(())
}

#[test]
fn the_signifier_is_read_where_the_reference_is_written() -> Result<(), Error> {
    let source = "= A\n:sectnums:\n\nDefault: <<s1>>\n\n:section-refsig: Abschnitt\n\n\
                  Changed: <<s1>>\n\n:section-refsig:\n\nEmpty: <<s1>>\n\n:section-refsig!:\n\n\
                  Unset: <<s1>>\n\n[[s1]]\n== S1\n\nx\n";
    let parsed = parse(source, &Options::default())?;
    let signifiers: Vec<_> = cross_references(parsed.document_ref().blocks())
        .into_iter()
        .map(CrossReferenceRef::signifier)
        .collect();
    assert_eq!(
        signifiers,
        [
            XrefSignifier::AtReference("Section"),
            XrefSignifier::AtReference("Abschnitt"),
            XrefSignifier::AtReference(""),
            XrefSignifier::Omitted,
        ]
    );
    Ok(())
}

#[test]
fn the_xref_macro_reads_its_brackets_as_an_attribute_list() -> Result<(), Error> {
    let source = "= D\n\n\
                  xref:fig[xrefstyle=short]\n\n\
                  xref:fig[Custom,xrefstyle=full]\n\n\
                  xref:fig[xrefstyle=short,Words]\n\n\
                  xref:fig[\"Quoted, text\",xrefstyle=short]\n\n\
                  xref:fig[text, with comma]\n\n\
                  xref:fig[1+1=2]\n\n\
                  xref:fig[role=r]\n\n\
                  xref:fig[Text,role=\"a b\",xrefstyle=short]\n\n\
                  xref:fig[role=r,role='s']\n\n\
                  <<fig,role=r>>\n\n\
                  [[fig]]\n.A figure\nimage::f.png[]\n";
    let parsed = parse(source, &Options::default())?;
    let xrefs = cross_references(parsed.document_ref().blocks());
    let seen: Vec<_> = xrefs
        .iter()
        .map(|xref| {
            let text: String = xref
                .text()
                .inlines()
                .map(|node| {
                    if let InlineKind::Plain(plain) = node.kind() {
                        plain.text()
                    } else {
                        "?"
                    }
                })
                .collect();
            (
                text,
                xref.xrefstyle(),
                xref.roles().iter().collect::<Vec<_>>(),
            )
        })
        .collect();

    assert_eq!(
        seen,
        [
            // A named attribute alone leaves the text automatic.
            (String::new(), XrefStyle::Short, vec![]),
            // The first positional attribute is the link text.
            ("Custom".to_string(), XrefStyle::Full, vec![]),
            // Text after a named attribute is not the first positional one.
            (String::new(), XrefStyle::Short, vec![]),
            ("Quoted, text".to_string(), XrefStyle::Short, vec![]),
            // Without an `=`, the brackets are the text as written.
            ("text, with comma".to_string(), XrefStyle::Default, vec![]),
            // `1+1` is no attribute name, so this is text too.
            ("1+1=2".to_string(), XrefStyle::Default, vec![]),
            // `role=` is kept for the link, unquoted, and the last one wins.
            (String::new(), XrefStyle::Default, vec!["r"]),
            ("Text".to_string(), XrefStyle::Short, vec!["a", "b"]),
            (String::new(), XrefStyle::Default, vec!["s"]),
            // The shorthand has no attribute list, so this is its text.
            ("role=r".to_string(), XrefStyle::Default, vec![]),
        ]
    );
    Ok(())
}
