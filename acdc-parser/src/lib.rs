mod error;
mod model;

pub use model::{
    AttributeEntry, Author, Block, DelimitedBlock, DelimitedBlockType, Document, Header, ListItem,
    Location, OrderedList, Paragraph, Parser, Position, Revision, Section, UnorderedList,
};
use pest::{
    iterators::{Pair, Pairs},
    Parser as _,
};
use pest_derive::Parser;
use tracing::instrument;

pub use error::{Detail as ErrorDetail, Error};

#[derive(Debug)]
pub struct PestParser;

#[derive(Parser, Debug)]
#[grammar = "../grammar/block.pest"]
#[grammar = "../grammar/core.pest"]
#[grammar = "../grammar/list.pest"]
#[grammar = "../grammar/delimited.pest"]
#[grammar = "../grammar/document.pest"]
#[grammar = "../grammar/asciidoc.pest"]
struct InnerPestParser;

impl crate::model::Parser for PestParser {
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
        let input = normalize(input);
        match InnerPestParser::parse(Rule::document, &input) {
            Ok(pairs) => parse_document(pairs),
            Err(e) => {
                dbg!(&e);
                Err(Error::Parse(e.to_string()))
            }
        }
    }
}

fn normalize(input: &str) -> String {
    input
        .lines()
        .map(str::trim_end)
        .collect::<Vec<&str>>()
        .join("\n")
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

    build_section_tree(&mut content)?;
    validate_section_block_level(&content, None)?;

    Ok(Document {
        header: document_header,
        content,
    })
}

