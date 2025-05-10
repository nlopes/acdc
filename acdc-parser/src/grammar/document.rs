use crate::{
    AttributeValue, Author, Block, BlockMetadata, Document, DocumentAttribute, DocumentAttributes,
    Error, Header, InlineNode, Location, Plain, Section, TableOfContents, grammar::PositionTracker,
};

#[derive(Debug, Default)]
pub(crate) struct ParserState {
    document_attributes: DocumentAttributes,
    tracker: PositionTracker,
}

#[derive(Debug)]
struct Position {
    offset: usize,
    position: crate::Position,
}

peg::parser! {
    pub(crate) grammar document_parser(state: &mut ParserState) for str {
        pub(crate) rule document() -> Result<Document, Error>
            = start:position() empty_or_comment()* header:header() empty_or_comment()* blocks:block()* end:position() {
                Ok(Document {
                    name: "document".to_string(),
                    r#type: "block".to_string(),
                    header,
                    location: Location {
                        absolute_start: start.offset,
                        absolute_end: end.offset,
                        start: start.position,
                        end: end.position
                    },
                    attributes: state.document_attributes.clone(),
                    ..Document::default()
                })
            }

        pub(crate) rule header() -> Option<Header>
            = start:position()
            comment()*
            (document_attribute() (eol() / ![_] / comment()))*
            comment()*
            title_authors:title_authors()?
            comment()*
            (document_attribute() (eol() / ![_] / comment()))*
            comment()*
            end:position() {
                if let Some((title, authors)) = title_authors {
                    let location = Location {
                        absolute_start: start.offset,
                        absolute_end: end.offset,
                        start: start.position,
                        end: end.position
                    };
                    Some(Header {
                        title,
                        subtitle: None,
                        authors,
                        location
                    })
                } else {
                    None
                }
            }

        pub(crate) rule title_authors() -> (Vec<InlineNode>, Vec<Author>) =
            title:document_title() eol() authors:authors_and_revision() (eol() / ![_]) {
                (title, authors)
            }
            / title:document_title() eol() {
                (title, vec![])
            }

        pub(crate) rule document_title() -> Vec<InlineNode>
            = document_title_token() ws() start:position() title:$([^'\n']*) {
                let location = state.tracker.calculate_location(start.position, title, 0);
                vec![InlineNode::PlainText(Plain {
                    content: title.to_string(),
                    location,
                })]
            }

        rule document_title_token() = ("=" / "#") { state.tracker.advance_by(1); }

        rule authors_and_revision() -> Vec<Author>
            = authors:authors() (eol() revision())? {
                authors
            }

        pub(crate) rule authors() -> Vec<Author>
            = authors:(author() ** (";" ws()*)) {
                authors
            }

        pub(crate) rule author() -> Author
            = author_first_name:name_part() ws()+ author_middle_name:name_part() ws()+ author_last_name:$(name_part() ** ws()) ws()* "<" author_email:$([^'>']*) ">" {
                state.tracker.advance_by(2); // skip the "<" and ">"
                state.tracker.advance(author_email);
                Author {
                    first_name: author_first_name.to_string(),
                    middle_name: Some(author_middle_name.to_string()),
                    last_name: author_last_name.to_string(),
                    initials: author_first_name.chars().next().unwrap_or_default().to_string() + &author_middle_name.chars().next().unwrap_or_default().to_string() + &author_last_name.chars().next().unwrap_or_default().to_string(),
                    email: Some(author_email.to_string()),
                }
            }
        / author_first_name:name_part() ws()+ author_last_name:name_part() ws()* "<" author_email:$([^'>']*) ">" {
            state.tracker.advance_by(2); // skip the "<" and ">"
            state.tracker.advance(author_email);
            Author {
                first_name: author_first_name.to_string(),
                middle_name: None,
                last_name: author_last_name.to_string(),
                initials: author_first_name.chars().next().unwrap_or_default().to_string() + &author_last_name.chars().next().unwrap_or_default().to_string(),
                email: Some(author_email.to_string()),
            }
        }
        / author_first_name:name_part() ws()* "<" author_email:$([^'>']*) ">" {
            state.tracker.advance_by(2); // skip the "<" and ">"
            state.tracker.advance(author_email);
            Author {
                first_name: author_first_name.to_string(),
                middle_name: None,
                last_name: String::new(),
                initials: author_first_name.chars().next().unwrap_or_default().to_string(),
                email: Some(author_email.to_string()),
            }
        }
        / author_first_name:name_part() ws()+ author_last_name:name_part() {
            Author {
                first_name: author_first_name.to_string(),
                last_name: author_last_name.to_string(),
                middle_name: None,
                initials: author_first_name.chars().next().unwrap_or_default().to_string() + &author_last_name.chars().next().unwrap_or_default().to_string(),
                email: None,
            }
        }

        rule name_part() -> &'input str
            = name:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-']+ ("_" ['a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-']+)*) {
                state.tracker.advance(name);
                name
            }

        pub(crate) rule revision() -> ()
            = start:position!() number:$("v"? digits() ** ".") date:revdate()? remark:revremark()? end:position!() {
                state.tracker.advance_by(end - start);
                dbg!(number);
                dbg!(date);
                dbg!(remark);
                state.document_attributes.insert("revnumber".to_string(), AttributeValue::String(number.to_string()));
                if let Some(date) = date {
                    state.document_attributes.insert("revdate".to_string(), AttributeValue::String(date.to_string()));
                }
                if let Some(remark) = remark {
                    state.document_attributes.insert("revremark".to_string(), AttributeValue::String(remark.to_string()));
                }
            }

        rule revdate() -> &'input str
            = ", " date:$([^ (':'|'\n')]+) {
                date
            }

        rule revremark() -> &'input str
            = ": " remark:$([^'\n']+) {
                remark
            }

        rule document_attribute() -> ()
            = start:position!() att:document_attribute_match() end:position!()
        {
            state.tracker.advance_by(end - start);
            let (key, value) = att;
            state.document_attributes.insert(key.to_string(), value);
        }

        pub(crate) rule block() -> Block = block:(document_attribute_block() / section() / block_generic()) {
            block
        }

        pub(crate) rule document_attribute_block() -> Block
            = start:position() att:document_attribute_match() end:position() {
                state.tracker.advance_by(end.offset - start.offset);
                let (key, value) = att;
                Block::DocumentAttribute(DocumentAttribute {
                    name: key.to_string(),
                    value,
                    location: Location {
                        absolute_start: start.offset,
                        absolute_end: end.offset,
                        start: start.position,
                        end: end.position
                    }
                })
            }

        pub(crate) rule section() -> Block
            = "== " title:$([^'\n']+) eol() {
                Block::Section(Section{
                    level: 1,
                    title: vec![],
                    metadata: BlockMetadata::default(),
                    content: vec![],
                    location: Location::default()
                })
            }

        pub(crate) rule block_generic() -> Block
            = "::toc::" eol() {
                Block::TableOfContents(TableOfContents {
                    location: Location::default()
                })
            }

        rule digits() = ['0'..='9']+

        rule eol() = quiet!{ "\n" { state.tracker.advance("\n"); } }

        rule empty_or_comment() = quiet!{ eol() / comment() }

        rule comment() = quiet!{ c:$("//" [^'\n']+ "\n"?) { state.tracker.advance(c); } }

        rule document_attribute_match() -> (&'input str, AttributeValue) = ":"
            key:("!" key:$([^':']+) {
                (true, key)
            } / key:$([^'!']+) "!" {
                (true, key)
            } / key:$([^':']+) {
                (false, key)
            })
            ":"
            maybe_value:(" " value:$([^'\n']*) {
                value
            })? {
                let (unset, key) = key;
                if unset {
                    // e.g: :!background: or :background!:
                    (key, AttributeValue::Bool(false))
                } else if let Some(value) = maybe_value {
                    // if it's not unset, and we have a value, set it to that
                    // e.g: :background-color: #fff

                    // if the value is "true" or "false", set it to a boolean
                    // TODO(nlopes): I don't like this specialization but oh well.
                    if value == "true" {
                        (key, AttributeValue::Bool(true))
                    } else if value == "false" {
                        (key, AttributeValue::Bool(false))
                    } else {
                        (key, AttributeValue::String(value.to_string()))
                    }
                } else {
                    // if it's not unset, and we don't have a value, set it to true
                    // e.g: :toc:
                    (key, AttributeValue::Bool(true))
                }
            }

        rule ws() = quiet!{
            c:$(" " / "\t") { state.tracker.advance(c); }
        }

        rule position() -> Position = {
            Position {
                offset: state.tracker.get_offset(),
                position: state.tracker.get_position()
            }
        }

    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document() {
        let input = "// this comment line is ignored
= Document Title
Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>
v2.9, 01-09-2024: Fall incarnation
:description: The document's description.
:sectanchors:
:url-repo: https://my-git-repo.com";
        let mut state = ParserState::default();
        let result = document_parser::document(input, &mut state)
            .unwrap()
            .unwrap();
        dbg!(&result);
        let header = result.header.unwrap();
        assert_eq!(header.title.len(), 1);
        assert_eq!(
            header.title[0],
            InlineNode::PlainText(Plain {
                content: "Document Title".to_string(),
                location: Location {
                    absolute_start: 34,
                    absolute_end: 48,
                    start: crate::Position { line: 2, column: 3 },
                    end: crate::Position {
                        line: 2,
                        column: 17
                    },
                }
            })
        );
        assert_eq!(header.authors.len(), 2);
        assert_eq!(header.authors[0].first_name, "Lorn_Kismet");
        assert_eq!(header.authors[0].middle_name, Some("R.".to_string()));
        assert_eq!(header.authors[0].last_name, "Lee");
        assert_eq!(header.authors[0].initials, "LRL");
        assert_eq!(
            header.authors[0].email,
            Some("kismet@asciidoctor.org".to_string())
        );
        assert_eq!(header.authors[1].first_name, "Norberto");
        assert_eq!(header.authors[1].middle_name, Some("M.".to_string()));
        assert_eq!(header.authors[1].last_name, "Lopes");
        assert_eq!(header.authors[1].initials, "NML");
        assert_eq!(
            header.authors[1].email,
            Some("nlopesml@gmail.com".to_string())
        );
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("v2.9".to_string()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("01-09-2024".to_string()))
        );
        assert_eq!(
            state.document_attributes.get("revremark"),
            Some(&AttributeValue::String("Fall incarnation".to_string()))
        );
        assert_eq!(
            state.document_attributes.get("description"),
            Some(&AttributeValue::String(
                "The document's description.".to_string()
            ))
        );
        assert_eq!(
            state.document_attributes.get("sectanchors"),
            Some(&AttributeValue::Bool(true))
        );
        assert_eq!(
            state.document_attributes.get("url-repo"),
            Some(&AttributeValue::String(
                "https://my-git-repo.com".to_string()
            ))
        );
    }

    #[test]
    fn test_authors() {
        let input =
            "Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>";
        let mut state = ParserState::default();
        let result = document_parser::authors(input, &mut state).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].first_name, "Lorn_Kismet");
        assert_eq!(result[0].middle_name, Some("R.".to_string()));
        assert_eq!(result[0].last_name, "Lee");
        assert_eq!(result[0].initials, "LRL");
        assert_eq!(result[0].email, Some("kismet@asciidoctor.org".to_string()));
        assert_eq!(result[1].first_name, "Norberto");
        assert_eq!(result[1].middle_name, Some("M.".to_string()));
        assert_eq!(result[1].last_name, "Lopes");
        assert_eq!(result[1].initials, "NML");
        assert_eq!(result[1].email, Some("nlopesml@gmail.com".to_string()));
    }

    #[test]
    fn test_author() {
        let input = "Norberto M. Lopes supa dough <nlopesml@gmail.com>";
        let mut state = ParserState::default();
        let result = document_parser::author(input, &mut state).unwrap();
        assert_eq!(result.first_name, "Norberto");
        assert_eq!(result.middle_name, Some("M.".to_string()));
        assert_eq!(result.last_name, "Lopes supa dough");
        assert_eq!(result.initials, "NML");
        assert_eq!(result.email, Some("nlopesml@gmail.com".to_string()));
    }

    #[test]
    fn test_revision_full() {
        let input = "v2.9, 01-09-2024: Fall incarnation";
        let mut state = ParserState::default();
        document_parser::revision(input, &mut state).unwrap();
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("v2.9".to_string()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("01-09-2024".to_string()))
        );
        assert_eq!(
            state.document_attributes.get("revremark"),
            Some(&AttributeValue::String("Fall incarnation".to_string()))
        );
    }

    #[test]
    fn test_revision_with_date_no_remark() {
        let input = "v2.9, 01-09-2024";
        let mut state = ParserState::default();
        document_parser::revision(input, &mut state).unwrap();
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("v2.9".to_string()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("01-09-2024".to_string()))
        );
        assert_eq!(state.document_attributes.get("revremark"), None);
    }

    #[test]
    fn test_revision_no_date_with_remark() {
        let input = "v2.9: Fall incarnation";
        let mut state = ParserState::default();
        document_parser::revision(input, &mut state).unwrap();
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("v2.9".to_string()))
        );
        assert_eq!(state.document_attributes.get("revdate"), None);
        assert_eq!(
            state.document_attributes.get("revremark"),
            Some(&AttributeValue::String("Fall incarnation".to_string()))
        );
    }

    #[test]
    fn test_revision_no_date_no_remark() {
        let input = "v2.9";
        let mut state = ParserState::default();
        document_parser::revision(input, &mut state).unwrap();
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("v2.9".to_string()))
        );
        assert_eq!(state.document_attributes.get("revdate"), None);
        assert_eq!(state.document_attributes.get("revremark"), None);
    }

    #[test]
    fn test_document_comment_start() {
        let input = "// this comment line is ignored";
        let mut state = ParserState::default();
        let result = document_parser::document(input, &mut state)
            .unwrap()
            .unwrap();
        assert_eq!(result.header, None);
        assert_eq!(result.blocks.len(), 0);
        assert!(result.attributes.is_empty());
        assert_eq!(result.location.absolute_start, 0);
        assert_eq!(result.location.absolute_end, 31);
        assert_eq!(result.location.start.line, 1);
        assert_eq!(result.location.start.column, 1);
        assert_eq!(result.location.end.line, 1);
        assert_eq!(result.location.end.column, 32);
    }

    #[test]
    fn test_document_title() {
        let input = "= Document Title";
        let mut state = ParserState::default();
        let result = document_parser::document_title(input, &mut state).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            InlineNode::PlainText(Plain {
                content: "Document Title".to_string(),
                location: Location {
                    absolute_start: 2,
                    absolute_end: 16,
                    start: crate::Position { line: 1, column: 3 },
                    end: crate::Position {
                        line: 1,
                        column: 17
                    },
                }
            })
        );
    }

    #[test]
    fn test_header_with_title_and_authors() {
        let input = "= Document Title
Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>";
        let mut state = ParserState::default();
        let result = document_parser::header(input, &mut state).unwrap().unwrap();
        assert_eq!(result.title.len(), 1);
        assert_eq!(
            result.title[0],
            InlineNode::PlainText(Plain {
                content: "Document Title".to_string(),
                location: Location {
                    absolute_start: 2,
                    absolute_end: 16,
                    start: crate::Position { line: 1, column: 3 },
                    end: crate::Position {
                        line: 1,
                        column: 17
                    },
                }
            })
        );
        assert_eq!(result.authors.len(), 2);
        assert_eq!(result.authors[0].first_name, "Lorn_Kismet");
        assert_eq!(result.authors[0].middle_name, Some("R.".to_string()));
        assert_eq!(result.authors[0].last_name, "Lee");
        assert_eq!(result.authors[0].initials, "LRL");
        assert_eq!(
            result.authors[0].email,
            Some("kismet@asciidoctor.org".to_string())
        );
        assert_eq!(result.authors[1].first_name, "Norberto");
        assert_eq!(result.authors[1].middle_name, Some("M.".to_string()));
        assert_eq!(result.authors[1].last_name, "Lopes");
        assert_eq!(result.authors[1].initials, "NML");
        assert_eq!(
            result.authors[1].email,
            Some("nlopesml@gmail.com".to_string())
        );
    }
}
