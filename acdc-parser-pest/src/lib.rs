use acdc_parser::{
    AttributeEntry, Author, Block, DelimitedBlock, DelimitedBlockType, Document, Error,
    ErrorDetail, Header, Location, Position, Revision, Section,
};
use pest::{
    iterators::{Pair, Pairs},
    Parser,
};
use pest_derive::Parser;
use tracing::instrument;

#[derive(Debug)]
pub struct PestParser;

#[derive(Parser, Debug)]
#[grammar = "../grammar/block.pest"]
#[grammar = "../grammar/core.pest"]
#[grammar = "../grammar/delimited.pest"]
#[grammar = "../grammar/document.pest"]
#[grammar = "../grammar/asciidoc.pest"]
struct InnerPestParser;

impl acdc_parser::Parser for PestParser {
    /// Parse the input string into a Document.
    ///
    /// # Arguments
    ///
    /// * `input` - A string slice that holds the input to be parsed.
    ///
    /// # Returns
    ///
    /// A Result containing the parsed Document or an Error.
    ///
    /// # Errors
    ///
    /// Returns an Error if the input string cannot be parsed.
    #[instrument]
    fn parse(&self, input: &str) -> Result<Document, Error> {
        match InnerPestParser::parse(Rule::document, input) {
            Ok(pairs) => parse_document(pairs),
            Err(e) => {
                dbg!(&e);
                Err(Error::Parse(e.to_string()))
            }
        }
    }
}

fn parse_document(pairs: Pairs<Rule>) -> Result<Document, Error> {
    let mut document_header = None;
    let mut content = Vec::new();

    for pair in pairs {
        match pair.as_rule() {
            Rule::document_header => {
                document_header = Some(parse_document_header(pair.into_inner()));
            }
            Rule::block => {
                content.extend(parse_block(pair.into_inner())?);
            }
            Rule::comment | Rule::EOI => {}
            unknown => unimplemented!("{:?}", unknown),
        }
    }

    dbg!(&content);
    build_section_tree(&mut content)?;
    validate_section_block_level(&content, None)?;

    Ok(Document {
        header: document_header,
        content,
    })
}

// Build a tree of sections from the content blocks.
//
// This function modifies the content vector in place.
//
// The function assumes that the content vector is sorted by the order of appearance in the document.
//
// The approach is to iterate over the content blocks and build a tree of sections.
// The tree is built by keeping track of the current section level and the parent section.
// When a new section is found, it is added to the parent's content.
// If the new section found is at a higher level (and strictly one level more) than its parent section, the new section is added to the parent's content vector.
// If the new section found is at the same level as the current section, the new section is added to the parent section's content.
// If the new section is at a lower level than the current section, the new section is added to the parent section's content.
// If the new section is at a higher leve and more than one level more than the current section, the function returns an error.
fn build_section_tree(document: &mut Vec<Block>) -> Result<(), Error> {
    let mut lowest_section_level = 255;
    let mut current_section_level = 0;
    let mut parent_section: Option<&mut Section> = None;
    let mut indices_to_remove = Vec::new();

    let document_copy = document.clone();

    for (i, block) in document.iter_mut().enumerate() {
        if let Block::Section(section) = block {
            let section_copy = section.clone();
            let current_level = section.level;
            if current_level == lowest_section_level {
                continue;
            }
            if parent_section.is_none() {
                parent_section = Some(section);
                current_section_level = current_level;
                if current_level < lowest_section_level {
                    lowest_section_level = current_level;
                }
            }
            if current_level == current_section_level + 1 {
                current_section_level = current_level;
                let new_block = document_copy[i].clone();
                if let Some(parent) = parent_section.as_mut() {
                    // update the parent's end location to the end of the new section
                    parent.location.end = section_copy.location.end;
                    parent.content.push(new_block);
                }
                indices_to_remove.push(i);
                parent_section = Some(section);
            } else if current_level == current_section_level {
                dbg!("same level, do nothing");
            } else if current_level == current_section_level - 1 {
                current_section_level = current_level;
                let new_block = document_copy[i].clone();
                if let Some(parent) = parent_section {
                    parent.content.push(new_block);
                }
                indices_to_remove.push(i);
                parent_section = None;
            } else {
                let error_detail = ErrorDetail {
                    location: section.location.clone(),
                };
                return Err(Error::NestedSectionLevelMismatch(
                    error_detail,
                    section.level,
                    current_section_level,
                ));
            }
        }
    }

    for i in indices_to_remove.iter().rev() {
        document.remove(*i);
    }

    Ok(())
}

