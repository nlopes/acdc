use crate::{
    grammar::PositionTracker, model::DiscreteHeaderSection, Anchor, AttributeValue, Author, Block,
    BlockMetadata, Document, DocumentAttribute, DocumentAttributes, ElementAttributes, Error,
    Header, InlineNode, Location, Plain, Section, TableOfContents,
};

#[derive(Debug, Default)]
pub(crate) struct ParserState {
    pub(crate) document_attributes: DocumentAttributes,
    pub(crate) tracker: PositionTracker,
}

#[derive(Debug)]
struct Position {
    offset: usize,
    position: crate::Position,
}

#[derive(Debug)]
// Used purely in the grammar to break down the block metadata lines into its different
// types.
enum BlockMetadataLine {
    Anchor(Anchor),
    Attributes((bool, BlockMetadata)),
    Title(String),
}

#[derive(Debug)]
// Used purely inside the grammar
enum BlockStyle {
    Id(String),
    Role(String),
    Option(String),
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
                        // The start position is the start of the document, but if the end offset is 0, we set it to 0
                        start: if end.offset == 0 { crate::Position{
                            column: 0,
                            .. start.position
                        }} else {start.position},
                        end: if end.offset == 0 { crate::Position{
                            column: 0,
                            .. end.position
                        }} else {end.position},
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

