use super::document_parser;
use crate::{
    Anchor, AttributeValue, Block, CommentKind, DelimitedBlock, DelimitedBlockType, Document,
    Error, Image, InlineMacro, InlineNode, Location, Options, Plain, Position, Section, Subtitle,
    Title, WarningKind, XrefCaptionLabel, XrefStyle,
    grammar::ParserState,
    model::{Caption, CaptionKind, SectionKind},
    parse,
};
#[cfg(feature = "setext")]
use std::rc::Rc;

#[test]
#[tracing_test::traced_test]
fn test_document() -> Result<(), Error> {
    let input = "// this comment line is ignored
= Document Title
Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>
v2.9, 01-09-2024: Fall incarnation
:description: The document's description.
:sectanchors:
:url-repo: https://my-git-repo.com";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;
    let header = result.header.expect("document has a header");
    assert_eq!(header.title.len(), 1);
    assert_eq!(
        header.title[0],
        InlineNode::PlainText(Plain {
            content: "Document Title",
            location: Location {
                absolute_start: 34,
                absolute_end: 47,
                start: Position::new(2, 3),
                end: Position::new(2, 16),
            },
            escaped: false,
        })
    );
    assert_eq!(header.authors.len(), 2);
    assert_eq!(header.authors[0].first_name, "Lorn Kismet");
    assert_eq!(header.authors[0].middle_name, Some("R."));
    assert_eq!(header.authors[0].last_name, "Lee");
    assert_eq!(header.authors[0].initials, "LRL");
    assert_eq!(header.authors[0].email, Some("kismet@asciidoctor.org"));
    assert_eq!(header.authors[1].first_name, "Norberto");
    assert_eq!(header.authors[1].middle_name, Some("M."));
    assert_eq!(header.authors[1].last_name, "Lopes");
    assert_eq!(header.authors[1].initials, "NML");
    assert_eq!(header.authors[1].email, Some("nlopesml@gmail.com"));
    assert_eq!(state.document_attributes.text("revnumber"), Some("2.9"));
    assert_eq!(
        state.document_attributes.text("revdate"),
        Some("01-09-2024")
    );
    assert_eq!(
        state.document_attributes.text("revremark"),
        Some("Fall incarnation")
    );
    assert_eq!(
        state.document_attributes.text("description"),
        Some("The document's description.")
    );
    assert!(
        state
            .document_attributes
            .get("sectanchors")
            .is_some_and(crate::DocumentAttributeValue::is_presence)
    );
    assert_eq!(
        state.document_attributes.text("url-repo"),
        Some("https://my-git-repo.com")
    );
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_authors() -> Result<(), Error> {
    let input =
        "Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::authors(input, &mut state)?;

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].first_name, "Lorn Kismet");
    assert_eq!(result[0].middle_name, Some("R."));
    assert_eq!(result[0].last_name, "Lee");
    assert_eq!(result[0].initials, "LRL");
    assert_eq!(result[0].email, Some("kismet@asciidoctor.org"));
    assert_eq!(result[1].first_name, "Norberto");
    assert_eq!(result[1].middle_name, Some("M."));
    assert_eq!(result[1].last_name, "Lopes");
    assert_eq!(result[1].initials, "NML");
    assert_eq!(result[1].email, Some("nlopesml@gmail.com"));
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_author() -> Result<(), Error> {
    let input = "Norberto M. Lopes supa dough <nlopesml@gmail.com>";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::author(input, &mut state)?;
    assert_eq!(result.first_name, "Norberto");
    assert_eq!(result.middle_name, Some("M."));
    assert_eq!(result.last_name, "Lopes supa dough");
    assert_eq!(result.initials, "NML");
    assert_eq!(result.email, Some("nlopesml@gmail.com"));
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_compound_first_name() -> Result<(), Error> {
    let input = "Ann_Marie Jenson";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::author(input, &mut state)?;
    assert_eq!(result.first_name, "Ann Marie");
    assert_eq!(result.middle_name, None);
    assert_eq!(result.last_name, "Jenson");
    assert_eq!(result.initials, "AJ");
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_compound_last_name() -> Result<(), Error> {
    let input = "Tomás López_del_Toro";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::author(input, &mut state)?;
    assert_eq!(result.first_name, "Tomás");
    assert_eq!(result.middle_name, None);
    assert_eq!(result.last_name, "López del Toro");
    assert_eq!(result.initials, "TL");
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_compound_middle_name() -> Result<(), Error> {
    let input = "First Middle_Name Last";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::author(input, &mut state)?;
    assert_eq!(result.first_name, "First");
    assert_eq!(result.middle_name, Some("Middle Name"));
    assert_eq!(result.last_name, "Last");
    assert_eq!(result.initials, "FML");
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_multiple_compound_authors() -> Result<(), Error> {
    let input = "Ann_Marie Jenson; Tomás López_del_Toro";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::authors(input, &mut state)?;
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].first_name, "Ann Marie");
    assert_eq!(result[0].last_name, "Jenson");
    assert_eq!(result[0].initials, "AJ");
    assert_eq!(result[1].first_name, "Tomás");
    assert_eq!(result[1].last_name, "López del Toro");
    assert_eq!(result[1].initials, "TL");
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_unicode_author_name() -> Result<(), Error> {
    let input = "Tomás Müller";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::author(input, &mut state)?;
    assert_eq!(result.first_name, "Tomás");
    assert_eq!(result.last_name, "Müller");
    assert_eq!(result.initials, "TM");
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_revision_full() -> Result<(), Error> {
    let input = "v2.9, 01-09-2024: Fall incarnation";
    let mut state = ParserState::new_for_test(input);
    document_parser::revision(input, &mut state)?;
    assert_eq!(state.document_attributes.text("revnumber"), Some("2.9"));
    assert_eq!(
        state.document_attributes.text("revdate"),
        Some("01-09-2024")
    );
    assert_eq!(
        state.document_attributes.text("revremark"),
        Some("Fall incarnation")
    );
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_revision_with_date_no_remark() -> Result<(), Error> {
    let input = "v2.9, 01-09-2024";
    let mut state = ParserState::new_for_test(input);
    document_parser::revision(input, &mut state)?;
    assert_eq!(state.document_attributes.text("revnumber"), Some("2.9"));
    assert_eq!(
        state.document_attributes.text("revdate"),
        Some("01-09-2024")
    );
    assert_eq!(state.document_attributes.get("revremark"), None);
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_revision_no_date_with_remark() -> Result<(), Error> {
    let input = "v2.9: Fall incarnation";
    let mut state = ParserState::new_for_test(input);
    document_parser::revision(input, &mut state)?;
    assert_eq!(state.document_attributes.text("revnumber"), Some("2.9"));
    assert_eq!(state.document_attributes.get("revdate"), None);
    assert_eq!(
        state.document_attributes.text("revremark"),
        Some("Fall incarnation")
    );
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_revision_no_date_no_remark() -> Result<(), Error> {
    let input = "v2.9";
    let mut state = ParserState::new_for_test(input);
    document_parser::revision(input, &mut state)?;
    assert_eq!(state.document_attributes.text("revnumber"), Some("2.9"));
    assert_eq!(state.document_attributes.get("revdate"), None);
    assert_eq!(state.document_attributes.get("revremark"), None);
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_comment_between_author_and_revision() -> Result<(), Error> {
    // asciidoctor skips a line comment between the author line and the
    // revision line and still reads the revision (and following attributes).
    let input = "= T
Roberto Avanzi
// a comment
v2.0, 2026-01-15: rel
:foo: bar";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;
    let header = result.header.expect("document has a header");
    assert_eq!(header.authors.len(), 1);
    assert_eq!(header.authors[0].first_name, "Roberto");
    assert_eq!(state.document_attributes.text("revnumber"), Some("2.0"));
    assert_eq!(
        state.document_attributes.text("revdate"),
        Some("2026-01-15")
    );
    assert_eq!(state.document_attributes.text("foo"), Some("bar"));
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_authorcount_defaults_to_zero_without_author() -> Result<(), Error> {
    let input = "= T\n\nbody";
    let mut state = ParserState::new_for_test(input);
    document_parser::document(input, &mut state)??;
    assert_eq!(state.document_attributes.text("authorcount"), Some("0"));
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_document_title() -> Result<(), Error> {
    let input = "= Document Title";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document_title(input, &mut state)?;
    assert_eq!(result.0.len(), 1);
    assert_eq!(
        result.0[0],
        InlineNode::PlainText(Plain {
            content: "Document Title",
            location: Location {
                absolute_start: 2,
                absolute_end: 15,
                start: Position::new(1, 3),
                end: Position::new(1, 16),
            },
            escaped: false,
        })
    );
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_document_title_and_subtitle() -> Result<(), Error> {
    let input = "= Document Title: And a subtitle";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document_title(input, &mut state)?;
    assert_eq!(
        result,
        (
            Title::new(vec![InlineNode::PlainText(Plain {
                content: "Document Title",
                location: Location {
                    absolute_start: 2,
                    absolute_end: 15,
                    start: Position::new(1, 3),
                    end: Position::new(1, 16),
                },
                escaped: false,
            })]),
            Some(Subtitle::new(vec![InlineNode::PlainText(Plain {
                content: "And a subtitle",
                location: Location {
                    absolute_start: 18,
                    absolute_end: 31,
                    start: Position::new(1, 19),
                    end: Position::new(1, 32),
                },
                escaped: false,
            })]))
        )
    );
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_header_with_title_and_authors() -> Result<(), Error> {
    let input = "= Document Title
Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::header(input, &mut state)??.expect("header should be present");
    assert_eq!(result.title.len(), 1);
    assert_eq!(
        result.title[0],
        InlineNode::PlainText(Plain {
            content: "Document Title",
            location: Location {
                absolute_start: 2,
                absolute_end: 15,
                start: Position::new(1, 3),
                end: Position::new(1, 16),
            },
            escaped: false,
        })
    );
    assert_eq!(result.authors.len(), 2);
    assert_eq!(result.authors[0].first_name, "Lorn Kismet");
    assert_eq!(result.authors[0].middle_name, Some("R."));
    assert_eq!(result.authors[0].last_name, "Lee");
    assert_eq!(result.authors[0].initials, "LRL");
    assert_eq!(result.authors[0].email, Some("kismet@asciidoctor.org"));
    assert_eq!(result.authors[1].first_name, "Norberto");
    assert_eq!(result.authors[1].middle_name, Some("M."));
    assert_eq!(result.authors[1].last_name, "Lopes");
    assert_eq!(result.authors[1].initials, "NML");
    assert_eq!(result.authors[1].email, Some("nlopesml@gmail.com"));
    Ok(())
}

/// A document whose only content is a title (no body, no following blank
/// line) is recognised as the doctitle, not a level-0 section. The
/// preprocessor strips the trailing newline, so the title sits at EOF — the
/// `title_authors` rule must accept end-of-input, not only a following `\n`.
/// Matches asciidoctor, which treats a lone `= Title` as the doctitle.
#[test]
fn test_title_only_document_is_doctitle() -> Result<(), Error> {
    // No trailing newline: mirrors the post-preprocessor buffer for a
    // single-line `= Doc Title\n` source.
    let input = "= Doc Title";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let header = doc.header.expect("title-only doc should have a header");
    assert_eq!(header.title.len(), 1);
    assert_eq!(
        header.title[0],
        InlineNode::PlainText(Plain {
            content: "Doc Title",
            location: Location {
                absolute_start: 2,
                absolute_end: 10,
                start: Position::new(1, 3),
                end: Position::new(1, 11),
            },
            escaped: false,
        })
    );
    assert!(
        doc.blocks.is_empty(),
        "title-only doc should have no body blocks, got: {:?}",
        doc.blocks
    );
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_document_empty_attribute_list() -> Result<(), Error> {
    let input = "[]";
    let mut state = ParserState::new_for_test(input);
    let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
    assert!(!discrete); // Not discrete
    assert_eq!(metadata.id, None);
    assert_eq!(metadata.style, None);
    assert_eq!(metadata.roles, [] as [&str; 0]);
    assert_eq!(metadata.options, [] as [&str; 0]);
    assert!(metadata.attributes.is_empty());
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_document_empty_attribute_list_with_discrete() -> Result<(), Error> {
    let input = "[discrete]";
    let mut state = ParserState::new_for_test(input);
    let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
    assert!(discrete); // Should be discrete
    assert_eq!(metadata.id, None);
    // The `discrete` style is retained so a discrete heading renders it as a class.
    assert_eq!(metadata.style, Some("discrete"));
    assert_eq!(metadata.roles, [] as [&str; 0]);
    assert_eq!(metadata.options, [] as [&str; 0]);
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_document_attribute_with_id() -> Result<(), Error> {
    let input = "[id=my-id,role=admin,options=read,options=write]";
    let mut state = ParserState::new_for_test(input);
    let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
    assert!(!discrete); // Not discrete
    assert_eq!(
        metadata.id,
        Some(Anchor {
            id: "my-id",
            xreflabel: None,
            location: Location {
                absolute_start: 4,
                absolute_end: 9,
                start: Position::new(1, 5),
                end: Position::new(1, 10),
            },
            bibliography_label: None,
            bibliography: false,
        })
    );
    assert_eq!(metadata.style, None);
    assert!(metadata.roles.contains(&"admin"));
    assert!(metadata.options.contains(&"read"));
    assert!(metadata.options.contains(&"write"));
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_document_attribute_with_id_mixed() -> Result<(), Error> {
    let input = "[astyle#myid.admin,options=read,options=write]";
    let mut state = ParserState::new_for_test(input);
    let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
    assert!(!discrete); // Not discrete
    assert_eq!(
        metadata.id,
        Some(Anchor {
            id: "myid",
            xreflabel: None,
            location: Location {
                absolute_start: 8,
                absolute_end: 12,
                start: Position::new(1, 9),
                end: Position::new(1, 13),
            },
            bibliography_label: None,
            bibliography: false,
        })
    );
    assert_eq!(metadata.style, Some("astyle"));
    assert!(metadata.roles.contains(&"admin"));
    assert!(metadata.options.contains(&"read"));
    assert!(metadata.options.contains(&"write"));
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_document_attribute_with_id_mixed_with_quotes() -> Result<(), Error> {
    let input = "[astyle#myid.admin,options=\"read,write\"]";
    let mut state = ParserState::new_for_test(input);
    let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
    assert!(!discrete); // Not discrete
    assert_eq!(
        metadata.id,
        Some(Anchor {
            id: "myid",
            xreflabel: None,
            location: Location {
                absolute_start: 8,
                absolute_end: 12,
                start: Position::new(1, 9),
                end: Position::new(1, 13),
            },
            bibliography_label: None,
            bibliography: false,
        })
    );
    assert_eq!(metadata.style, Some("astyle"));
    assert!(metadata.roles.contains(&"admin"));
    assert!(metadata.options.contains(&"read"));
    assert!(metadata.options.contains(&"write"));
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_shorthand_id_role_combined() -> Result<(), Error> {
    let input = "[#bracket-id.some-role]";
    let mut state = ParserState::new_for_test(input);
    let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
    assert!(!discrete);
    assert_eq!(
        metadata.id,
        Some(Anchor {
            id: "bracket-id",
            xreflabel: None,
            location: Location {
                absolute_start: 2,
                absolute_end: 12,
                start: Position::new(1, 3),
                end: Position::new(1, 13),
            },
            bibliography_label: None,
            bibliography: false,
        })
    );
    assert_eq!(metadata.style, None);
    assert!(metadata.roles.contains(&"some-role"));
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_shorthand_id_role_option_combined() -> Result<(), Error> {
    let input = "[#my-id.my-role%my-option]";
    let mut state = ParserState::new_for_test(input);
    let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
    assert!(!discrete);
    assert_eq!(
        metadata.id,
        Some(Anchor {
            id: "my-id",
            xreflabel: None,
            location: Location {
                absolute_start: 2,
                absolute_end: 7,
                start: Position::new(1, 3),
                end: Position::new(1, 8),
            },
            bibliography_label: None,
            bibliography: false,
        })
    );
    assert_eq!(metadata.style, None);
    assert!(metadata.roles.contains(&"my-role"));
    assert!(metadata.options.contains(&"my-option"));
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_shorthand_multiple_roles() -> Result<(), Error> {
    let input = "[#my-id.role-one.role-two]";
    let mut state = ParserState::new_for_test(input);
    let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
    assert!(!discrete);
    assert_eq!(metadata.id.as_ref().map(|a| a.id), Some("my-id"));
    assert!(metadata.roles.contains(&"role-one"));
    assert!(metadata.roles.contains(&"role-two"));
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_shorthand_style_id_role() -> Result<(), Error> {
    let input = "[quote#my-id.my-role]";
    let mut state = ParserState::new_for_test(input);
    let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
    assert!(!discrete);
    assert_eq!(metadata.id.as_ref().map(|a| a.id), Some("my-id"));
    assert_eq!(metadata.style, Some("quote"));
    assert!(metadata.roles.contains(&"my-role"));
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_shorthand_just_roles() -> Result<(), Error> {
    let input = "[.role-one.role-two]";
    let mut state = ParserState::new_for_test(input);
    let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
    assert!(!discrete);
    assert_eq!(metadata.id, None);
    assert!(metadata.roles.contains(&"role-one"));
    assert!(metadata.roles.contains(&"role-two"));
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_toc_simple() -> Result<(), Error> {
    let input =
        "= Document Title\n\n== Section 1\n\nSome content.\n\n== Section 2\n\nMore content.";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;

    assert_eq!(result.toc_entries.len(), 2);
    assert_eq!(result.toc_entries[0].level, 1);
    assert_eq!(result.toc_entries[0].id, "_section_1");
    assert_eq!(result.toc_entries[1].level, 1);
    assert_eq!(result.toc_entries[1].id, "_section_2");
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_toc_tree() -> Result<(), Error> {
    let input = "= Document Title\n\n== Section A\n\nContent A.\n\n=== Section A.1\n\nContent A.1\n\n== Section B\n\nContent B.";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;

    assert_eq!(result.toc_entries.len(), 3);
    assert_eq!(result.toc_entries[0].id, "_section_a");
    assert_eq!(result.toc_entries[1].id, "_section_a_1");
    assert_eq!(result.toc_entries[2].id, "_section_b");
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_section_kind_classifies_special_sections() -> Result<(), Error> {
    // A plain subsection keeps its own `Normal` kind. The numbering pass
    // handles any suppression inherited from a special parent.
    let input = "= Title\n\n[preface]\n== Introduction\n\nintro\n\n=== Features\n\nfeatures\n\n== Real Chapter\n\ntext";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;

    let mut sections = Vec::new();
    fn collect<'a, 'b>(blocks: &'b [Block<'a>], out: &mut Vec<&'b Section<'a>>) {
        for block in blocks {
            if let Block::Section(s) = block {
                out.push(s);
                collect(&s.content, out);
            }
        }
    }
    collect(&result.blocks, &mut sections);
    assert_eq!(sections.len(), 3);
    assert_eq!(sections[0].kind, SectionKind::Preface); // Introduction
    assert_eq!(sections[1].kind, SectionKind::Normal); // Features (plain subsection)
    assert_eq!(sections[2].kind, SectionKind::Normal); // Real Chapter

    // The flat TOC list carries the same per-section kinds.
    assert_eq!(result.toc_entries.len(), 3);
    assert_eq!(result.toc_entries[0].kind, SectionKind::Preface);
    assert_eq!(result.toc_entries[1].kind, SectionKind::Normal);
    assert_eq!(result.toc_entries[2].kind, SectionKind::Normal);
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_section_kind_appendix() -> Result<(), Error> {
    // `[appendix]` is classified as Appendix; its plain subsection is Normal.
    let input = "= Title\n:doctype: book\n\n[appendix]\n== App\n\napp\n\n=== App Sub\n\nsub";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;

    assert_eq!(result.toc_entries.len(), 2);
    assert_eq!(result.toc_entries[0].kind, SectionKind::Appendix);
    assert_eq!(result.toc_entries[1].kind, SectionKind::Normal);
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_toc_empty_document() -> Result<(), Error> {
    let input = "= Document Title\n\nJust some content without sections.";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;
    assert_eq!(result.toc_entries.len(), 0);
    Ok(())
}

#[cfg(feature = "setext")]
#[test]
#[tracing_test::traced_test]
fn test_setext_document_title() -> Result<(), Error> {
    let input = "Document Title
==============

Some content.
";
    let mut state = ParserState::new_for_test(input);
    Rc::make_mut(&mut state.options).setext = true;
    let result = document_parser::document(input, &mut state)??;
    let header = result.header.expect("document has a header");
    assert_eq!(header.title.len(), 1);
    assert!(
        matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Document Title")
    );
    Ok(())
}

#[cfg(feature = "setext")]
#[test]
#[tracing_test::traced_test]
fn test_setext_section() -> Result<(), Error> {
    let input = "= Document Title

Section One
-----------

Content.
";
    let mut state = ParserState::new_for_test(input);
    Rc::make_mut(&mut state.options).setext = true;
    let result = document_parser::document(input, &mut state)??;

    let section = result.blocks.iter().find_map(|b| {
        if let Block::Section(s) = b {
            Some(s)
        } else {
            None
        }
    });
    let section = section.expect("should have a section");
    assert_eq!(section.level, 1);
    assert!(
        matches!(&section.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section One")
    );
    Ok(())
}

#[cfg(feature = "setext")]
#[test]
#[tracing_test::traced_test]
fn test_setext_disabled_by_default() {
    let input = "Document Title
==============

Some content.
";
    let mut state = ParserState::new_for_test(input);
    // setext is disabled by default
    assert!(!state.options.setext);
    // Should not parse as setext title when disabled
    let result = document_parser::document(input, &mut state);
    // The document will be parsed but without recognizing the setext title
    // The title line will be parsed as a paragraph or similar
    if let Ok(Ok(doc)) = result {
        // No header should be found when setext is disabled
        assert!(doc.header.is_none());
    }
}

#[cfg(feature = "setext")]
#[test]
#[tracing_test::traced_test]
fn test_setext_single_section_per_level() -> Result<(), Error> {
    let input = "Document Title
==============

Section One
-----------

Content here.
";
    let mut state = ParserState::new_for_test(input);
    Rc::make_mut(&mut state.options).setext = true;
    let result = document_parser::document(input, &mut state)??;

    let header = result.header.expect("document has a header");
    assert!(
        matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Document Title")
    );

    let section = result
        .blocks
        .iter()
        .find_map(|b| {
            if let Block::Section(s) = b {
                Some(s)
            } else {
                None
            }
        })
        .expect("should have a section");

    assert_eq!(section.level, 1);
    assert!(
        matches!(&section.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section One")
    );

    Ok(())
}

#[cfg(feature = "setext")]
#[test]
#[tracing_test::traced_test]
fn test_setext_sibling_sections() -> Result<(), Error> {
    // Test that multiple same-level setext sections are parsed as siblings, not nested
    let input = "Document Title
==============

Section A
---------

Content A.

Section B
---------

Content B.

Section C
---------

Content C.
";
    let mut state = ParserState::new_for_test(input);
    Rc::make_mut(&mut state.options).setext = true;
    let result = document_parser::document(input, &mut state)??;

    let header = result.header.expect("document has a header");
    assert!(
        matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Document Title")
    );

    // All three sections should be at the top level (siblings, not nested)
    let sections: Vec<&Section> = result
        .blocks
        .iter()
        .filter_map(|b| {
            if let Block::Section(s) = b {
                Some(s)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(
        sections.len(),
        3,
        "should have 3 top-level sibling sections"
    );

    for (i, section) in sections.iter().enumerate() {
        assert_eq!(section.level, 1, "section {i} should be level 1");
    }

    assert!(
        matches!(&sections[0].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section A")
    );
    assert!(
        matches!(&sections[1].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section B")
    );
    assert!(
        matches!(&sections[2].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section C")
    );

    Ok(())
}

#[cfg(feature = "setext")]
#[test]
#[tracing_test::traced_test]
fn test_setext_all_underline_characters() -> Result<(), Error> {
    // Test each setext underline character individually
    // = → level 0 (document title)
    // - → level 1
    // ~ → level 2
    // ^ → level 3
    // + → level 4

    // Test level 1 with -
    let input = "= Doc\n\nLevel One\n---------\n\nContent.\n";
    let mut state = ParserState::new_for_test(input);
    Rc::make_mut(&mut state.options).setext = true;
    let result = document_parser::document(input, &mut state)??;
    let section = result
        .blocks
        .iter()
        .find_map(|b| {
            if let Block::Section(s) = b {
                Some(s)
            } else {
                None
            }
        })
        .expect("level 1 section");
    assert_eq!(section.level, 1);

    // Test level 2 with ~
    let input = "= Doc\n\nLevel Two\n~~~~~~~~~\n\nContent.\n";
    let mut state = ParserState::new_for_test(input);
    Rc::make_mut(&mut state.options).setext = true;
    let result = document_parser::document(input, &mut state)??;
    let section = result
        .blocks
        .iter()
        .find_map(|b| {
            if let Block::Section(s) = b {
                Some(s)
            } else {
                None
            }
        })
        .expect("level 2 section");
    assert_eq!(section.level, 2);

    // Test level 3 with ^
    let input = "= Doc\n\nLevel Three\n^^^^^^^^^^^\n\nContent.\n";
    let mut state = ParserState::new_for_test(input);
    Rc::make_mut(&mut state.options).setext = true;
    let result = document_parser::document(input, &mut state)??;
    let section = result
        .blocks
        .iter()
        .find_map(|b| {
            if let Block::Section(s) = b {
                Some(s)
            } else {
                None
            }
        })
        .expect("level 3 section");
    assert_eq!(section.level, 3);

    // Test level 4 with +
    let input = "= Doc\n\nLevel Four\n++++++++++\n\nContent.\n";
    let mut state = ParserState::new_for_test(input);
    Rc::make_mut(&mut state.options).setext = true;
    let result = document_parser::document(input, &mut state)??;
    let section = result
        .blocks
        .iter()
        .find_map(|b| {
            if let Block::Section(s) = b {
                Some(s)
            } else {
                None
            }
        })
        .expect("level 4 section");
    assert_eq!(section.level, 4);

    Ok(())
}

#[cfg(feature = "setext")]
#[test]
#[tracing_test::traced_test]
fn test_setext_manpage_style_document() -> Result<(), Error> {
    let input = "gitdatamodel(7)\n===============\n\nNAME\n----\ngitdatamodel - Git's core data model\n\nSYNOPSIS\n--------\ngitdatamodel\n";
    let mut state = ParserState::new_for_test(input);
    Rc::make_mut(&mut state.options).setext = true;
    let result = document_parser::document(input, &mut state)??;

    let header = result.header.expect("document has a header");
    assert!(
        matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if content.contains("gitdatamodel"))
    );

    // Verify NAME and SYNOPSIS are level-1 sections
    let sections: Vec<&Section> = result
        .blocks
        .iter()
        .filter_map(|b| {
            if let Block::Section(s) = b {
                Some(s)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(
        sections.len(),
        2,
        "should have 2 top-level sections (NAME and SYNOPSIS)"
    );
    assert_eq!(sections[0].level, 1);
    assert_eq!(sections[1].level, 1);
    assert!(
        matches!(&sections[0].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "NAME")
    );
    assert!(
        matches!(&sections[1].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "SYNOPSIS")
    );

    Ok(())
}

#[cfg(feature = "setext")]
#[test]
#[tracing_test::traced_test]
fn test_setext_with_description_lists() -> Result<(), Error> {
    // Regression: description list markers (::) anywhere in the document
    // used to cause setext sections to fail because the lookahead
    // `check_start_of_description_list` scanned the entire remaining input
    let input = "\
gitdatamodel(7)
===============

NAME
----
gitdatamodel - description

SYNOPSIS
--------
gitdatamodel

OBJECTS
-------

commit::
    A commit.

REFERENCES
----------

References.
";
    let options = Options::builder().with_setext().build()?;
    let parsed = parse(input, &options)?;
    let result = parsed.document();

    let header = result.header.as_ref().expect("document has a header");
    assert!(
        matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if content.contains("gitdatamodel"))
    );

    let sections: Vec<&Section> = result
        .blocks
        .iter()
        .filter_map(|b| {
            if let Block::Section(s) = b {
                Some(s)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(
        sections.len(),
        4,
        "should have 4 sections (NAME, SYNOPSIS, OBJECTS, REFERENCES)"
    );
    for section in &sections {
        assert_eq!(section.level, 1);
    }

    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_index_term_flow() -> Result<(), Error> {
    use crate::InlineMacro;

    let input = "= Test\n\nThis is about ((Arthur)) the king.\n";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;

    let paragraph = result
        .blocks
        .iter()
        .find_map(|b| {
            if let Block::Paragraph(p) = b {
                Some(p)
            } else {
                None
            }
        })
        .expect("paragraph exists");

    let has_index_term = paragraph.content.iter().any(|inline| {
        matches!(
            inline,
            InlineNode::Macro(InlineMacro::IndexTerm(it))
                if it.is_visible()
                    && matches!(it.term(), [InlineNode::PlainText(text)] if text.content == "Arthur")
        )
    });

    assert!(
        has_index_term,
        "Expected to find visible index term 'Arthur', but found: {:?}",
        paragraph.content
    );
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_index_term_concealed() -> Result<(), Error> {
    use crate::InlineMacro;

    let input = "= Test\n\n(((Sword, Broadsword)))This is a concealed index term.\n";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;

    let paragraph = result
        .blocks
        .iter()
        .find_map(|b| {
            if let Block::Paragraph(p) = b {
                Some(p)
            } else {
                None
            }
        })
        .expect("paragraph exists");

    let has_concealed_term = paragraph.content.iter().any(|inline| {
        matches!(
            inline,
            InlineNode::Macro(InlineMacro::IndexTerm(it))
                if !it.is_visible()
                    && matches!(it.term(), [InlineNode::PlainText(text)] if text.content == "Sword")
        )
    });

    assert!(
        has_concealed_term,
        "Expected to find concealed index term 'Sword', but found: {:?}",
        paragraph.content
    );
    Ok(())
}

/// Test that macro attributes (like `image::`) correctly allow . # % as literal characters.
///
/// This verifies the fix for the issue where `image::photo.jpg[Diablo 4 picture of Lilith.]`
/// would fail because the trailing `.` was interpreted as a role shorthand prefix.
///
/// In asciidoctor, shorthand syntax (.role, #id, %option) is only valid in block-level
/// attributes, NOT inside macro brackets. Macro brackets should treat these characters
/// as literal content.
#[test]
#[tracing_test::traced_test]
fn test_macro_attributes_allow_literal_special_chars() -> Result<(), Error> {
    fn get_image<'a>(doc: &'a Document<'a>) -> &'a Image<'a> {
        doc.blocks
            .iter()
            .find_map(|b| {
                if let Block::Image(img) = b {
                    Some(img)
                } else {
                    None
                }
            })
            .expect("document should have an image block")
    }

    let input = "image::photo.jpg[Diablo 4 picture of Lilith.]";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;
    let img = get_image(&result);
    assert_eq!(
        img.metadata.attributes.get("alt"),
        Some(&AttributeValue::String(
            "Diablo 4 picture of Lilith.".into()
        )),
        "Trailing period should be preserved in alt text"
    );

    // Test .role as literal text (not a shorthand)
    let input = "image::photo.jpg[.role]";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;
    let img = get_image(&result);
    assert_eq!(
        img.metadata.attributes.get("alt"),
        Some(&AttributeValue::String(".role".into())),
        ".role should be literal alt text, not a CSS class"
    );
    assert!(
        img.metadata.roles.is_empty(),
        "roles should be empty - .role is literal text"
    );

    // Test #id as literal text (not a shorthand)
    let input = "image::photo.jpg[Issue #42]";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;
    let img = get_image(&result);
    assert_eq!(
        img.metadata.attributes.get("alt"),
        Some(&AttributeValue::String("Issue #42".into())),
        "#42 should be preserved as literal text"
    );
    assert!(
        img.metadata.id.is_none(),
        "id should be empty - #42 is literal text"
    );

    let input = "image::photo.jpg[role=thumbnail]";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;
    let img = get_image(&result);
    assert_eq!(
        img.metadata.roles,
        vec![std::borrow::Cow::Borrowed("thumbnail")],
        "Named role= attribute should work"
    );

    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn test_block_macro_uses_last_closing_bracket() -> Result<(), Error> {
    let input = "image::foo.svg[role=inline][100,100]\n\n[.lead]\nHello\n";
    let mut state = ParserState::new_for_test(input);
    let result = document_parser::document(input, &mut state)??;

    let image = result.blocks.iter().find_map(|block| {
        if let Block::Image(image) = block {
            Some(image)
        } else {
            None
        }
    });
    assert!(image.is_some(), "document should contain an image block");
    if let Some(image) = image {
        assert_eq!(image.metadata.roles, ["inline][100"]);
        assert_eq!(
            image.metadata.attributes.get("width"),
            Some(&AttributeValue::String("100".into()))
        );
    }
    assert!(
        result
            .blocks
            .iter()
            .any(|b| matches!(b, Block::Paragraph(_))),
        "document should contain a paragraph block"
    );
    assert!(state.warnings.borrow().is_empty());
    Ok(())
}

/// When `source_ranges` are set, `warn_trailing_macro_content` should resolve
/// the correct file name and line number from the included file.
#[test]
fn test_trailing_content_warning_resolves_source_range() {
    use crate::model::SourceRange;
    use std::path::PathBuf;

    // Bytes 28..60 come from sponsor.adoc; byte 40 is on its fourth line.
    let input = "a]b\n".repeat(20); // 80 bytes total (4 bytes per line)
    let mut state = ParserState::new_for_test(&input);
    state.current_file = Some(PathBuf::from("/docs/main.adoc").into());
    state.source_ranges = vec![SourceRange {
        start_offset: 28, // byte 28 starts the included region
        end_offset: 60,
        file: Some(PathBuf::from("/docs/sponsor.adoc")),
        file_chain: vec!["sponsor.adoc".to_string()],
        start_line: 1,
        source_start_offset: 0,
        column_shift: 0,
    }];

    // Trigger warning at byte offset 40 (inside the included range)
    // 40 - 28 = 12 bytes into the included content = 3 newlines = line 4
    state.warn_trailing_macro_content("image", "[100,100]", 40, 0);

    let warnings = state.warnings.borrow();
    assert_eq!(warnings.len(), 1);
    let loc = warnings[0]
        .source_location()
        .expect("warning should have a location");
    assert_eq!(
        loc.file.as_deref(),
        Some(std::path::Path::new("/docs/sponsor.adoc")),
        "should reference the included file, got: {:?}",
        loc.file,
    );
    let position_line = loc.location.start.line;
    assert_eq!(
        position_line, 4,
        "should reference line 4 in included file, got line {position_line}",
    );
}

/// When offset is outside any `source_range`, `warn_trailing_macro_content`
/// should fall back to the entry-point file.
#[test]
fn test_trailing_content_warning_falls_back_to_entry_file() {
    use crate::model::SourceRange;
    use std::path::PathBuf;

    let input = "image::x.png[alt]extra\nsecond line\n";
    let mut state = ParserState::new_for_test(input);
    state.current_file = Some(PathBuf::from("/docs/main.adoc").into());
    state.source_ranges = vec![SourceRange {
        start_offset: 100, // well beyond input - shouldn't match
        end_offset: 200,
        file: Some(PathBuf::from("/docs/other.adoc")),
        file_chain: vec!["other.adoc".to_string()],
        start_line: 1,
        source_start_offset: 0,
        column_shift: 0,
    }];

    state.warn_trailing_macro_content("image", "extra", 17, 0);

    let warnings = state.warnings.borrow();
    assert_eq!(warnings.len(), 1);
    let loc = warnings[0]
        .source_location()
        .expect("warning should have a location");
    assert_eq!(
        loc.file.as_deref(),
        Some(std::path::Path::new("/docs/main.adoc")),
        "should reference the entry-point file, got: {:?}",
        loc.file,
    );
}

/// When the document has a title and the first section skips level 1,
/// the parser should warn (asciidoctor's "section title out of sequence").
#[test]
fn test_first_section_not_level_1_emits_warning() -> Result<(), Error> {
    let input = "= Doc Title\n\n=== Starts at level 2\n\nContent\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    let warning = warnings
        .iter()
        .find(|w| {
            matches!(
                &w.kind,
                WarningKind::SectionLevelOutOfSequence { got: 2, .. },
            )
        })
        .expect("expected out-of-sequence warning");
    // The warning should carry the location of the offending section
    // (byte 13 = line 3 in the test input).
    let loc = warning
        .source_location()
        .expect("warning should carry a location");
    assert_eq!(loc.location.start.line, 3);
    Ok(())
}

/// A level-0 `[appendix]` is rendered at level 1, so its first subsection
/// must be a level-2 (`===`) section — that is in sequence and must NOT warn,
/// matching asciidoctor.
#[test]
fn test_level0_appendix_level2_subsection_no_warning() -> Result<(), Error> {
    let input = "= Book\n:doctype: book\n\n= Part One\n\n== Chapter\n\nbody\n\n[appendix]\n= App Part\n\nintro\n\n=== First Subsection\n\nbody\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(&w.kind, WarningKind::SectionLevelOutOfSequence { .. },)),
        "level-2 subsection of a level-0 appendix is in sequence, got: {warnings:?}"
    );
    Ok(())
}

#[test]
fn test_level0_preface_level2_subsection_no_warning() -> Result<(), Error> {
    let input =
        "= Book\n:doctype: book\n\n[preface]\n= Preface\n\nintro\n\n=== Background\n\nbody\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        !warnings
            .iter()
            .any(|warning| matches!(&warning.kind, WarningKind::SectionLevelOutOfSequence { .. },)),
        "level-2 subsection of a level-0 preface is in sequence, got: {warnings:?}"
    );
    Ok(())
}

/// A level-0 `[appendix]`'s children are expected at level 2, so a level-3
/// (`====`) child that skips level 2 still warns — with `expected: 2`,
/// matching asciidoctor.
#[test]
fn test_level0_appendix_level3_child_still_warns() -> Result<(), Error> {
    let input = "= Book\n:doctype: book\n\n= Part One\n\n== Chapter\n\nbody\n\n[appendix]\n= App Part\n\nintro\n\n==== Too Deep\n\nbody\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        warnings.iter().any(|w| matches!(
            &w.kind,
            WarningKind::SectionLevelOutOfSequence {
                expected: 2,
                got: 3
            },
        )),
        "level-3 child skipping level 2 should warn, got: {warnings:?}"
    );
    Ok(())
}

/// A titleless document whose first section skips level 1 still warns when
/// preamble body content (here a description list) precedes it — the
/// preamble anchors the document at level 0. Matches asciidoctor.
#[test]
fn test_titleless_preamble_then_deep_section_emits_warning() -> Result<(), Error> {
    let input = "term:: desc\n\n===== Deep\n\ntext\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        warnings.iter().any(|w| matches!(
            &w.kind,
            WarningKind::SectionLevelOutOfSequence {
                expected: 1,
                got: 4
            },
        )),
        "expected out-of-sequence warning, got: {warnings:?}"
    );
    Ok(())
}

/// A titleless document whose very first block is a deeper-than-1 section
/// (no doctitle, no preamble) does not warn — matches asciidoctor.
#[test]
fn test_titleless_bare_deep_section_no_warning() -> Result<(), Error> {
    let input = "===== Deep\n\ntext\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(&w.kind, WarningKind::SectionLevelOutOfSequence { .. },)),
        "expected no out-of-sequence warning, got: {warnings:?}"
    );
    Ok(())
}

/// Once anchored (here by a doctitle), asciidoctor flags *every* top-level
/// section that skips level 1, not just the first. Two sibling `=====`
/// sections must each produce a warning.
#[test]
fn test_multiple_top_level_sections_each_warn() -> Result<(), Error> {
    let input = "= Doc Title\n\n===== One\n\ntext\n\n===== Two\n\ntext\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    let count = warnings
        .iter()
        .filter(|w| {
            matches!(
                &w.kind,
                WarningKind::SectionLevelOutOfSequence {
                    expected: 1,
                    got: 4
                },
            )
        })
        .count();
    assert_eq!(
        count, 2,
        "expected one warning per sibling, got: {warnings:?}"
    );
    Ok(())
}

/// An un-anchored document that opens with a deep section establishes that
/// section's level as the base, so same-level siblings are not flagged.
#[test]
fn test_bare_deep_section_siblings_no_warning() -> Result<(), Error> {
    let input = "===== One\n\ntext\n\n===== Two\n\ntext\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(&w.kind, WarningKind::SectionLevelOutOfSequence { .. },)),
        "expected no out-of-sequence warning, got: {warnings:?}"
    );
    Ok(())
}

/// A `[comment]`-styled block produces no output. The `--` open block
/// becomes a `DelimitedComment` (kept distinct from a `////` block by its
/// `--` delimiter); the paragraph becomes a `Comment` of kind `Paragraph`.
/// The following blank-separated paragraph is kept.
#[test]
fn test_comment_style_block_dropped() -> Result<(), Error> {
    let input =
        "[comment]\n--\nhidden\n\n== Hidden heading\n--\n\n[comment]\nhidden para.\n\nVisible.\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    assert_eq!(doc.blocks.len(), 3);

    // The open block: a `--`-delimited DelimitedComment (no leftover
    // `comment` style) retaining its raw inner text.
    assert!(
        matches!(
            &doc.blocks[0],
            Block::DelimitedBlock(delimited)
            if matches!(&delimited.inner, DelimitedBlockType::DelimitedComment(nodes)
                if matches!(&nodes[0], InlineNode::PlainText(text)
                    if text.content.contains("Hidden heading")))
                    && delimited.delimiter == "--"
                    && delimited.metadata.style.is_none()
        ),
        "the [comment] open block should be a `--` DelimitedComment"
    );

    // The paragraph: a `Comment` of kind `Paragraph`.
    assert!(
        matches!(&doc.blocks[1], Block::Comment(comment) if comment.kind == CommentKind::Paragraph),
        "the [comment] paragraph should be a Comment of kind Paragraph"
    );

    // The trailing blank-separated paragraph is normal content.
    assert!(
        matches!(&doc.blocks[2], Block::Paragraph(para)
            if matches!(&para.content[..], [InlineNode::PlainText(text)]
                if text.content == "Visible.")),
        "the trailing paragraph should survive"
    );
    Ok(())
}

/// `[comment]` only suppresses open blocks and paragraphs. On any other
/// block (e.g. a listing) `asciidoctor` ignores the style and renders the
/// block, so it must stay a normal `DelimitedListing`, not become a comment.
#[test]
fn test_comment_style_on_listing_renders() -> Result<(), Error> {
    let input = "[comment]\n----\nvisible\n----\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    assert_eq!(doc.blocks.len(), 1);
    assert!(
        matches!(
            &doc.blocks[0],
            Block::DelimitedBlock(delimited)
                if matches!(delimited.inner, DelimitedBlockType::DelimitedListing(_))
        ),
        "a [comment]-styled listing must still render as a listing"
    );
    Ok(())
}

/// An `<<id>>` whose target is defined nowhere is an unresolved reference
/// and warns, pointing at the cross-reference.
#[test]
fn test_unresolved_reference_warns() -> Result<(), Error> {
    let input = "A paragraph.\n\nSee <<missing>>.\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        warnings.iter().any(|w| matches!(
            &w.kind,
            WarningKind::UnresolvedReference { target } if target == "missing"
        )),
        "expected an unresolved-reference warning for `missing`"
    );
    Ok(())
}

/// An `<<id>>` pointing at an inline `[[id]]` anchor resolves (the catalog
/// includes inline anchors), so it does not warn.
#[test]
fn test_inline_anchor_reference_resolves() -> Result<(), Error> {
    let input = "Some text [[here]] in a paragraph.\n\nSee <<here>>.\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    assert!(doc.references.contains_key("here"));
    let warnings = state.warnings.borrow();
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(&w.kind, WarningKind::UnresolvedReference { .. })),
        "a reference to an existing inline anchor must not warn"
    );
    Ok(())
}

/// IDs attached to formatted spans are reference targets in the same way
/// as explicit inline anchors.
#[test]
fn test_formatted_inline_ids_resolve_cross_references() -> Result<(), Error> {
    let input = r#"A [#bold-id]*bold*, [#italic-id]_italic_, [#mono-id]`mono`, [#mark-id]#mark#, [#sub-id]~sub~, [#super-id]^super^, [#double-id]"`double`", and [#single-id]'`single`'.

See <<bold-id>>, <<italic-id>>, <<mono-id>>, <<mark-id>>, <<sub-id>>, <<super-id>>, <<double-id>>, and <<single-id>>.
"#;
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;

    for id in [
        "bold-id",
        "italic-id",
        "mono-id",
        "mark-id",
        "sub-id",
        "super-id",
        "double-id",
        "single-id",
    ] {
        assert!(
            doc.references.contains_key(id),
            "formatted inline ID `{id}` must be a reference target"
        );
    }
    let warnings = state.warnings.borrow();
    assert!(
        !warnings
            .iter()
            .any(|warning| matches!(warning.kind, WarningKind::UnresolvedReference { .. })),
        "formatted inline references should resolve: {warnings:?}"
    );
    Ok(())
}

#[test]
fn test_link_ids_resolve_cross_references_without_using_link_text() -> Result<(), Error> {
    let input = r"Before: <<link-id>>, <<url-id>>, <<mailto-id>>, and <<bare-id>>.

link:https://example.com[Link text,id=link-id,role=hot]

https://example.org[URL text,id=url-id]

mailto:person@example.com[Mail text,id=mailto-id]

link:https://example.net[,id=bare-id]

After: <<link-id>>, <<url-id>>, <<mailto-id>>, and <<bare-id>>.
";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;

    for id in ["link-id", "url-id", "mailto-id", "bare-id"] {
        let Some(reference) = doc.references.get(id) else {
            unreachable!("link ID `{id}` must be a reference target");
        };
        assert!(reference.xreflabel.is_none());
        assert!(reference.title.is_none());
    }
    let warnings = state.warnings.borrow();
    assert!(
        !warnings
            .iter()
            .any(|warning| matches!(warning.kind, WarningKind::UnresolvedReference { .. })),
        "link references should resolve: {warnings:?}"
    );
    Ok(())
}

#[test]
fn test_link_id_catalog_keeps_first_definition_and_ignores_positional_text() -> Result<(), Error> {
    let input = r"link:https://example.com[First,id=duplicate]

link:https://example.org[Second,id=duplicate]

link:https://example.net[Text,positional-id]
";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;

    assert_eq!(
        doc.references
            .get("duplicate")
            .expect("the first link ID must be catalogued")
            .location
            .start
            .line,
        1
    );
    assert!(!doc.references.contains_key("positional-id"));
    Ok(())
}

/// An inline `[[id]]` anchor inside a callout-list item's text is catalogued
/// (callout lists are walked like other list containers), so a reference to
/// it resolves.
#[test]
fn test_callout_item_inline_anchor_resolves() -> Result<(), Error> {
    let input = "----\ncode <1>\n----\n<1> Note with an [[cnote]] anchor.\n\nSee <<cnote>>.\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    assert!(doc.references.contains_key("cnote"));
    let warnings = state.warnings.borrow();
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(&w.kind, WarningKind::UnresolvedReference { .. })),
        "a reference to an anchor inside a callout item must not warn"
    );
    Ok(())
}

#[test]
fn test_callout_item_explicit_continuation_attaches_block() -> Result<(), Error> {
    let input = "----\nfirst <1>\nsecond <2>\n----\n<1> First explanation.\n+\nAttached paragraph.\n<2> Second explanation.\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let list = doc
        .blocks
        .iter()
        .find_map(|block| {
            if let Block::CalloutList(list) = block {
                Some(list)
            } else {
                None
            }
        })
        .expect("callout list must be parsed");

    assert_eq!(list.items.len(), 2);
    assert_eq!(list.items[0].blocks.len(), 1);
    assert!(matches!(list.items[0].blocks[0], Block::Paragraph(_)));
    assert_eq!(list.items[1].blocks, []);
    assert!(state.warnings.borrow().is_empty());
    Ok(())
}

/// A titled block with an id is collected into `references` so a `<<id>>`
/// reference can resolve to its title.
#[test]
fn test_titled_block_collected_in_references() -> Result<(), Error> {
    let input = "[[data-table]]\n.Important Data\n[cols=\"1,1\"]\n|===\n| a | b\n|===\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let entry = doc
        .references
        .get("data-table")
        .expect("titled block should be a reference target");
    let title = entry
        .title
        .as_ref()
        .expect("titled block has reference text");
    assert!(
        matches!(&title[..], [InlineNode::PlainText(text)] if text.content == "Important Data")
    );
    // The location points at the anchor on line 1 (for LSP navigation).
    assert_eq!(entry.location.start.line, 1);
    Ok(())
}

#[test]
fn natural_title_cross_references_resolve_to_section_ids() -> Result<(), Error> {
    let input = "Generated: <<Syntax Highlighting>>.\n\nCustom: <<Syntax Highlighting,section>>.\n\nExplicit: <<explicit-id>>.\n\nExplicit title: <<Explicit Title>>.\n\nMissing: <<Missing Title>>.\n\n== Syntax Highlighting\n\n[#explicit-id]\n== Explicit Title\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let xrefs = doc
        .blocks
        .iter()
        .filter_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            paragraph.content.iter().find_map(|inline| {
                let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                    return None;
                };
                Some(xref.target)
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(
        xrefs,
        [
            "_syntax_highlighting",
            "_syntax_highlighting",
            "explicit-id",
            "explicit-id",
            "Missing Title",
        ]
    );
    let warnings = state.warnings.borrow();
    assert_eq!(
        warnings
            .iter()
            .filter_map(|warning| {
                let WarningKind::UnresolvedReference { target } = &warning.kind else {
                    return None;
                };
                Some(target.as_str())
            })
            .collect::<Vec<_>>(),
        ["Missing Title"]
    );
    Ok(())
}

#[test]
fn named_section_reftext_populates_catalog_toc_and_warnings()
-> Result<(), Box<dyn std::error::Error>> {
    let input = include_str!("../../../fixtures/tests/named_section_reftext_cross_references.adoc");
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let reference = doc.references.get("id").ok_or("missing id reference")?;
    assert!(
        matches!(
            reference.xreflabel.as_deref(),
            Some([InlineNode::PlainText(text)]) if text.content == "Custom Label"
        ),
        "{:?}",
        reference.xreflabel
    );
    assert_eq!(
        doc.toc_entries
            .iter()
            .find(|entry| entry.id == "id")
            .and_then(|entry| entry.xreflabel),
        Some("Custom Label")
    );
    let formatted = doc
        .references
        .get("formatted")
        .ok_or("missing formatted reference")?;
    assert!(
        matches!(
            formatted.xreflabel.as_deref(),
            Some([
                InlineNode::PlainText(prefix),
                InlineNode::BoldText(bold),
                InlineNode::PlainText(suffix),
            ]) if prefix.content == "Custom "
                && suffix.content == " Label"
                && matches!(
                    &bold.content[..],
                    [InlineNode::PlainText(text)] if text.content == "Formatted"
                )
        ),
        "{:?}",
        formatted.xreflabel
    );
    assert_eq!(
        state
            .warnings
            .borrow()
            .iter()
            .filter_map(|warning| {
                let WarningKind::UnresolvedReference { target } = &warning.kind else {
                    return None;
                };
                Some(target.as_str())
            })
            .collect::<Vec<_>>(),
        ["Actual Title", "Generated Title", "Custom Formatted Label"]
    );
    Ok(())
}

#[test]
fn passthrough_xref_warnings_use_restored_targets_and_source_locations() -> Result<(), Error> {
    let input = include_str!(
        "../../../fixtures/tests/natural_title_cross_references_with_passthrough.adoc"
    );
    let result = parse(input, &Options::default())?;

    let warnings = result
        .warnings()
        .iter()
        .filter_map(|warning| {
            let WarningKind::UnresolvedReference { target } = &warning.kind else {
                return None;
            };
            let location = warning.source_location()?;
            Some((
                target.as_str(),
                location.location.start.line,
                location.location.start.column,
            ))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        warnings,
        [
            ("Target raw Title", 3, 15),
            ("Target raw Title", 4, 14),
            ("Missing raw Title", 5, 16),
            ("Missing raw Title", 6, 15),
        ]
    );
    Ok(())
}

#[test]
fn compat_mode_skips_natural_title_cross_reference_resolution() -> Result<(), Error> {
    let input = "= Document\n:compat-mode:\n\nNatural: <<Syntax Highlighting>>.\n\nExplicit: <<_syntax_highlighting>>.\n\n== Syntax Highlighting\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let targets = doc
        .blocks
        .iter()
        .filter_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            paragraph.content.iter().find_map(|inline| {
                let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                    return None;
                };
                Some(xref.target)
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(targets, ["Syntax Highlighting", "_syntax_highlighting"]);
    assert!(doc.references.contains_key("_syntax_highlighting"));
    assert_eq!(
        state
            .warnings
            .borrow()
            .iter()
            .filter_map(|warning| {
                let WarningKind::UnresolvedReference { target } = &warning.kind else {
                    return None;
                };
                Some(target.as_str())
            })
            .collect::<Vec<_>>(),
        ["Syntax Highlighting"]
    );
    Ok(())
}

#[test]
fn compat_mode_natural_reference_resolution_follows_source_position() -> Result<(), Error> {
    let input = "Before set: <<First Natural>>.\n\n:compat-mode:\n\nAfter set: <<Second Natural>>.\n\n:compat-mode!:\n\nAfter unset: <<Third Natural>>.\n\n== First Natural\n\n== Second Natural\n\n== Third Natural\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let targets = doc
        .blocks
        .iter()
        .filter_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            paragraph.content.iter().find_map(|inline| {
                let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                    return None;
                };
                Some(xref.target)
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(
        targets,
        ["_first_natural", "Second Natural", "_third_natural"]
    );
    assert_eq!(
        state
            .warnings
            .borrow()
            .iter()
            .filter_map(|warning| {
                let WarningKind::UnresolvedReference { target } = &warning.kind else {
                    return None;
                };
                Some(target.as_str())
            })
            .collect::<Vec<_>>(),
        ["Second Natural"]
    );
    Ok(())
}

#[test]
fn interdocument_xref_macro_targets_are_not_naturally_resolved() -> Result<(), Error> {
    let input = "Empty: xref:Other.adoc[].\n\nExplicit: xref:Other.adoc[Other].\n\nShorthand: <<Other.adoc>>.\n\nFragment: xref:Foo#Bar[].\n\n== Other.adoc\n\n== Foo#Bar\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let targets = doc
        .blocks
        .iter()
        .filter_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            paragraph.content.iter().find_map(|inline| {
                let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                    return None;
                };
                Some(xref.target)
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(
        targets,
        ["Other.adoc", "Other.adoc", "_other_adoc", "Foo#Bar"]
    );
    assert!(
        state.warnings.borrow().is_empty(),
        "interdocument targets and the resolved shorthand must not warn"
    );
    Ok(())
}

#[cfg(feature = "setext")]
#[test]
fn natural_title_cross_references_resolve_setext_section_ids() -> Result<(), Error> {
    let input = "See <<Setext Title>>.\n\nSetext Title\n------------\n";
    let mut state = ParserState::new_for_test(input);
    Rc::make_mut(&mut state.options).setext = true;
    let doc = document_parser::document(input, &mut state)??;
    let xrefs = doc
        .blocks
        .iter()
        .filter_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            paragraph.content.iter().find_map(|inline| {
                let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                    return None;
                };
                Some(xref.target)
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(xrefs, ["_setext_title"]);
    Ok(())
}

#[test]
fn captioned_references_keep_source_order_xrefstyle_and_target_caption() -> Result<(), Error> {
    let input = "= Caption references\n:xrefstyle: short\n\nShort: <<figure-target>>.\n\n:table-caption: ReferenceTable\n:xrefstyle: full\n\nFull: <<table-target>>.\n\n:xrefstyle: basic\n\nBasic: <<figure-target>>.\n\n:table-caption:\n:xrefstyle: short\n\nNumber only: <<table-target>>.\n\n:table-caption!:\n\nTarget label: <<table-target>>.\n\n:table-caption: TargetTable\n\n[[figure-target]]\n.A figure\nimage::figure.svg[]\n\n[[table-target]]\n.A table\n|===\n|Cell\n|===\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let xrefs = doc
        .blocks
        .iter()
        .filter_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            paragraph.content.iter().find_map(|inline| {
                let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                    return None;
                };
                Some((xref.target, xref.xrefstyle, xref.caption_label))
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(
        xrefs,
        [
            (
                "figure-target",
                XrefStyle::Short,
                XrefCaptionLabel::AtTarget,
            ),
            (
                "table-target",
                XrefStyle::Full,
                XrefCaptionLabel::AtReference("ReferenceTable"),
            ),
            (
                "figure-target",
                XrefStyle::Basic,
                XrefCaptionLabel::AtTarget,
            ),
            (
                "table-target",
                XrefStyle::Short,
                XrefCaptionLabel::NumberOnly,
            ),
            ("table-target", XrefStyle::Short, XrefCaptionLabel::AtTarget,),
        ]
    );
    assert!(matches!(
        doc.references
            .get("figure-target")
            .and_then(|reference| reference.caption.as_ref()),
        Some(Caption::Numbered {
            kind: CaptionKind::Figure,
            label,
            number: Some(number),
        }) if label == "Figure" && number.get() == 1
    ));
    assert!(matches!(
        doc.references
            .get("table-target")
            .and_then(|reference| reference.caption.as_ref()),
        Some(Caption::Numbered {
            kind: CaptionKind::Table,
            label,
            number: Some(number),
        }) if label == "TargetTable" && number.get() == 1
    ));
    Ok(())
}

#[test]
fn test_titled_single_line_admonition_keeps_reference_title() -> Result<(), Error> {
    let input = "[[notice]]\n.Admonition *Title*\nNOTE: note\n\nSee <<notice>>.\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let entry = doc
        .references
        .get("notice")
        .expect("titled admonition should be a reference target");
    let title = entry
        .title
        .as_ref()
        .expect("titled admonition should keep its reference text");

    assert!(
        matches!(
            &title[..],
            [InlineNode::PlainText(prefix), InlineNode::BoldText(bold)]
                if prefix.content == "Admonition "
                    && matches!(
                        &bold.content[..],
                        [InlineNode::PlainText(text)] if text.content == "Title"
                    )
        ),
        "{title:?}"
    );
    assert!(
        !state
            .warnings
            .borrow()
            .iter()
            .any(|warning| matches!(warning.kind, WarningKind::UnresolvedReference { .. }))
    );
    Ok(())
}

/// A block with an id but no title is still a reference target — present in
/// the catalog with no reference text (`title: None`). This distinguishes a
/// resolvable-but-untitled id (renders `[id]`) from an absent/unresolved id.
#[test]
fn test_untitled_block_in_references_without_reftext() -> Result<(), Error> {
    let input = "[[untitled]]\n[cols=\"1,1\"]\n|===\n| a | b\n|===\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let entry = doc
        .references
        .get("untitled")
        .expect("untitled block with an id is still a reference target");
    assert!(
        entry.title.is_none(),
        "untitled block has no reference text"
    );
    Ok(())
}

/// The first element to claim an id owns its reference text, matching
/// asciidoctor: a later element with the same id (here a formatted span,
/// which carries no title) does not take the text away from the titled
/// block that registered first.
#[test]
fn test_duplicate_id_keeps_first_reference_text() -> Result<(), Error> {
    let input = "[[dup]]\n.Titled Block\n====\nbody\n====\n\nA [#dup]*bold* span.\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let entry = doc
        .references
        .get("dup")
        .expect("the id is a reference target");
    let title = entry
        .title
        .as_ref()
        .expect("the first registration keeps its reference text");
    assert!(
        matches!(
            &title[..],
            [InlineNode::PlainText(text)] if text.content == "Titled Block"
        ),
        "{title:?}"
    );
    Ok(())
}

/// A reference label is inline content, so `[[id,*Bold* label]]` reaches
/// converters as parsed inline nodes rather than literal asterisks.
#[test]
fn test_reference_label_is_parsed_as_inlines() -> Result<(), Error> {
    let input = "Some [[labelled,*Bold* label]]text.\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let label = doc
        .references
        .get("labelled")
        .expect("the id is a reference target")
        .xreflabel
        .as_ref()
        .expect("the anchor has a label");
    assert!(
        matches!(
            &label[..],
            [InlineNode::BoldText(bold), InlineNode::PlainText(rest)]
                if rest.content == " label"
                    && matches!(
                        &bold.content[..],
                        [InlineNode::PlainText(text)] if text.content == "Bold"
                    )
        ),
        "{label:?}"
    );
    Ok(())
}

#[test]
fn test_reference_label_restores_passthrough_syntax() -> Result<(), Error> {
    let input = "Some [[labelled,+++<mark>Label</mark>+++]]text.\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    let label = doc
        .references
        .get("labelled")
        .expect("the id is a reference target")
        .xreflabel
        .as_ref()
        .expect("the anchor has a label");
    assert!(
        matches!(
            &label[..],
            [InlineNode::PlainText(text)]
                if text.content == "+++<mark>Label</mark>+++"
        ),
        "{label:?}"
    );
    Ok(())
}

#[test]
fn test_nested_passthrough_retains_substitution_policy() -> Result<(), Error> {
    let input = "*before +++<mark>nested</mark>+++ after*\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;
    assert!(
        matches!(
            &doc.blocks[..],
            [Block::Paragraph(paragraph)]
                if matches!(
                    &paragraph.content[..],
                    [InlineNode::BoldText(bold)]
                        if matches!(
                            &bold.content[..],
                            [InlineNode::PlainText(before), InlineNode::RawText(raw), InlineNode::PlainText(after)]
                                if before.content == "before "
                                    && raw.content == "<mark>nested</mark>"
                                    && raw.subs.is_empty()
                                    && after.content == " after"
                        )
                )
        ),
        "{:?}",
        doc.blocks
    );
    Ok(())
}

/// An author line that doesn't parse as structured authors is kept as a
/// single author, and the parser warns (acdc-only heads-up; asciidoctor is
/// silent). The warning points at the author line.
#[test]
fn test_non_standard_author_line_emits_warning() -> Result<(), Error> {
    let input = "= Doc Title\nAuthor: Roberto Avanzi (Lead), Ruud Derwig\n:foo: bar\n\nBody.\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    let warning = warnings
        .iter()
        .find(|w| {
            matches!(
                &w.kind,
                WarningKind::NonStandardAuthorLine { line }
                    if line == "Author: Roberto Avanzi (Lead), Ruud Derwig"
            )
        })
        .expect("expected non-standard author line warning");
    let loc = warning
        .source_location()
        .expect("warning should carry a location");
    assert_eq!(loc.location.start.line, 2);
    Ok(())
}

/// A discrete heading marked with the legacy `float` block style warns so
/// authors can migrate to `discrete`.
#[test]
fn test_legacy_float_discrete_heading_warns() -> Result<(), Error> {
    let input = "== Parent\n\n[float]\n==== Floating\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        warnings
            .iter()
            .any(|w| matches!(&w.kind, WarningKind::LegacyFloatDiscreteHeading)),
        "`[float]` discrete heading should warn"
    );
    Ok(())
}

/// `float` only marks a discrete heading as a block *style*. The preferred
/// `[discrete]`, a table's `float=` layout attribute, and a bare `float`
/// positional (which leaves the block an ordinary section) must NOT raise the
/// legacy-`float` warning.
#[test]
fn test_no_legacy_float_warning() -> Result<(), Error> {
    for input in [
        "== Parent\n\n[discrete]\n==== Disc\n",
        "[float=\"center\",cols=\"1,1\"]\n|===\n| a | b\n|===\n",
        "= Doc\n\n[#f,float]\n=== Ordinary Section\n",
    ] {
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            !warnings
                .iter()
                .any(|w| matches!(&w.kind, WarningKind::LegacyFloatDiscreteHeading)),
            "input {input:?} should not raise the legacy-float warning"
        );
    }
    Ok(())
}

/// A plain `Firstname Lastname` author line parses structurally and must
/// NOT raise the non-standard-author warning.
#[test]
fn test_standard_author_line_no_warning() -> Result<(), Error> {
    let input = "= Doc Title\nRoberto Avanzi\n:foo: bar\n\nBody.\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(&w.kind, WarningKind::NonStandardAuthorLine { .. })),
        "structured author line should not warn"
    );
    Ok(())
}

/// A trailing partial row that cannot fill a complete row is dropped, and
/// the parser warns at the location of the dropped cell — matching
/// asciidoctor's "dropping cells from incomplete row" message.
#[test]
fn test_incomplete_final_row_emits_dropping_warning() -> Result<(), Error> {
    // The lone `|g` on line 5 cannot complete a 3-column row.
    let input = "[cols=\"3*\"]\n|===\n|a |b |c\n|d |e |f\n|g\n|===\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    let warning = warnings
        .iter()
        .find(|w| matches!(&w.kind, WarningKind::TableIncompleteRow))
        .expect("expected dropping-cells warning");
    let loc = warning
        .source_location()
        .expect("warning should carry a location");
    assert_eq!(loc.location.start.line, 5);
    Ok(())
}

/// Without a document title, the first-section-level check is silent
/// (matches asciidoctor's behavior).
#[test]
fn test_first_section_without_doc_title_does_not_warn() -> Result<(), Error> {
    let input = "=== No title above me\n\nContent\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(&w.kind, WarningKind::SectionLevelOutOfSequence { .. })),
        "should not warn without doc title, got: {warnings:?}",
    );
    Ok(())
}

/// Valid structure (doc title + level 1 first section) must not warn.
#[test]
fn test_first_section_level_1_no_warning() -> Result<(), Error> {
    let input = "= Doc Title\n\n== Good\n\n=== Nested\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(&w.kind, WarningKind::SectionLevelOutOfSequence { .. })),
        "should not warn for valid structure, got: {warnings:?}",
    );
    Ok(())
}

/// An opened table that never closes before EOF emits an
/// `UnterminatedTable { separator, equals }` warning and still
/// produces a table (matching asciidoctor's recovery).
#[test]
fn test_unterminated_pipe_table_emits_warning() -> Result<(), Error> {
    let input = "|===\n| A | B\n| C | D\n";
    let mut state = ParserState::new_for_test(input);
    let doc = document_parser::document(input, &mut state)??;

    let warnings = state.warnings.borrow();
    let warning = warnings
        .iter()
        .find(|w| {
            matches!(
                &w.kind,
                WarningKind::UnterminatedTable { delimiter } if delimiter == "|===",
            )
        })
        .expect("expected unterminated table warning");
    let loc = warning
        .source_location()
        .expect("warning should carry a location");
    assert_eq!(loc.location.start.line, 1);

    let has_table = doc.blocks.iter().any(|b| {
        matches!(
            b,
            Block::DelimitedBlock(DelimitedBlock {
                inner: DelimitedBlockType::DelimitedTable(_),
                ..
            })
        )
    });
    assert!(has_table, "expected a table block in the document");
    Ok(())
}

/// The `!===` (exclamation) table delimiter is also covered by the
/// unterminated fallback, and the warning carries the actual opening
/// delimiter so consumers can distinguish between delimiter variants.
#[test]
fn test_unterminated_excl_table_emits_warning() -> Result<(), Error> {
    let input = "!===\n! A ! B\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        warnings.iter().any(|w| matches!(
            &w.kind,
            WarningKind::UnterminatedTable { delimiter } if delimiter == "!===",
        )),
        "expected unterminated table warning with `!===` delimiter, got: {warnings:?}",
    );
    Ok(())
}

/// Diagnostics emitted from inside an `a`-style cell must point at the
/// offending token within the cell, not at the cell's `a|` style prefix.
/// Repro for the case where a nested `!===` is left unterminated:
/// the warning's reported line should match the line of `!===`, not the
/// line of `a|`.
#[test]
fn test_warning_in_ascii_cell_points_at_inner_token() -> Result<(), Error> {
    // Lines:
    //   1: `[cols="1a"]`
    //   2: `|===`
    //   3: `a|`           <- cell style prefix
    //   4: `!===`         <- offending unterminated inner table
    //   5: `! Inner A ! Inner B`
    //   6: `|===`
    let input = "[cols=\"1a\"]\n|===\na|\n!===\n! Inner A ! Inner B\n|===\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    let warning = warnings
        .iter()
        .find(|w| {
            matches!(
                &w.kind,
                WarningKind::UnterminatedTable { delimiter } if delimiter == "!===",
            )
        })
        .expect("expected unterminated inner-table warning");
    let loc = warning
        .source_location()
        .expect("warning should carry a location");
    let line = loc.location.start.line;
    assert_eq!(
        line, 4,
        "warning should point at line 4 (the `!===`), not the `a|` line; got {line}",
    );
    Ok(())
}

/// A properly closed table must not emit an unterminated warning.
#[test]
fn test_terminated_table_does_not_warn() -> Result<(), Error> {
    let input = "|===\n| A | B\n|===\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(&w.kind, WarningKind::UnterminatedTable { .. })),
        "should not warn for a properly closed table, got: {warnings:?}",
    );
    Ok(())
}

/// Degenerate case: the document is just an opening delimiter with no
/// content and no close. Asciidoctor still warns ("unterminated table
/// block"). The unterminated fallback rule should match and produce an
/// empty table rather than falling through to paragraph parsing.
#[test]
fn test_unterminated_pipe_table_with_no_content_emits_warning() -> Result<(), Error> {
    let input = "|===\n";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        warnings.iter().any(|w| matches!(
            &w.kind,
            WarningKind::UnterminatedTable { delimiter } if delimiter == "|===",
        )),
        "expected unterminated table warning for empty open, got: {warnings:?}",
    );
    Ok(())
}

/// Same as above but exercised through the public `parse` entry point
/// (which runs the preprocessor first). Catches the case where the
/// preprocessor normalises the input in a way that breaks the
/// unterminated fallback.
#[test]
fn test_unterminated_pipe_table_empty_through_parse_entry() {
    let opts = Options::default();
    let res = parse("|===\n", &opts).expect("parse should succeed");
    let has_warning = res.warnings().iter().any(|w| {
        matches!(
            &w.kind,
            WarningKind::UnterminatedTable { delimiter } if delimiter == "|===",
        )
    });
    assert!(
        has_warning,
        "expected unterminated table warning through parse(), got: {:?}",
        res.warnings(),
    );
}

/// Every delimited block whose opening delimiter runs to end of input
/// without a close is still produced (closed at EOF) and emits an
/// `UnterminatedDelimitedBlock` warning carrying the block kind and the
/// literal opening delimiter — matching asciidoctor's recovery.
#[test]
fn test_unterminated_delimited_blocks_emit_warning() -> Result<(), Error> {
    // (delimiter line + content, expected kind, expected opening delimiter).
    // A leading paragraph keeps every case in the body so the common
    // delimited-block parser handles it.
    let cases = [
        ("====\ntext", "example", "===="),
        ("----\ntext", "listing", "----"),
        ("....\ntext", "literal", "...."),
        ("****\ntext", "sidebar", "****"),
        ("____\ntext", "quote", "____"),
        ("--\ntext", "open", "--"),
        ("////\ntext", "comment", "////"),
        ("++++\ntext", "pass", "++++"),
        ("```\ntext", "listing", "```"),
    ];
    for (block, want_kind, want_delim) in cases {
        let input = &format!("para\n\n{block}");
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            warnings.iter().any(|w| matches!(
                &w.kind,
                WarningKind::UnterminatedDelimitedBlock { kind, delimiter }
                    if *kind == want_kind && delimiter == want_delim,
            )),
            "expected unterminated {want_kind} warning for input {input:?}, got: {warnings:?}",
        );
        // The block is still produced and recorded as unterminated (no
        // closing delimiter location).
        assert!(
            doc.blocks.iter().any(|b| matches!(
                b,
                Block::DelimitedBlock(d) if d.close_delimiter_location.is_none(),
            )),
            "expected an unterminated delimited block for input {input:?}, got: {:?}",
            doc.blocks,
        );
    }
    Ok(())
}

/// A properly closed delimited block must not emit the unterminated warning.
#[test]
fn test_terminated_delimited_block_no_warning() -> Result<(), Error> {
    let input = "====\ntext\n====";
    let mut state = ParserState::new_for_test(input);
    let _ = document_parser::document(input, &mut state)??;
    let warnings = state.warnings.borrow();
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(&w.kind, WarningKind::UnterminatedDelimitedBlock { .. },)),
        "a closed example block should not warn, got: {warnings:?}",
    );
    Ok(())
}

/// Exercised through the public `parse` entry point (which runs the
/// preprocessor, stripping the trailing newline) so a lone `====\n`
/// source still reaches the grammar as an unterminated block.
#[test]
fn test_unterminated_example_through_parse_entry() {
    let opts = Options::default();
    let res = parse("====\ntext\n", &opts).expect("parse should succeed");
    let has_warning = res.warnings().iter().any(|w| {
        matches!(
            &w.kind,
            WarningKind::UnterminatedDelimitedBlock { kind, delimiter }
                if *kind == "example" && delimiter == "====",
        )
    });
    assert!(
        has_warning,
        "expected unterminated example warning through parse(), got: {:?}",
        res.warnings(),
    );
}