fn parse_document_header(pairs: Pairs<Rule>) -> Header {
    let mut title = None;
    let mut subtitle = None;
    let mut authors = Vec::new();
    let mut revision = None;
    let mut attributes = Vec::new();

    for pair in pairs {
        match pair.as_rule() {
            Rule::document_title_token => {
                for inner_pair in pair.into_inner() {
                    match inner_pair.as_rule() {
                        Rule::document_title => {
                            title = Some(inner_pair.as_str().to_string());
                            // find the subtitle by looking for the last colon in title
                            // andsetting title to everything before the last colon and
                            // subtitle to everything after the last colon
                            if let Some(colon_index) = title.as_ref().unwrap().rfind(':') {
                                subtitle = Some(
                                    title.as_ref().unwrap()[colon_index + 1..]
                                        .trim()
                                        .to_string(),
                                );
                                title =
                                    Some(title.as_ref().unwrap()[..colon_index].trim().to_string());
                            }
                        }
                        unknown => unreachable!("{:?}", unknown),
                    }
                }
            }
            Rule::author => {
                let author = parse_author(pair.into_inner());
                authors.push(author);
            }
            Rule::revision_line => {
                let inner_pairs = pair.into_inner();
                let mut revision_number = String::new();
                let mut revision_date = None;
                let mut revision_remark = None;

                for pair in inner_pairs {
                    match pair.as_rule() {
                        Rule::revision_number => {
                            revision_number = pair.as_str().to_string();
                        }
                        Rule::revision_date => {
                            revision_date = Some(pair.as_str().to_string());
                        }
                        Rule::revision_remark => {
                            revision_remark = Some(pair.as_str().to_string());
                        }
                        unknown => unreachable!("{:?}", unknown),
                    }
                }
                revision = Some(Revision {
                    number: revision_number,
                    date: revision_date,
                    remark: revision_remark,
                });
            }
            Rule::document_attribute => {
                let mut inner_pairs = pair.into_inner();
                let name = inner_pairs.next().map(|p| p.as_str().to_string());
                let value = inner_pairs.next().map(|p| p.as_str().to_string());
                attributes.push(AttributeEntry { name, value });
            }
            unknown => unreachable!("{:?}", unknown),
        }
    }

    Header {
        title,
        subtitle,
        authors,
        revision,
        attributes,
    }
}

fn parse_author(pairs: Pairs<Rule>) -> Author {
    let mut first_name = String::new();
    let mut middle_name = None;
    let mut last_name = String::new();
    let mut email = None;

    for pair in pairs {
        match pair.as_rule() {
            Rule::author_first_name => {
                first_name = pair.as_str().to_string();
            }
            Rule::author_middle_name => middle_name = Some(pair.as_str().to_string()),
            Rule::author_last_name => {
                last_name = pair.as_str().to_string();
            }
            Rule::author_email => {
                email = Some(pair.as_str().to_string()).map(|s| s.to_string());
            }
            unknown => unreachable!("{unknown:?}"),
        }
    }

    Author {
        first_name,
        middle_name,
        last_name,
        email,
    }
}