        rule document_title_token() = t:$("=" / "#") { state.tracker.advance(t); }

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
                if state.document_attributes.contains_key("revnumber") {
                    tracing::warn!("Revision number found in revision line but ignoring due to being set through attribute entries.");
                } else {
                    state.document_attributes.insert("revnumber".to_string(), AttributeValue::String(number.to_string()));
                }
                if let Some(date) = date {
                    if state.document_attributes.contains_key("revdate") {
                        tracing::warn!("Revision date found in revision line but ignoring due to being set through attribute entries.");
                    } else {
                        state.document_attributes.insert("revdate".to_string(), AttributeValue::String(date.to_string()));
                    }
                }
                if let Some(remark) = remark {
                    if state.document_attributes.contains_key("revremark") {
                        tracing::warn!("Revision remark found in revision line but ignoring due to being set through attribute entries.");
                    } else {
                        state.document_attributes.insert("revremark".to_string(), AttributeValue::String(remark.to_string()));
                    }
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

        pub(crate) rule block() -> Block
            = block:(document_attribute_block() / section() / block_generic())
        {
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
            = start:position() block_metadata:block_metadata() eol()
            section_level:section_level() ws()
            title_start:position() title:section_title() title_end:position() eol()*<2,2>
            content:section_content()* end:position() {
                let level = section_level.len().try_into().unwrap_or(0);
                let location = Location {
                    absolute_start: start.offset,
                    absolute_end: end.offset,
                    start: start.position,
                    end: end.position
                };
                // TODO(nlopes): what do I do with metadata_title?!?
                let (discrete, metadata, metadata_title) = block_metadata;

                // Create a simple title with plain text
                let title_node = InlineNode::PlainText(Plain {
                    content: title,
                    location: Location {
                        absolute_start: title_start.offset,
                        absolute_end: title_end.offset,
                        start: title_start.position,
                        end: title_end.position
                    }
                });

                if discrete {
                    #[allow(clippy::used_underscore_items)]
                    return Block::_DiscreteHeaderSection(DiscreteHeaderSection {
                        anchors: metadata.anchors,
                        title: vec![title_node],
                        level,
                        location,
                        content: vec![],
                    });
                }
                Block::Section(Section {
                    metadata,
                    title: vec![title_node],
                    level,
                    content: vec![], // Simplified - we're ignoring actual content parsing
                    location
                })
            }

        rule block_metadata() -> (bool, BlockMetadata, Option<String>)
            = lines:(
                anchor:anchor() { BlockMetadataLine::Anchor(anchor) }
                / attr:attributes_line() { BlockMetadataLine::Attributes(attr) }
                / title:title_line() { BlockMetadataLine::Title(title) }
            )* {
                let mut metadata = BlockMetadata::default();
                let mut discrete = false;
                let mut title = None;

                for value in lines {
                    match value {
                        BlockMetadataLine::Anchor(value) => metadata.anchors.push(value),
                        BlockMetadataLine::Attributes((attr_discrete, attr_metadata)) => {
                            discrete = attr_discrete;
                            metadata.id = attr_metadata.id;
                            metadata.style = attr_metadata.style;
                            metadata.roles = attr_metadata.roles;
                            metadata.options = attr_metadata.options;
                            metadata.attributes = attr_metadata.attributes;
                        },
                        BlockMetadataLine::Title(value) => title = Some(value),
                        _ => unreachable!(),
                    }
                }

                (discrete, metadata, title)
            }

        rule title_line() -> String
            = period() title:$([^'\n']*) eol() {
                state.tracker.advance(title);
                title.to_string()
            }

        rule section_level() -> String
            = level:$(("=" / "#")*<1,6>) {
                state.tracker.advance(level);
                level.to_string()
            }

        rule section_title() -> String
            = title:$([^'\n']*) {
                state.tracker.advance(title);
                title.to_string()
            }

        rule section_content() -> Vec<Block>
            = b:block() eol() { vec![b] }

        pub(crate) rule block_generic() -> Block
            = "::toc::" eol() {
                Block::TableOfContents(TableOfContents {
                    location: Location::default()
                })
            }

        rule anchor() -> Anchor
            = result:(
                start:position() double_open_sb() id:$([^']' | ',' | ']']+) comma() reftext:$([^']']+) double_close_sb() eol() end:position() {
                state.tracker.advance(id);
                state.tracker.advance(reftext);
                (start, id, Some(reftext), end)
            } /
            start:position() double_open_sb() id:$([^']' | ',' | ']']+) double_close_sb() eol() end:position() {
                state.tracker.advance(id);
                (start, id, None, end)
            } /
            start:position() open_sb() "#" id:$([^']' | ',' | ']']+) comma() reftext:$([^']']+) close_sb() eol() end:position() {
                state.tracker.advance("#");
                state.tracker.advance(id);
                state.tracker.advance(reftext);
                (start, id, Some(reftext), end)
            } /
            start:position() open_sb() "#" id:$([^']' | ',' | ']']+) close_sb() eol() end:position() {
                state.tracker.advance("#");
                state.tracker.advance(id);
                (start, id, None, end)
            }) {
                let (start, id, reftext, end) = result;
                Anchor {
                    id: id.to_string(),
                    xreflabel: reftext.map(ToString::to_string),
                    location: Location {
                        absolute_start: start.offset,
                        absolute_end: end.offset,
                        start: start.position,
                        end: end.position
                    }
                }
            }

        rule attributes_line() -> (bool, BlockMetadata)
            = start:position() open_sb() content:(
                // The case in which we keep the style empty
                comma() attributes:(attribute() ** comma()) {
                    (true, false, None, attributes)
                } /
                // The case in which there is a block style and other attributes
                style:block_style() comma() attributes:(attribute() ++ comma()) {
                    (false, true, Some(style), attributes)
                } /
                // The case in which there is a block style and no other attributes
                style:block_style() {
                    (false, true, Some(style), vec![])
                } /
                // The case in which there are only attributes
                attributes:(attribute() ** comma()) {
                    (false, false, None, attributes)
                })
            close_sb() eol() end:position() {
                state.tracker.advance_by(end.offset - start.offset);
                let mut discrete = false;
                let mut style_found = false;
                let (empty, has_style, maybe_style, attributes) = content;
                let mut metadata = BlockMetadata::default();
                if let Some((maybe_attribute, id, roles, options)) = maybe_style {
                    if let Some(attribute_name) = maybe_attribute {
                        if attribute_name == "discrete" {
                            discrete = true;
                        }

                        if metadata.style.is_none() && !has_style {
                            metadata.style = Some(attribute_name);
                            style_found = true;
                        } else {
                            metadata.attributes.insert(attribute_name, AttributeValue::None);
                        }
                    }
                    metadata.id = id;
                    for role in roles {
                        metadata.roles.push(role);
                    }
                    for option in options {
                        metadata.options.push(option);
                    }
                }
                for (k, v) in attributes.into_iter().flatten() {
                    metadata.attributes.insert(k.to_string(), v);
                }
                (discrete, metadata)
            }

        // TODO(nlopes): This should return Vec<InlineNode>
        // Once I implement inlines_inner, I can come back here and fix.
        rule block_title() -> String
            = start:position() "." !['.' | ' '] title:$([^'\n']*) eol() end:position() {
                state.tracker.advance_by(end.offset - start.offset);
                title.to_string()
            }

        rule open_sb() = "[" { state.tracker.advance_by(1); }
        rule close_sb() = "]" { state.tracker.advance_by(1); }
        // This could be a double open_sb but this way we don't call advance_by twice
        rule double_open_sb() = "[[" { state.tracker.advance_by(2); }
        rule double_close_sb() = "]]" { state.tracker.advance_by(2); }
        rule comma() = "," { state.tracker.advance_by(1); }
        rule period() = "." { state.tracker.advance_by(1); }


        rule empty_style() = ""
        rule role() -> String = s:$(!(","/ "]" / "#" / "." / "%") [_]+) { s.to_string() }
        rule option() -> String = s:$("\\\"" / !("\"" / "," / "]" / "#" / "." / "%") [_]+) { s.to_string() }

        rule attribute_name() -> String = s:$((['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_'])+) { s.to_string() }

        rule attribute() -> Option<(String, AttributeValue)>
            = att:named_attribute() { att }
              / att:positional_attribute_value() {
                  Some((att, AttributeValue::None))
              }

        // Add a simple ID rule
        rule id() -> String
            = id:$((['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_'])+) { id.to_string() }

        // TODO(nlopes): this should instead return an enum
        //
        // TODO(nlopes): This is also missing the case when we have multiple options in
        // quotes separated by commas (the below doesn't work but is illustrative):
        //
        // / ("options" / "opts") "=" "\"" opts:(option() ** ",")+ "\""
        //   { Some(("options".to_string(), AttributeValue::String(opts))) }
        rule named_attribute() -> Option<(String, AttributeValue)>
            = "id" "=" id:id()
                { Some(("id".to_string(), AttributeValue::String(id))) }
              / "role" "=" role:role()
                { Some(("role".to_string(), AttributeValue::String(role))) }
              / ("options" / "opts") "=" option:option()
                { Some(("options".to_string(), AttributeValue::String(option))) }
              / name:attribute_name() "=" value:named_attribute_value()
                { Some((name, AttributeValue::String(value))) }

        // The block style is a positional attribute that is used to set the style of a block element.
        //
        // It has an optional "style", followed by the attribute shorthands.
        //
        // # - ID
        // . - role
        // % - option
        //
        // Each shorthand entry is placed directly adjacent to previous one, starting
        // immediately after the optional block style. The order of the entries does not
        // matter, except for the style, which must come first.
        rule block_style() -> (Option<String>, Option<Anchor>, Vec<String>, Vec<String>)
            = start:position() attribute:positional_attribute_value()? shorthands:(
                "#" id:block_style_id() { BlockStyle::Id(id)}
                / "." role:role() { BlockStyle::Role(role)}
                / "%" option:option() { BlockStyle::Option(option)}
            )* end:position() {
                let mut maybe_anchor = None;
                let mut roles = Vec::new();
                let mut options = Vec::new();
                for shorthand in shorthands {
                    match shorthand {
                        BlockStyle::Id(id) => maybe_anchor = Some(Anchor {
                            id,
                            xreflabel: None,
                            location: Location {
                                absolute_start: start.offset,
                                absolute_end: end.offset,
                                start: start.position.clone(),
                                end: end.position.clone()
                            }
                        }),
                        BlockStyle::Role(role) => roles.push(role),
                        BlockStyle::Option(option) => options.push(option),
                        _ => unreachable!()
                    }
                }
                (attribute, maybe_anchor, roles, options)
            }

        rule id_start_char() = ['A'..='Z' | 'a'..='z' | '_']

        rule block_style_id() -> String = s:$(id_start_char() block_style_id_subsequent_char()*) { s.to_string() }

        rule block_style_id_subsequent_char() =
            ['A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-']

        rule named_attribute_value() -> String
            = "\"" inner:inner_attribute_value() "\"" { inner }
            / s:$((!(","/ "]") [_])+) { s.to_string() }

        rule positional_attribute_value() -> String
            = s:$((!("\"" / "," / "]" / "#" / "." / "%") [_])
                 (!("\"" / "," / "]" / "#" / "%" / "=" / ".") [_])*)
        {
            state.tracker.advance(s);
            s.to_string()
        }

        rule inner_attribute_value() -> String
            = s:$(("\\\"" / (!"\"" [_]))*) { s.to_string() }

        pub rule url() -> String = proto:proto() "://" path:path() { format!("{}{}{}", proto, "://", path) }

        rule proto() -> String = s:$("https" / "http" / "ftp" / "irc" / "mailto") { s.to_string() }

        pub rule path() -> String = s:$(['A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-' | '.' | '/' | '~' ]+) { s.to_string() }

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
    #[tracing_test::traced_test]
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
    #[tracing_test::traced_test]
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
    #[tracing_test::traced_test]
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
    #[tracing_test::traced_test]
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
    #[tracing_test::traced_test]
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
    #[tracing_test::traced_test]
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
    #[tracing_test::traced_test]
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
    #[tracing_test::traced_test]
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
    #[tracing_test::traced_test]
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
    #[tracing_test::traced_test]
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