// Build a tree of sections from the content blocks.
fn build_section_tree(document: &mut Vec<Block>) -> Result<(), Error> {
    let mut current_layers = document.clone();
    let mut stack: Vec<Block> = Vec::new();

    current_layers.reverse();

    let mut kept_layers = Vec::new();
    for block in current_layers.drain(..) {
        if let Block::Section(section) = block {
            if stack.is_empty() {
                kept_layers.push(Block::Section(section));
                continue;
            }

            let mut section = section;
            while let Some(block_from_stack) = stack.pop() {
                section.location.end = match &block_from_stack {
                    Block::Section(section) => section.location.end.clone(),
                    Block::DelimitedBlock(delimited_block) => delimited_block.location.end.clone(),
                    // We don't use paragraph because we don't calculate positions for paragraphs yet
                    Block::Paragraph(_) => section.location.end.clone(),
                    _ => todo!(),
                };
                section.content.push(block_from_stack);
            }
            kept_layers.push(Block::Section(section));
        } else {
            stack.push(block);
        }
    }

    stack.reverse();
    // Add the remaining blocks to the kept_layers
    while let Some(block_from_stack) = stack.pop() {
        kept_layers.push(block_from_stack);
    }

    if !kept_layers.is_empty() {
        let mut i = 0;
        while i < kept_layers.len() - 1 {
            let should_move = {
                if let (Some(Block::Section(section)), Some(Block::Section(next_section))) =
                    (kept_layers.get(i), kept_layers.get(i + 1))
                {
                    match next_section.level.cmp(&(section.level - 1)) {
                        std::cmp::Ordering::Greater => false,
                        std::cmp::Ordering::Equal => true,
                        std::cmp::Ordering::Less => {
                            let error_detail = ErrorDetail {
                                location: next_section.location.clone(),
                            };
                            return Err(Error::NestedSectionLevelMismatch(
                                error_detail,
                                section.level - 1,
                                section.level,
                            ));
                        }
                    }
                } else {
                    false
                }
            };

            if should_move {
                if let Some(Block::Section(current_section)) = kept_layers.get(i).cloned() {
                    if let Some(Block::Section(parent_section)) = kept_layers.get_mut(i + 1) {
                        parent_section.location.end = match &current_section.content.last() {
                            Some(Block::Section(section)) => section.location.end.clone(),
                            Some(Block::DelimitedBlock(delimited_block)) => {
                                delimited_block.location.end.clone()
                            }
                            // We don't use paragraph because we don't calculate positions for paragraphs yet
                            Some(Block::Paragraph(_)) => current_section.location.end.clone(),
                            _ => todo!(),
                        };
                        parent_section.content.push(Block::Section(current_section));
                        kept_layers.remove(i);
                    } else {
                        return Err(Error::Parse("expected a section".to_string()));
                    }
                }
            } else {
                i += 1;
            }
        }
        kept_layers.reverse();
    }
    *document = kept_layers;
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
        return Ok(vec![parse_paragraph(pairs)]);
    }
    let mut blocks = Vec::new();
    for pair in pairs {
        match pair.as_rule() {
            Rule::section => blocks.push(parse_section(&pair)?),
            Rule::delimited_block => blocks.push(parse_delimited_block(pair.into_inner())),
            Rule::paragraph => blocks.push(parse_paragraph(pair.into_inner())),
            Rule::block => blocks.extend(parse_block(pair.into_inner())?),
            Rule::list => blocks.push(parse_list(pair.into_inner())?),
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    Ok(blocks)
}

fn parse_paragraph(pairs: Pairs<Rule>) -> Block {
    let mut content = String::new();
    let mut attributes = Vec::new();
    let mut roles = Vec::new();

    for pair in pairs {
        match pair.as_rule() {
            Rule::paragraph_inner => content = pair.as_str().trim_end().to_string(),
            Rule::inline_attribute_list => {
                attributes.extend(parse_attribute_list(pair.into_inner()))
            }
            Rule::role_list => roles.extend(parse_role_list(pair.into_inner())),
            _ => todo!(),
        }
    }
    Block::Paragraph(Paragraph {
        location: Location {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        },
        content,
        roles,
        attributes,
    })
}

fn parse_role_list(pairs: Pairs<Rule>) -> Vec<String> {
    let mut roles = Vec::new();
    for pair in pairs {
        match pair.as_rule() {
            Rule::role => {
                roles.push(pair.as_str().to_string());
            }
            unknown => tracing::warn!(?unknown, "found a non role in a role list"),
        }
    }
    roles
}

fn parse_list(pairs: Pairs<Rule>) -> Result<Block, Error> {
    let mut title = None;
    let mut block = Block::UnorderedList(UnorderedList {
        title: None,
        items: Vec::new(),
        location: Location {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        },
    });

    for pair in pairs {
        match pair.as_rule() {
            Rule::list_title | Rule::blocktitle | Rule::title => {
                title = Some(pair.as_str().to_string());
            }
            Rule::unordered_list | Rule::ordered_list => {
                block = parse_simple_list(title.clone(), pair.into_inner())?;
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    Ok(block)
}

fn parse_simple_list(title: Option<String>, pairs: Pairs<Rule>) -> Result<Block, Error> {
    let mut location = Location {
        start: Position { line: 0, column: 0 },
        end: Position { line: 0, column: 0 },
    };

    let mut items = Vec::new();
    let mut kind = "unordered";

    for pair in pairs {
        let span = pair.as_span();
        if location.start == location.end {
            location = Location {
                start: Position {
                    line: span.start_pos().line_col().0,
                    column: span.start_pos().line_col().1,
                },
                end: Position {
                    line: span.end_pos().line_col().0,
                    column: span.end_pos().line_col().1,
                },
            };
        }

        if span.start_pos().line_col().0 < location.start.line {
            location.start.line = span.start_pos().line_col().0;
        }
        if span.start_pos().line_col().1 < location.start.column {
            location.start.column = span.start_pos().line_col().1;
        }
        location.end.line = span.end_pos().line_col().0;
        location.end.column = span.end_pos().line_col().1;

        match pair.as_rule() {
            Rule::unordered_list_item => {
                items.push(parse_list_item(pair.into_inner())?);
            }
            Rule::ordered_list_item => {
                kind = "ordered";
                items.push(parse_list_item(pair.into_inner())?);
            }
            unknown => unreachable!("{unknown:?}"),
        }
    }
    Ok(match kind {
        "ordered" => Block::OrderedList(OrderedList {
            title,
            items,
            location,
        }),
        _ => Block::UnorderedList(UnorderedList {
            title,
            items,
            location,
        }),
    })
}

fn parse_list_item(pairs: Pairs<Rule>) -> Result<ListItem, Error> {
    let mut content = Vec::new();
    let mut level = 0;
    let mut checked = None;

    for pair in pairs {
        match pair.as_rule() {
            Rule::list_item => {
                content.push(pair.as_str().to_string());
            }
            Rule::unordered_level | Rule::ordered_level => {
                level = u8::try_from(pair.as_str().chars().count())
                    .map_err(|e| Error::Parse(format!("error with list level depth: {e}")))?;
            }
            Rule::ordered_level_number => {
                let number_string = pair.as_str();
                level = number_string.parse::<u8>().map_err(|e| {
                    Error::Parse(format!(
                        "error with ordered level number {number_string}: {e}"
                    ))
                })?;
                // TODO(nlopes): implement ordered_level_number
                //
                // Do I need to? Does this make a difference? (Perhaps in providing errors
                // to the user)
            }
            Rule::checklist_item_checked => checked = Some(true),
            Rule::checklist_item_unchecked => checked = Some(false),
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    Ok(ListItem {
        level,
        checked,
        content,
    })
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
fn validate_section_block_level(content: &[Block], prior_level: Option<u8>) -> Result<(), Error> {
    let mut prior_level = prior_level;
    for (i, block) in content.iter().enumerate() {
        if let Block::Section(section) = block {
            if let Some(Block::Section(next_section)) = content.get(i + 1) {
                if next_section.level > section.level + 1 {
                    let error_detail = ErrorDetail {
                        location: next_section.location.clone(),
                    };
                    return Err(Error::NestedSectionLevelMismatch(
                        error_detail,
                        section.level,
                        section.level + 1,
                    ));
                }
            }
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
    let mut location = Location {
        start: Position { line: 0, column: 0 },
        end: Position { line: 0, column: 0 },
    };
    for pair in pairs {
        if pair.as_rule() == Rule::EOI || pair.as_rule() == Rule::comment {
            continue;
        }
        if location.start.line == 0
            && location.start.column == 0
            && location.end.line == 0
            && location.end.column == 0
        {
            location.start.line = pair.as_span().start_pos().line_col().0;
            location.start.column = pair.as_span().start_pos().line_col().1;
            location.end.line = pair.as_span().end_pos().line_col().0;
            location.end.column = pair.as_span().end_pos().line_col().1;
        }
        if pair.as_span().start_pos().line_col().0 < location.start.line {
            location.start.line = pair.as_span().start_pos().line_col().0;
        }
        if pair.as_span().start_pos().line_col().1 < location.start.column {
            location.start.column = pair.as_span().start_pos().line_col().1;
        }
        location.end.line = pair.as_span().end_pos().line_col().0;
        location.end.column = pair.as_span().end_pos().line_col().1;

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
            Rule::title => {
                title = Some(pair.as_str().to_string());
            }
            Rule::attribute => {
                attributes.push(parse_attribute(pair.into_inner()));
            }
            Rule::anchor => {
                anchor = Some(pair.into_inner().as_str().to_string());
            }
            unknown => unreachable!("{unknown:?}"),
        }
    }

    Block::DelimitedBlock(DelimitedBlock {
        inner,
        anchor,
        title,
        attributes,
        location,
    })
}

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

fn parse_attribute_list(pairs: Pairs<Rule>) -> Vec<AttributeEntry> {
    let mut attributes = Vec::new();

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
    use crate::model::Parser;

    #[rstest::rstest]
    #[trace]
    fn for_each_file(#[files("fixtures/tests/**/*.adoc")] path: std::path::PathBuf) {
        let parser = PestParser;
        let test_file_path = path.with_extension("test");

        // We do this check because we have files that won't have a test file, namely ones
        // that are supposed to error out!
        if test_file_path.exists() {
            let result = parser
                .parse(&std::fs::read_to_string(&path).unwrap())
                .unwrap();
            let test: Document =
                serde_json::from_str(&std::fs::read_to_string(test_file_path).unwrap()).unwrap();
            assert_eq!(test, result);
        } else {
            tracing::warn!("no test file found for {:?}", path);
        }
    }

    #[test]
    fn test_section_with_invalid_subsection() {
        let parser = PestParser;
        let result = parser
            .parse(include_str!(
                "../fixtures/tests/section_with_invalid_subsection.adoc"
            ))
            .unwrap_err();
        if let Error::NestedSectionLevelMismatch(ref detail, 2, 3) = result {
            assert_eq!(
                &ErrorDetail {
                    location: Location {
                        start: Position { line: 3, column: 1 },
                        end: Position { line: 7, column: 1 }
                    }
                },
                detail
            );
        } else {
            panic!("unexpected error: {result:?}");
        }
    }

    #[test]
    fn test_blah() {
        let result = PestParser
            .parse(
                "[[cpu,CPU]]Central Processing Unit (CPU)::
                The brain of the computer.

                [[hard-drive]]Hard drive::
                Permanent storage for operating system and/or user files.",
            )
            .unwrap();
        dbg!(&result);
        panic!()
    }

    #[test]
    fn test_mdbasics_adoc() {
        // let parser = PestParser;
        // let result = parser
        //     .parse(include_str!("../fixtures/samples/mdbasics.adoc"))
        //     .unwrap();
        // dbg!(&result.content[3]);
        // panic!()
    }
}