fn parse_block(pairs: Pairs<Rule>) -> Result<Vec<Block>, Error> {
    if pairs.peek().is_none() {
        // TODO(nlopes): confirm if this is the correct behavior
        tracing::warn!(?pairs, "empty block");
        return Ok(vec![Block::Paragraph(
            pairs.as_str().trim_end().to_string(),
        )]);
    }
    let mut blocks = Vec::new();
    for pair in pairs {
        match pair.as_rule() {
            Rule::section => blocks.push(parse_section(&pair)?),
            Rule::delimited_block => blocks.push(parse_delimited_block(pair.into_inner())),
            Rule::paragraph => blocks.push(Block::Paragraph(pair.as_str().trim_end().to_string())),
            Rule::block => blocks.extend(parse_block(pair.into_inner())?),
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    Ok(blocks)
}

fn parse_section(pair: &Pair<Rule>) -> Result<Block, Error> {
    let mut title = String::new();
    let mut level = 0;
    let mut content = Vec::new();

    for inner_pair in pair.clone().into_inner() {
        match inner_pair.as_rule() {
            Rule::section_title => {
                title = inner_pair.as_str().to_string();
            }
            Rule::section_level => {
                level = u8::try_from(inner_pair.as_str().chars().count())
                    .map_err(|e| Error::Parse(format!("error with section level depth: {e}")))?
                    - 1;
            }
            Rule::section_content => {
                let inner = inner_pair.clone().into_inner();
                if inner.peek().is_none() {
                    let pairs = InnerPestParser::parse(Rule::document, inner_pair.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    content.extend(parse_block(pairs)?);
                } else {
                    for pair in inner {
                        content.extend(parse_block(pair.into_inner())?);
                    }
                }
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{:?}", unknown),
        }
    }

    Ok(Block::Section(Section {
        title,
        level,
        content,
        location: Location {
            start: Position {
                line: pair.as_span().start_pos().line_col().0,
                column: pair.as_span().start_pos().line_col().1,
            },
            end: Position {
                line: pair.as_span().end_pos().line_col().0,
                column: pair.as_span().end_pos().line_col().1,
            },
        },
    }))
}

// Validate that the block level is correct for the section level.
//
// For example, a section level 1 should only contain blocks of level 2 or higher.
fn validate_section_block_level(
    content: &Vec<Block>,
    prior_level: Option<u8>,
) -> Result<(), Error> {
    let mut prior_level = prior_level;
    for block in content {
        if let Block::Section(section) = block {
            if let Some(parent_level) = prior_level {
                if section.level == parent_level + 1 {
                    prior_level = Some(section.level);
                    return validate_section_block_level(&section.content, prior_level);
                }
                let error_detail = ErrorDetail {
                    location: section.location.clone(),
                };
                return Err(Error::NestedSectionLevelMismatch(
                    error_detail,
                    section.level,
                    parent_level + 1,
                ));
            }
            prior_level = Some(section.level);
            return validate_section_block_level(&section.content, prior_level);
        }
    }
    Ok(())
}

fn parse_delimited_block(pairs: Pairs<Rule>) -> Block {
    let mut inner = DelimitedBlockType::DelimitedComment(String::new());
    let mut title = None;
    let mut attributes = Vec::new();
    let mut anchor = None;

    for pair in pairs {
        match pair.as_rule() {
            Rule::delimited_comment => {
                inner =
                    DelimitedBlockType::DelimitedComment(pair.into_inner().as_str().to_string());
            }
            Rule::delimited_example => {
                inner =
                    DelimitedBlockType::DelimitedExample(pair.into_inner().as_str().to_string());
            }
            Rule::delimited_pass => {
                inner = DelimitedBlockType::DelimitedPass(pair.into_inner().as_str().to_string());
            }
            Rule::delimited_quote => {
                inner = DelimitedBlockType::DelimitedQuote(pair.into_inner().as_str().to_string());
            }
            Rule::delimited_listing => {
                dbg!(&pair.clone().into_inner().as_str());
                inner =
                    DelimitedBlockType::DelimitedListing(pair.into_inner().as_str().to_string());
            }
            Rule::delimited_literal => {
                inner =
                    DelimitedBlockType::DelimitedLiteral(pair.into_inner().as_str().to_string());
            }
            Rule::delimited_open => {
                inner = DelimitedBlockType::DelimitedOpen(pair.into_inner().as_str().to_string());
            }
            Rule::delimited_sidebar => {
                inner =
                    DelimitedBlockType::DelimitedSidebar(pair.into_inner().as_str().to_string());
            }
            Rule::delimited_table => {
                inner = DelimitedBlockType::DelimitedTable(pair.into_inner().as_str().to_string());
            }
            Rule::blocktitle => {
                title = Some(pair.into_inner().as_str().to_string());
            }
            Rule::attribute_list => {
                attributes.extend(parse_attribute_list(pair.into_inner()));
            }
            Rule::anchor => {
                anchor = Some(pair.into_inner().as_str().to_string());
            }
            unknown => unreachable!("{unknown:?}"),
        }
    }

    dbg!(Block::DelimitedBlock(DelimitedBlock {
        inner,
        anchor,
        title,
        attributes,
    }))
}

fn parse_attribute_list(pairs: Pairs<Rule>) -> Vec<AttributeEntry> {
    let mut attributes = Vec::new();

    fn parse_attribute(pairs: Pairs<Rule>) -> AttributeEntry {
        let mut name = None;
        let mut value = None;

        for pair in pairs {
            match pair.as_rule() {
                Rule::attribute_name => {
                    name = Some(pair.as_str().to_string());
                }
                Rule::attribute_value => {
                    value = Some(pair.as_str().to_string());
                }
                unknown => unreachable!("{unknown:?}"),
            }
        }

        AttributeEntry { name, value }
    }

    for pair in pairs {
        match pair.as_rule() {
            Rule::attribute => {
                attributes.push(parse_attribute(pair.into_inner()));
            }
            unknown => unreachable!("{unknown:?}"),
        }
    }
    attributes
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::Parser;

    #[test]
    fn test_empty() {
        let parser = PestParser;
        let result = parser.parse("").unwrap();
        assert_eq!(result.header, None);
        assert_eq!(result.content.len(), 0);
    }

    #[test]
    fn test_basic_title_with_subtitle() {
        let parser = PestParser;
        let result = parser
            .parse(
                "
// this comment line is ignored
= Document Title: this is the subtitle

body text
",
            )
            .unwrap();
        assert_eq!(
            Document {
                header: Some(Header {
                    title: Some("Document Title".to_string()),
                    subtitle: Some("this is the subtitle".to_string()),
                    authors: vec![],
                    revision: None,
                    attributes: vec![],
                }),
                content: vec![Block::Paragraph("body text".to_string())],
            },
            result
        );
    }

    #[test]
    fn test_basic_title_with_double_colon_with_subtitle() {
        let parser = PestParser;
        let result = parser
            .parse(
                "
// this comment line is ignored
= Document Title: this is not the subtitle: this is the subtitle

body text
",
            )
            .unwrap();
        assert_eq!(
            Document {
                header: Some(Header {
                    title: Some("Document Title: this is not the subtitle".to_string()),
                    subtitle: Some("this is the subtitle".to_string()),
                    authors: vec![],
                    revision: None,
                    attributes: vec![],
                }),
                content: vec![Block::Paragraph("body text".to_string())],
            },
            result
        );
    }

    #[test]
    fn test_basic_header() {
        let parser = PestParser;
        let result = parser
            .parse(
                "
// this comment line is ignored
= Document Title
Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>
v2.9, 01-09-2024: Fall incarnation
:description: The document's description.
:sectanchors:
:url-repo: https://my-git-repo.com

The document body starts here.
",
            )
            .unwrap();
        assert_eq!(
            Document {
                header: Some(Header {
                    title: Some("Document Title".to_string()),
                    subtitle: None,
                    authors: vec![
                        Author {
                            first_name: "Lorn_Kismet".to_string(),
                            middle_name: Some("R.".to_string()),
                            last_name: "Lee".to_string(),
                            email: Some("kismet@asciidoctor.org".to_string()),
                        },
                        Author {
                            first_name: "Norberto".to_string(),
                            middle_name: Some("M.".to_string()),
                            last_name: "Lopes".to_string(),
                            email: Some("nlopesml@gmail.com".to_string()),
                        },
                    ],
                    revision: Some(Revision {
                        number: "v2.9".to_string(),
                        date: Some("01-09-2024".to_string()),
                        remark: Some("Fall incarnation".to_string()),
                    }),
                    attributes: vec![
                        AttributeEntry {
                            name: Some("description".to_string()),
                            value: Some("The document's description.".to_string()),
                        },
                        AttributeEntry {
                            name: Some("sectanchors".to_string()),
                            value: None,
                        },
                        AttributeEntry {
                            name: Some("url-repo".to_string()),
                            value: Some("https://my-git-repo.com".to_string()),
                        },
                    ],
                }),
                content: vec![Block::Paragraph(
                    "The document body starts here.".to_string()
                )],
            },
            result
        );
    }

    #[test]
    fn test_multiline_description() {
        let parser = PestParser;
        let result = parser.parse(
            "= The Intrepid Chronicles
Kismet Lee <kismet@asciidoctor.org>
:description: A story chronicling the inexplicable \
hazards and unique challenges a team must vanquish \
on their journey to finding an open source \
project's true power.

This journey begins on a bleary Monday morning.",
        );
        assert_eq!(
            Document {
                header: Some(Header {
                    title: Some("The Intrepid Chronicles".to_string()),
                    subtitle: None,
                    authors: vec![Author {
                        first_name: "Kismet".to_string(),
                        middle_name: None,
                        last_name: "Lee".to_string(),
                        email: Some("kismet@asciidoctor.org".to_string()),
                    }],
                    revision: None,
                    attributes: vec![AttributeEntry {
                        name: Some("description".to_string()),
                        value: Some(
                            "A story chronicling the inexplicable hazards and unique challenges a team must vanquish on their journey to finding an open source project's true power."
                                .to_string()
                        ),
                    }],
                }),
                content: vec![Block::Paragraph(
                    "This journey begins on a bleary Monday morning.".to_string()
                )],
            },
            result.unwrap()
        );
    }

    #[test]
    fn test_two_paragraphs() {
        let parser = PestParser;
        let result = parser
            .parse(
                "Paragraphs don't require any special markup in AsciiDoc.
A paragraph is just one or more lines of consecutive text.

To begin a new paragraph, separate it by at least one empty line from the previous paragraph or block.",
            )
            .unwrap();
        assert_eq!(
            Document {
                header: None,
                content: vec![
                    Block::Paragraph("Paragraphs don't require any special markup in AsciiDoc.\nA paragraph is just one or more lines of consecutive text.".to_string()),
                    Block::Paragraph("To begin a new paragraph, separate it by at least one empty line from the previous paragraph or block.".to_string()),
                ],
            },
            result
        );
    }

    #[test]
    fn test_boolean_attributes() {
        let parser = PestParser;
        let result = parser
            .parse(
                "= Document Title
:sectanchors:
:toc:

content",
            )
            .unwrap();
        assert_eq!(
            Document {
                header: Some(Header {
                    title: Some("Document Title".to_string()),
                    subtitle: None,
                    authors: vec![],
                    revision: None,
                    attributes: vec![
                        AttributeEntry {
                            name: Some("sectanchors".to_string()),
                            value: None,
                        },
                        AttributeEntry {
                            name: Some("toc".to_string()),
                            value: None,
                        },
                    ],
                }),
                content: vec![Block::Paragraph("content".to_string())],
            },
            result
        );
    }

    #[test]
    fn test_delimited_block() {
        let parser = PestParser;
        let result = parser
            .parse(
                "This is a paragraph.

// A comment block
// that spans multiple lines.

====
This is an example of an example block.
That's so meta.
====",
            )
            .unwrap();
        assert_eq!(
            Document {
                header: None,
                content: vec![
                    Block::Paragraph("This is a paragraph.".to_string()),
                    Block::Paragraph(
                        "// A comment block\n// that spans multiple lines.".to_string()
                    ),
                    Block::DelimitedBlock(DelimitedBlock {
                        inner: DelimitedBlockType::DelimitedExample(
                            "This is an example of an example block.\nThat's so meta.".to_string()
                        ),
                        title: None,
                        anchor: None,
                        attributes: vec![],
                    }),
                ],
            },
            result
        );
    }

    #[test]
    fn test_section() {
        let parser = PestParser;
        let result = parser
            .parse(
                "= Document Title

== Section 1

This is the content of section 1.",
            )
            .unwrap();
        assert_eq!(
            Document {
                header: Some(Header {
                    title: Some("Document Title".to_string()),
                    subtitle: None,
                    authors: vec![],
                    revision: None,
                    attributes: vec![],
                }),
                content: vec![Block::Section(Section {
                    title: "Section 1".to_string(),
                    level: 1,
                    content: vec![Block::Paragraph(
                        "This is the content of section 1.".to_string()
                    )],
                    location: Location {
                        start: Position { line: 3, column: 1 },
                        end: Position {
                            line: 5,
                            column: 34
                        }
                    },
                })],
            },
            result
        );
    }

    #[test]
    fn test_section_with_multiple_paragraphs() {
        let parser = PestParser;
        let result = parser
            .parse(
                "= Document Title

== Section 1

This is the content of section 1.

And another paragraph with content.",
            )
            .unwrap();
        assert_eq!(
            Document {
                header: Some(Header {
                    title: Some("Document Title".to_string()),
                    subtitle: None,
                    authors: vec![],
                    revision: None,
                    attributes: vec![],
                }),
                content: vec![Block::Section(Section {
                    title: "Section 1".to_string(),
                    level: 1,
                    content: vec![
                        Block::Paragraph("This is the content of section 1.".to_string()),
                        Block::Paragraph("And another paragraph with content.".to_string())
                    ],
                    location: Location {
                        start: Position { line: 3, column: 1 },
                        end: Position {
                            line: 7,
                            column: 36
                        }
                    },
                })],
            },
            result
        );
    }

    #[test]
    fn test_section_with_invalid_subsection() {
        let parser = PestParser;
        let result = parser
            .parse(
                "= Document Title

== Section 1

This is the content of section 1.

==== Section 4

This is the content of section 4.",
            )
            .unwrap_err();
        if let Error::NestedSectionLevelMismatch(ref detail, 3, 1) = result {
            assert_eq!(
                &ErrorDetail {
                    location: Location {
                        start: Position { line: 7, column: 1 },
                        end: Position {
                            line: 9,
                            column: 34
                        }
                    }
                },
                detail
            );
        } else {
            panic!("unexpected error: {result:?}");
        }
    }

    #[test]
    fn test_section_with_valid_subsection_interleaved() {
        let parser = PestParser;
        let result = parser
            .parse(
                "= Document Title

== First Section

Content of first section

=== Nested Section

Content of nested section

== Second Section

Content of second section",
            )
            .unwrap();
        assert_eq!(
            Document {
                header: Some(Header {
                    title: Some("Document Title".to_string()),
                    subtitle: None,
                    authors: vec![],
                    revision: None,
                    attributes: vec![],
                }),
                content: vec![
                    Block::Section(Section {
                        title: "First Section".to_string(),
                        level: 1,
                        content: vec![
                            Block::Paragraph("Content of first section".to_string()),
                            Block::Section(Section {
                                title: "Nested Section".to_string(),
                                level: 2,
                                content: vec![Block::Paragraph(
                                    "Content of nested section".to_string()
                                )],
                                location: Location {
                                    start: Position { line: 7, column: 1 },
                                    end: Position {
                                        line: 11,
                                        column: 1
                                    }
                                },
                            }),
                        ],
                        location: Location {
                            start: Position { line: 3, column: 1 },
                            end: Position {
                                line: 11,
                                column: 1
                            }
                        },
                    }),
                    Block::Section(Section {
                        title: "Second Section".to_string(),
                        level: 1,
                        content: vec![Block::Paragraph("Content of second section".to_string())],
                        location: Location {
                            start: Position {
                                line: 11,
                                column: 1
                            },
                            end: Position {
                                line: 13,
                                column: 26
                            }
                        },
                    }),
                ],
            },
            result
        );
    }

    #[test]
    fn test_delimited_block_with_header() {
        let parser = PestParser;
        let result = parser
            .parse(
                ".Specify GitLab CI stages
[source,yaml]
----
image: node:16-buster
stages: [ init, verify, deploy ]
----",
            )
            .unwrap();
        assert_eq!(
            Document {
                header: None,
                content: vec![Block::DelimitedBlock(DelimitedBlock {
                    inner: DelimitedBlockType::DelimitedListing(
                        "image: node:16-buster\nstages: [ init, verify, deploy ]".to_string()
                    ),
                    anchor: None,
                    title: Some("Specify GitLab CI stages".to_string()),
                    attributes: vec![
                        AttributeEntry {
                            name: None,
                            value: Some("source".to_string()),
                        },
                        AttributeEntry {
                            name: None,
                            value: Some("yaml".to_string()),
                        },
                    ]
                })],
            },
            result
        );
    }

    #[test]
    fn test_delimited_block_within_section() {
        let parser = PestParser;
        let result = parser
            .parse(
                "## Section 1

Something is up. Let's see.

[source,yaml]
----
image: node:16-buster
stages: [ init, verify, deploy ]
----

And that's it.",
            )
            .unwrap();
        assert_eq!(
            Document {
                header: None,
                content: vec![Block::Section(Section {
                    title: "Section 1".to_string(),
                    level: 1,
                    content: vec![
                        Block::Paragraph("Something is up. Let's see.".to_string()),
                        Block::DelimitedBlock(DelimitedBlock {
                            inner: DelimitedBlockType::DelimitedListing(
                                "image: node:16-buster\nstages: [ init, verify, deploy ]"
                                    .to_string()
                            ),
                            anchor: None,
                            title: None,
                            attributes: vec![
                                AttributeEntry {
                                    name: None,
                                    value: Some("source".to_string()),
                                },
                                AttributeEntry {
                                    name: None,
                                    value: Some("yaml".to_string()),
                                },
                            ]
                        }),
                        Block::Paragraph("And that's it.".to_string())
                    ],
                    location: Location {
                        start: Position { line: 1, column: 1 },
                        end: Position {
                            line: 11,
                            column: 15
                        }
                    },
                }),],
            },
            result
        );
    }

    #[test]
    fn test_mdbasics_adoc() {
        let parser = PestParser;
        let result = parser
            .parse(include_str!("../../fixtures/samples/mdbasics.adoc"))
            .unwrap_err();
    }
}
