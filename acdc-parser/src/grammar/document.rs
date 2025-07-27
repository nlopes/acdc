use crate::{
    grammar::LineMap,
    inline_preprocessing,
    model::{DiscreteHeaderSection, ListLevel},
    Admonition, AdmonitionVariant, Anchor, AttributeValue, Audio, Author, Block, BlockMetadata,
    DelimitedBlock, DelimitedBlockType, Document, DocumentAttribute, DocumentAttributes, Error,
    Header, Image, InlineNode, InlinePreprocessorParserState, Italic, LineBreak, ListItem,
    ListItemCheckedStatus, Location, OrderedList, PageBreak, Paragraph, Plain, Raw, Section,
    Source, Table, TableOfContents, ThematicBreak, UnorderedList, Video,
};

#[derive(Debug)]
pub(crate) struct ParserState {
    pub(crate) document_attributes: DocumentAttributes,
    pub(crate) line_map: LineMap,
}

impl ParserState {
    pub(crate) fn new(input: &str) -> Self {
        Self {
            document_attributes: DocumentAttributes::default(),
            line_map: LineMap::new(input),
        }
    }

    /// Create a Location from raw byte offsets
    pub(crate) fn create_location(&self, start: usize, end: usize) -> Location {
        Location {
            absolute_start: start,
            absolute_end: end,
            start: self.line_map.offset_to_position(start),
            end: self.line_map.offset_to_position(end),
        }
    }

    /// Create a Location from Position structs (which contain both offset and position)
    pub(crate) fn create_location_from_positions(start: &Position, end: &Position) -> Location {
        Location {
            absolute_start: start.offset,
            absolute_end: end.offset,
            start: start.position.clone(),
            end: end.position.clone(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Position {
    offset: usize,
    position: crate::Position,
}

#[derive(Debug)]
// Used purely in the grammar to break down the block metadata lines into its different
// types.
enum BlockMetadataLine {
    Anchor(Anchor),
    Attributes((bool, BlockMetadata)),
    Title(String, Location),
}

#[derive(Debug)]
// Used purely inside the grammar to represent the style of a block
enum BlockStyle {
    Id(String, Option<(usize, usize)>),
    Role(String),
    Option(String),
}

#[derive(Debug)]
// Parsed revision information
struct RevisionInfo {
    number: String,
    date: Option<String>,
    remark: Option<String>,
}

/// Get the title nodes from the title data, if available
fn get_title_nodes(title_data: Option<&(String, Location)>) -> Vec<InlineNode> {
    if let Some((title, location)) = title_data {
        vec![InlineNode::PlainText(Plain {
            content: title.to_string(),
            location: location.clone(),
        })]
    } else {
        vec![]
    }
}

/// Generate initials from first, optional middle, and last name parts
fn generate_initials(first: &str, middle: Option<&str>, last: &str) -> String {
    let first_initial = first.chars().next().unwrap_or_default().to_string();
    let middle_initial = middle
        .map(|m| m.chars().next().unwrap_or_default().to_string())
        .unwrap_or_default();
    let last_initial = last.chars().next().unwrap_or_default().to_string();
    first_initial + &middle_initial + &last_initial
}

/// Process revision info and insert into document attributes
fn process_revision_info(
    revision_info: RevisionInfo,
    document_attributes: &mut DocumentAttributes,
) {
    if document_attributes.contains_key("revnumber") {
        tracing::warn!(
            "Revision number found in revision line but ignoring due to being set through attribute entries."
        );
    } else {
        document_attributes.insert(
            "revnumber".to_string(),
            AttributeValue::String(revision_info.number),
        );
    }

    if let Some(date) = revision_info.date {
        if document_attributes.contains_key("revdate") {
            tracing::warn!(
                "Revision date found in revision line but ignoring due to being set through attribute entries."
            );
        } else {
            document_attributes.insert("revdate".to_string(), AttributeValue::String(date));
        }
    }

    if let Some(remark) = revision_info.remark {
        if document_attributes.contains_key("revremark") {
            tracing::warn!(
                "Revision remark found in revision line but ignoring due to being set through attribute entries."
            );
        } else {
            document_attributes.insert("revremark".to_string(), AttributeValue::String(remark));
        }
    }
}

const RESERVED_NAMED_ATTRIBUTE_ID: &str = "id";
const RESERVED_NAMED_ATTRIBUTE_ROLE: &str = "role";
const RESERVED_NAMED_ATTRIBUTE_OPTIONS: &str = "opts";

peg::parser! {
    pub(crate) grammar document_parser(state: &mut ParserState) for str {
        use std::str::FromStr;

        pub(crate) rule document() -> Result<Document, Error>
            = start:position() newline_or_comment()* header:header() newline_or_comment()* blocks:blocks() end:position() (eol()* / ![_]) {
                // For documents that end with text content (like body-only), adjust the end position
                let document_end_offset = if blocks.is_empty() {
                    end.offset
                } else {
                    // If the last block is a paragraph with only text content, use its end position
                    match blocks.last().unwrap() {
                        Block::Paragraph(_) | Block::Admonition(_) => end.offset.saturating_sub(1),
                        _ => end.offset.saturating_sub(1),
                    }
                };

                Ok(Document {
                    name: "document".to_string(),
                    r#type: "block".to_string(),
                    header,
                    location: Location {
                        absolute_start: start.offset,
                        absolute_end: document_end_offset,
                        // The start position is the start of the document, but if the end offset is 0, we set it to 0
                        start: if end.offset == 0 { crate::Position{
                            column: 0,
                            .. start.position
                        }} else {start.position},
                        end: if end.offset == 0 { crate::Position{
                            column: 0,
                            .. end.position
                        }} else {
                            state.line_map.offset_to_position(document_end_offset)
                        },
                    },
                    attributes: state.document_attributes.clone(),
                    blocks,
                })
            }

        pub(crate) rule header() -> Option<Header>
            = start:position!()
            comment()*
            (document_attribute() (eol() / ![_] / comment()))*
            comment()*
            title_authors:(title_authors:title_authors() eol() { title_authors })?
            comment()*
            (document_attribute() (eol() / ![_] / comment()))*
            comment()*
            end:position!()
            (eol()*<,2> / ![_])
        {
            if let Some((title, authors)) = title_authors {
                let location = state.create_location(start, end);
                Some(Header {
                    title,
                    subtitle: None,
                    authors,
                    location
                })
            } else {
                tracing::info!("No title or authors found in the document header.");
                None
            }
        }

        pub(crate) rule title_authors() -> (Vec<InlineNode>, Vec<Author>)
            = start:position() title:document_title() eol() authors:authors_and_revision() &(eol()+ / ![_])
        {
            tracing::info!(?title, ?authors, "Found title and authors in the document header.");
            (title, authors)
        }
        / title:document_title() eol() {
            tracing::info!(?title, "Found title in the document header without authors.");
            (title, vec![])
        }

        pub(crate) rule document_title() -> Vec<InlineNode>
            = document_title_token() whitespace() start:position!() title:$([^'\n']*) end:position!()
        {
            let location = state.create_location(start, end.saturating_sub(1));
            vec![InlineNode::PlainText(Plain {
                content: title.to_string(),
                location,
            })]
        }

        rule document_title_token() = "=" / "#"

        rule authors_and_revision() -> Vec<Author>
            = authors:authors() (eol() revision())? {
                authors
            }

        pub(crate) rule authors() -> Vec<Author>
            = authors:(author() ** (";" whitespace()*)) {
                authors
            }

        /// Parse an author in various formats:
        /// - "First Middle Last <email>"
        /// - "First Last <email>"
        /// - "First <email>"
        /// - "First Last"
        pub(crate) rule author() -> Author
            = name:author_name() email:author_email()? {
                let mut author = name;
                if let Some(email_addr) = email {
                    author.email = Some(email_addr.to_string());
                }
                author
            }

        /// Parse author name in format: "First [Middle] Last" or just "First"
        rule author_name() -> Author
            = first:name_part() whitespace()+ middle:name_part() whitespace()+ last:$(name_part() ** whitespace()) {
                Author {
                    first_name: first.to_string(),
                    middle_name: Some(middle.to_string()),
                    last_name: last.to_string(),
                    initials: generate_initials(first, Some(middle), last),
                    email: None,
                }
            }
            / first:name_part() whitespace()+ last:name_part() {
                Author {
                    first_name: first.to_string(),
                    middle_name: None,
                    last_name: last.to_string(),
                    initials: generate_initials(first, None, last),
                    email: None,
                }
            }
            / first:name_part() {
                Author {
                    first_name: first.to_string(),
                    middle_name: None,
                    last_name: String::new(),
                    initials: generate_initials(first, None, ""),
                    email: None,
                }
            }

        /// Parse email address in format: " <email@domain>"
        rule author_email() -> &'input str
            = whitespace()* "<" email:$([^'>']*) ">" { email }

        rule name_part() -> &'input str
            = name:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-']+ ("_" ['a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-']+)*) {
                name
            }

        pub(crate) rule revision() -> ()
            = number:$("v"? digits() ** ".") date:revision_date()? remark:revision_remark()? {
                let revision_info = RevisionInfo {
                    number: number.to_string(),
                    date: date.map(ToString::to_string),
                    remark: remark.map(ToString::to_string),
                };
                process_revision_info(revision_info, &mut state.document_attributes);
            }

        rule revision_date() -> &'input str
            = ", " date:$([^ (':'|'\n')]+) {
                date
            }

        rule revision_remark() -> &'input str
            = ": " remark:$([^'\n']+) {
                remark
            }

        rule document_attribute() -> ()
            = att:document_attribute_match()
        {
            tracing::info!(?att, "Found document attribute in the document header");
            let (key, value) = att;
            state.document_attributes.insert(key.to_string(), value);
        }

        pub(crate) rule blocks() -> Vec<Block>
            = blocks:(block() ** (eol()*<2,2>)) {
                blocks
            }

        pub(crate) rule block() -> Block
            = block:(document_attribute_block() / section() / block_generic())
        {
            block
        }

        pub(crate) rule document_attribute_block() -> Block
            = start:position!() att:document_attribute_match() end:position!() {
                let (key, value) = att;
                Block::DocumentAttribute(DocumentAttribute {
                    name: key.to_string(),
                    value,
                    location: state.create_location(start, end)
                })
            }

        pub(crate) rule section() -> Block
            = start:position!() block_metadata:block_metadata()
            section_level:section_level() whitespace()
            title_start:position!() title:section_title() title_end:position!() eol()*<2,2>
            content:section_content()* end:position!() {
                // TODO(nlopes): what do I do with metadata_title?!?
                let (discrete, metadata, metadata_title) = block_metadata;
                tracing::info!(?metadata, ?metadata_title, ?title, ?title_start, ?title_end, "parsing section block");

                let level = section_level.len().try_into().unwrap_or(1) - 1;
                let location = state.create_location(start, end.saturating_sub(1));

                // Create a simple title with plain text
                let title_node = InlineNode::PlainText(Plain {
                    content: title,
                    location: state.create_location(title_start, title_end.saturating_sub(1)),
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
                    content: content[0].clone(),
                    location
                })
            }

        rule block_metadata() -> (bool, BlockMetadata, Option<(String, Location)>)
            = lines:(
                anchor:anchor() { BlockMetadataLine::Anchor(anchor) }
                / attr:attributes_line() { BlockMetadataLine::Attributes(attr) }
                / data:title_line() { BlockMetadataLine::Title(data.0, data.1) }
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
                        BlockMetadataLine::Title(value, location) => {
                            title = Some((value, location));
                        }
                        _ => unreachable!(),
                    }
                }

                (discrete, metadata, title)
            }

        // A title line can be a simple title or a section title
        //
        // A title line is a line that starts with a period (.) followed by a non-whitespace character
        rule title_line() -> (String, Location)
            = period() start:position!() &(!whitespace()) title:$([^'\n']*) end:position!() eol() {
                let location = state.create_location(start, end.saturating_sub(1));
                tracing::info!(?title, ?start, ?end, ?location, "Found title line in block metadata");
                (title.to_string(), location)
            }

        rule section_level() -> &'input str
            = level:$(("=" / "#")*<1,6>) {
                level
            }

        rule section_title() -> String
            = title:$([^'\n']*) {
                title.to_string()
            }

        rule section_content() -> Vec<Block>
            = b:block() &(eol()+ / ![_]) { vec![b] }

        pub(crate) rule block_generic() -> Block
            = start:position!() block_metadata:block_metadata() block_type:(
                delimited_block:delimited_block(start, &block_metadata) { delimited_block }
                / image:image(start, &block_metadata) { image }
                / audio:audio(start, &block_metadata) { audio }
                / video:video(start, &block_metadata) { video }
                / toc:toc(start, &block_metadata) { toc }
                / thematic_break:thematic_break(start, &block_metadata) { thematic_break }
                / page_break:page_break(start, &block_metadata) { page_break }
                / list:list(start, &block_metadata) { list }
                / paragraph:paragraph(start, &block_metadata) { paragraph }
            ) {
                tracing::info!(?block_metadata, ?block_type, "parsing generic block");
                block_type
            }

        rule delimited_block(
            start: usize,
            block_details: &(bool, BlockMetadata, Option<(String, Location)>),
        ) -> Block
            = comment_block(start, block_details)
            / example_block(start, block_details)
            / listing_block(start, block_details)
            / literal_block(start, block_details)
            / open_block(start, block_details)
            / sidebar_block(start, block_details)
            / table_block(start, block_details)
            / pass_block(start, block_details)
            / quote_block(start, block_details)

        // Delimiter recognition rules
        rule comment_delimiter() -> &'input str = delim:$("/"*<4,>) { delim }
        rule example_delimiter() -> &'input str = delim:$("="*<4,>) { delim }
        rule listing_delimiter() -> &'input str = delim:$("-"*<4,>) { delim }
        rule literal_delimiter() -> &'input str = delim:$("."*<4,>) { delim }
        rule open_delimiter() -> &'input str = delim:$("-"*<2,> / "~"*<4,>) { delim }
        rule sidebar_delimiter() -> &'input str = delim:$("*"*<4,>) { delim }
        rule table_delimiter() -> &'input str = delim:$((['|' | ',' | ':' | '!'] "="*<3,>)) { delim }
        rule pass_delimiter() -> &'input str = delim:$("+"*<4,>) { delim }
        rule quote_delimiter() -> &'input str = delim:$("_"*<4,>) { delim }

        rule until_comment_delimiter() -> &'input str
            = content:$((!("\n" comment_delimiter()) [_])*)
        {
            content
        }

        rule until_listing_delimiter() -> &'input str
            = content:$((!("\n" listing_delimiter()) [_])*)
        {
            content
        }

        rule until_literal_delimiter() -> &'input str
            = content:$((!("\n" literal_delimiter()) [_])*)
        {
            content
        }

        rule until_open_delimiter() -> &'input str
            = content:$((!("\n" open_delimiter()) [_])*)
        {
            content
        }

        rule until_sidebar_delimiter() -> &'input str
            = content:$((!("\n" sidebar_delimiter()) [_])*)
        {
            content
        }

        rule until_table_delimiter() -> &'input str
            = content:$((!("\n" table_delimiter()) [_])*)
        {
            content
        }

        rule until_pass_delimiter() -> &'input str
            = content:$((!("\n" pass_delimiter()) [_])*)
        {
            content
        }

        rule until_quote_delimiter() -> &'input str
            = content:$((!("\n" quote_delimiter()) [_])*)
        {
            content
        }

        // Individual delimited block rules
        rule example_block(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = open_delim:example_delimiter() eol()
              content_start:position!() content:blocks() content_end:position!()
              eol()? close_delim:example_delimiter() end:position!()
        {?
            tracing::info!(?block_details, ?content, "Parsing example block");

            // Ensure the opening and closing delimiters match
            if open_delim != close_delim {
                return Err("mismatched example delimiters");
            }
            let (_discrete, metadata, title_data) = block_details;
            let location = state.create_location(start, end.saturating_sub(1));
            let title = get_title_nodes(title_data.as_ref());
            // Parse content as blocks with proper positioning
            // let blocks = if content.trim().is_empty() {
            //     Vec::new()
            // } else {
            //     let content_location = state.create_location(content_start, content_end.saturating_sub(1));
            //     vec![Block::Paragraph(Paragraph {
            //         content: vec![InlineNode::PlainText(Plain {
            //             content: content.to_string(),
            //             location: content_location.clone(),
            //         })],
            //         metadata: BlockMetadata::default(),
            //         title: Vec::new(),
            //         location: content_location,
            //     })]
            // };

            // We want to detect if this is an admonition block. We do that by checking if
            // we have a style that matches an admonition variant.
            if let Some(ref style) = metadata.style {
                if let Ok(admonition_variant) = AdmonitionVariant::from_str(style) {
                    tracing::debug!("Detected admonition block with variant: {:?}", admonition_variant);
                    let mut metadata = metadata.clone();
                    metadata.style = None; // Clear style to avoid confusion
                    return Ok(Block::Admonition(Admonition {
                        variant: admonition_variant,
                        blocks: content,
                        metadata,
                        title,
                        location,
                    }));
                }
            }
            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedExample(content),
                title,
                location,
            }))
        }

        rule comment_block(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = open_delim:comment_delimiter() eol()
            content_start:position!() content:until_comment_delimiter() content_end:position!()
            eol() close_delim:comment_delimiter() end:position!()
        {?
            if open_delim != close_delim {
                return Err("mismatched comment delimiters");
            }
            let (_discrete, metadata, title_data) = block_details;
            let location = state.create_location(start, end.saturating_sub(1));
            let content_location = state.create_location(content_start, content_end.saturating_sub(1));
            let title = get_title_nodes(title_data.as_ref());

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedComment(vec![InlineNode::PlainText(Plain {
                    content: content.to_string(),
                    location: content_location,
                })]),
                title,
                location,
            }))
        }

        rule listing_block(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = open_delim:listing_delimiter() eol()
            content_start:position!() content:until_listing_delimiter() content_end:position!()
            eol() close_delim:listing_delimiter() end:position!()
        {?
            if open_delim != close_delim {
                return Err("mismatched listing delimiters");
            }
            let (_discrete, metadata, title_data) = block_details;
            let location = state.create_location(start, end.saturating_sub(1));
            let content_location = state.create_location(content_start, content_end.saturating_sub(1));
            let title = get_title_nodes(title_data.as_ref());

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedListing(vec![InlineNode::PlainText(Plain {
                    content: content.to_string(),
                    location: content_location,
                })]),
                title,
                location,
            }))
        }

        rule literal_block(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = open_delim:literal_delimiter() eol()
            content_start:position!() content:until_literal_delimiter() content_end:position!()
            eol() close_delim:literal_delimiter() end:position!()
        {?
            if open_delim != close_delim {
                return Err("mismatched literal delimiters");
            }
            let (_discrete, metadata, title_data) = block_details;
            let location = state.create_location(start, end.saturating_sub(1));
            let content_location = state.create_location(content_start, content_end.saturating_sub(1));
            let title = get_title_nodes(title_data.as_ref());

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedLiteral(vec![InlineNode::PlainText(Plain {
                    content: content.to_string(),
                    location: content_location,
                })]),
                title,
                location,
            }))
        }

        rule open_block(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = open_delim:open_delimiter() eol()
            content_start:position!() content:until_open_delimiter() content_end:position!()
            eol() close_delim:open_delimiter() end:position!()
        {?
            if open_delim != close_delim {
                return Err("mismatched open delimiters");
            }
            let (_discrete, metadata, title_data) = block_details;
            let location = state.create_location(start, end.saturating_sub(1));
            let title = get_title_nodes(title_data.as_ref());

            let blocks = if content.trim().is_empty() {
                Vec::new()
            } else {
                let content_location = state.create_location(content_start, content_end.saturating_sub(1));
                vec![Block::Paragraph(Paragraph {
                    content: vec![InlineNode::PlainText(Plain {
                        content: content.to_string(),
                        location: content_location.clone(),
                    })],
                    metadata: BlockMetadata::default(),
                    title: Vec::new(), // TODO(nlopes): Handle paragraph titles
                    location: content_location.clone(),
                })]
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedOpen(blocks),
                title,
                location,
            }))
        }

        rule sidebar_block(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = open_delim:sidebar_delimiter() eol()
            content_start:position!() content:until_sidebar_delimiter() content_end:position!()
            eol() close_delim:sidebar_delimiter() end:position!()
        {?
            if open_delim != close_delim {
                return Err("mismatched sidebar delimiters");
            }
            let (_discrete, metadata, title_data) = block_details;
            let location = state.create_location(start, end.saturating_sub(1));
            let title = get_title_nodes(title_data.as_ref());

            let blocks = if content.trim().is_empty() {
                Vec::new()
            } else {
                let content_location = state.create_location(content_start, content_end.saturating_sub(1));
                vec![Block::Paragraph(Paragraph {
                    content: vec![InlineNode::PlainText(Plain {
                        content: content.to_string(),
                        location: content_location.clone(),
                    })],
                    metadata: BlockMetadata::default(),
                    title: Vec::new(), // TODO(nlopes): Handle paragraph titles
                    location: content_location.clone(),
                })]
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedSidebar(blocks),
                title,
                location,
            }))
        }

        rule table_block(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = open_delim:table_delimiter() eol()
            content_start:position!() content:until_table_delimiter() content_end:position!()
            eol() close_delim:table_delimiter() end:position!()
        {?
            if open_delim != close_delim {
                return Err("mismatched table delimiters");
            }
            let (_discrete, metadata, title_data) = block_details;
            let location = state.create_location(start, end.saturating_sub(1));
            let content_location = state.create_location(content_start, content_end.saturating_sub(1));
            let title = get_title_nodes(title_data.as_ref());

            let table = Table {
                header: None,
                footer: None,
                rows: Vec::new(),
                location: content_location.clone(),
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedTable(table),
                title,
                location,
            }))
        }

        rule pass_block(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = open_delim:pass_delimiter() eol()
            content_start:position!() content:until_pass_delimiter() content_end:position!()
            eol() close_delim:pass_delimiter() end:position!()
        {?
            if open_delim != close_delim {
                return Err("mismatched pass delimiters");
            }
            let (_discrete, metadata, title_data) = block_details;
            let location = state.create_location(start, end.saturating_sub(1));
            let content_location = state.create_location(content_start, content_end.saturating_sub(1));
            let title = get_title_nodes(title_data.as_ref());

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedPass(vec![InlineNode::RawText(Raw {
                    content: content.to_string(),
                    location: content_location,
                })]),
                title,
                location,
            }))
        }

        rule quote_block(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = open_delim:quote_delimiter() eol()
            content_start:position!() content:until_quote_delimiter() content_end:position!()
            eol() close_delim:quote_delimiter() end:position!()
        {?
            if open_delim != close_delim {
                return Err("mismatched quote delimiters");
            }
            let (_discrete, metadata, title_data) = block_details;
            let location = state.create_location(start, end.saturating_sub(1));
            let content_location = state.create_location(content_start, content_end.saturating_sub(1));
            let title = get_title_nodes(title_data.as_ref());

            let inner = if let Some(ref style) = metadata.style {
                if style == "verse" {
                    DelimitedBlockType::DelimitedVerse(vec![InlineNode::PlainText(Plain {
                        content: content.to_string(),
                        location: content_location.clone(),
                    })])
                } else {
                    let blocks = if content.trim().is_empty() {
                        Vec::new()
                    } else {
                        vec![Block::Paragraph(Paragraph {
                            content: vec![InlineNode::PlainText(Plain {
                                content: content.to_string(),
                                location: content_location.clone(),
                            })],
                            metadata: BlockMetadata::default(),
                            title: Vec::new(), // TODO(nlopes): Handle paragraph titles
                            location: content_location.clone(),
                        })]
                    };
                    DelimitedBlockType::DelimitedQuote(blocks)
                }
            } else {
                let blocks = if content.trim().is_empty() {
                    Vec::new()
                } else {
                    vec![Block::Paragraph(Paragraph {
                        content: vec![InlineNode::PlainText(Plain {
                            content: content.to_string(),
                            location: content_location.clone(),
                        })],
                        metadata: BlockMetadata::default(),
                        title: Vec::new(), // TODO(nlopes): Handle paragraph titles
                        location: content_location.clone(),
                    })]
                };
                DelimitedBlockType::DelimitedQuote(blocks)
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner,
                title,
                location,
            }))
        }

        rule toc(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = "toc::[]" end:position!()
        {
            let (_discrete, metadata, _title) = block_details;
            tracing::info!("Found Table of Contents block");
            Block::TableOfContents(TableOfContents {
                metadata: metadata.clone(),
                location: state.create_location(start, end),
            })
        }

        rule image(start: usize, _block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = "image::" source:source() attributes:attributes() end:position!()
        {
            let (_discrete, metadata) = attributes;
            Block::Image(Image {
                title: Vec::new(), // TODO(nlopes): Handle image titles
                source,
                metadata: metadata.clone(),
                location: state.create_location(start, end),
            })
        }

        rule audio(start: usize, _block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = "audio::" source:source() attributes:attributes() end:position!()
        {
            let (_discrete, metadata) = attributes;
            Block::Audio(Audio {
                title: Vec::new(), // TODO(nlopes): Handle audio titles
                source,
                metadata: metadata.clone(),
                location: state.create_location(start, end),
            })
        }

        // The video block is similar to the audio and image blocks, but it supports
        // multiple sources. This is for example to allow passing multiple youtube video
        // ids to form a playlist.
        rule video(start: usize, _block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = "video::" sources:(source() ** comma()) attributes:attributes() end:position!()
        {
            let (_discrete, metadata) = attributes;
            Block::Video(Video {
                title: Vec::new(), // TODO(nlopes): Handle video titles
                sources,
                metadata: metadata.clone(),
                location: state.create_location(start, end),
            })
        }

        rule thematic_break(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = ("'''"
               // Below are the markdown-style thematic breaks
               / "---"
               / "- - -"
               / "***"
               / "* * *"
            ) end:position!()
        {
            tracing::info!("Found thematic break block");
            let (_discrete, metadata, _title) = block_details;
            Block::ThematicBreak(ThematicBreak {
                anchors: metadata.anchors.clone(), // TODO(nlopes): should this simply be metadata?
                title: Vec::new(), // TODO(nlopes): Handle thematic break titles
                location: state.create_location(start, end),
            })
        }

        rule page_break(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = "<<<" end:position!()
        {
            tracing::info!("Found page break block");
            let (_discrete, metadata, title_data) = block_details;
            let title = get_title_nodes(title_data.as_ref());

            Block::PageBreak(PageBreak {
                title,
                metadata: metadata.clone(),
                location: state.create_location(start, end),
            })
        }

        rule list(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = unordered_list(start, block_details) / ordered_list(start, block_details)

        rule unordered_list_marker() -> &'input str = $("*"+ / "-")

        rule ordered_list_marker() -> &'input str = $(digits()? "."+)

        rule unordered_list(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = &(unordered_list_marker() whitespace()) content:list_item()+ end:position!()
        {?
            tracing::info!(?content, "Found unordered list block");
            let (_discrete, metadata, _title_data) = block_details;
            let end = content.last().map_or(end, |(_, item_end)| item_end.clone());
            let items: Vec<ListItem> = content.into_iter().map(|(item, end)| item).collect();
            let marker = items.first().map_or(String::new(), |item| item.marker.clone());

            Ok(Block::UnorderedList(UnorderedList {
                title: Vec::new(), // TODO(nlopes): Handle list item titles
                metadata: metadata.clone(),
                items,
                marker,
                location: state.create_location(start, end),
            }))
        }

        rule ordered_list(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = &(ordered_list_marker() whitespace()) content:list_item()+ end:position!()
        {?
            tracing::info!(?content, "Found ordered list block");
            let (_discrete, metadata, _title_data) = block_details;
            let end = content.last().map_or(end, |(_, item_end)| item_end.clone());
            let items: Vec<ListItem> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or(String::new(), |item| item.marker.clone());

            Ok(Block::OrderedList(OrderedList {
                title: Vec::new(), // TODO(nlopes): Handle list item titles
                metadata: metadata.clone(),
                items,
                marker,
                location: state.create_location(start, end),
            }))
        }

        rule list_item() -> (ListItem, usize)
            = start:position!()
              marker:(unordered_list_marker() / ordered_list_marker())
              whitespace()
              checked:checklist_item()?
              list_content_start:position!()
              list_item:$((!(eol() / ![_]) [_])+)
              end:position!() (eol()*<1,2> / ![_])
        {?
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1)).map_err(|_| "could not parse depth from marker")?;
            let end = end.saturating_sub(1);

            tracing::info!(%list_item, %marker, ?checked, %level, "found list item");

            Ok((ListItem {
                content: vec![InlineNode::PlainText(Plain {
                    content: list_item.to_string(), // TODO(nlopes): Handle item content
                    location: state.create_location(list_content_start, end),
                })],
                level,
                marker: marker.to_string(),
                checked,
                location: state.create_location(start, end),
            }, end))
        }

        rule checklist_item() -> ListItemCheckedStatus
            = checked:(("[x]" / "[X]" / "[*]") { ListItemCheckedStatus::Checked } / "[ ]" { ListItemCheckedStatus::Unchecked }) whitespace()
        {
            checked
        }

        pub(crate) rule inlines(start: usize) -> Vec<InlineNode>
            = inlines:(non_plain_text(start) / plain_text(start))+ {
                tracing::info!(?inlines, "Found inlines");
                inlines
            }

        rule non_plain_text(start: usize) -> InlineNode
            = inline:(
                hard_wrap:hard_wrap() { hard_wrap }
                / italic_text_unconstrained:italic_text_unconstrained(start) { italic_text_unconstrained }
            ) {
                inline
            }

        rule hard_wrap() -> InlineNode
            = start:position!() " + \\" eol() end:position!()
        {
            tracing::info!("Found hard wrap inline");
            InlineNode::LineBreak(LineBreak {
                location: state.create_location(start, end.saturating_sub(1)),
            })
        }

        rule italic_text_unconstrained(start: usize) -> InlineNode
            = "__" italic_text:$((!(eol() / ![_] / "__") [_])+) "__" end:position!()
        {
            tracing::info!(?italic_text, "Found unconstrained italic text inline");
            InlineNode::ItalicText(Italic {
                content: vec![InlineNode::PlainText(Plain {
                    content: italic_text.to_string(),
                    location: state.create_location(start, end.saturating_sub(1)),
                })],
                role: None, // TODO(nlopes): Handle roles (come from attributes list)
                location: state.create_location(start, end.saturating_sub(1)),
            })
        }

        rule plain_text(start: usize) -> InlineNode
            = attributes:attributes()? content:$((!(eol() / ![_] / italic_text_unconstrained(start)) [_])+) end:position!()
        {
            tracing::info!(?content, "Found plain text inline");
            InlineNode::PlainText(Plain {
                content: content.to_string(),
                location: state.create_location(start, end.saturating_sub(1)),
            })
        }

        rule paragraph(start: usize, block_details: &(bool, BlockMetadata, Option<(String, Location)>)) -> Block
            = admonition:admonition()? content_start:position!() content:$((!(eol()+ / ![_] / example_delimiter() / list(start, block_details)) [_])+) end:position!()
        {?
            let (_discrete, metadata, _title_data) = block_details;

            // parse the inline content - this needs to be handed over to the inline preprocessing
            let mut inline_state = InlinePreprocessorParserState::new();
            let location = state.create_location(start, end.saturating_sub(1));
            inline_state.set_initial_position(&location, start);

            let processed = inline_preprocessing::run(content, &state.document_attributes, &inline_state)
                .map_err(|e| {
                    tracing::error!(?e, "failed to preprocess inline content in paragraph block");
                    "failed to preprocess inline content in paragraph block"
                })?;
            tracing::info!(?processed, "processed inline content in paragraph block");
            // XXX: currently working here - need to implement inline parsing
            let mut state = ParserState::new(&processed.text);
            let content = document_parser::inlines(&processed.text, &mut state, start).map_err(|e| {
                tracing::error!(?e, "failed to parse inlines in paragraph block");
                "failed to parse inlines in paragraph block"
            })?;
            tracing::info!(?content, "parsed inlines in paragraph block");

            if let Some(variant) = admonition {
                let Ok(variant) = AdmonitionVariant::from_str(&variant) else {
                    tracing::error!(%variant, "invalid admonition variant");
                    return Err("invalid admonition variant");
                };
                tracing::info!(%variant, "found admonition block with variant");
                Ok(Block::Admonition(Admonition{
                    metadata: metadata.clone(),
                    title: Vec::new(), // TODO(nlopes): Handle admonition titles
                    blocks: vec![Block::Paragraph(Paragraph {
                        content,
                        metadata: metadata.clone(),
                        title: Vec::new(), // TODO(nlopes): Handle paragraph titles
                        location: state.create_location(start, end.saturating_sub(1)),
                    })],
                    location: state.create_location(start, end.saturating_sub(1)),
                    variant,

                }))
            } else {
                tracing::info!(?content, "found paragraph block");
                Ok(Block::Paragraph(Paragraph {
                    content,
                    metadata: metadata.clone(),
                    title: Vec::new(), // TODO(nlopes): Handle paragraph titles
                    location: state.create_location(start, end.saturating_sub(1)),
                }))
            }
        }

        rule admonition() -> String
            = variant:$("NOTE" / "WARNING" / "TIP" / "IMPORTANT" / "CAUTION") ": "
        {
            variant.to_string()
        }

        rule anchor() -> Anchor
            = result:(
                start:position!() double_open_square_bracket() id:$([^']' | ',' | ']']+) comma() reftext:$([^']']+) double_close_square_bracket() eol() end:position!() {
                (start, id, Some(reftext), end)
            } /
            start:position!() double_open_square_bracket() id:$([^']' | ',' | ']']+) double_close_square_bracket() eol() end:position!() {
                (start, id, None, end)
            } /
            start:position!() open_square_bracket() "#" id:$([^']' | ',' | ']']+) comma() reftext:$([^']']+) close_square_bracket() eol() end:position!() {
                (start, id, Some(reftext), end)
            } /
            start:position!() open_square_bracket() "#" id:$([^']' | ',' | ']']+) close_square_bracket() eol() end:position!() {
                (start, id, None, end)
            }) {
                let (start, id, reftext, end) = result;
                Anchor {
                    id: id.to_string(),
                    xreflabel: reftext.map(ToString::to_string),
                    location: state.create_location(start, end)
                }
            }

        pub(crate) rule attributes_line() -> (bool, BlockMetadata)
            = attributes:attributes() eol() {
                let (discrete, metadata) = attributes;
                (discrete, metadata)
            }

        pub(crate) rule attributes() -> (bool, BlockMetadata)
            = start:position!() open_square_bracket() content:(
                // The case in which we keep the style empty
                comma() attributes:(attribute() ** comma()) {
                    tracing::info!("Found empty style with attributes");
                    (true, false, None, attributes)
                } /
                // The case in which there is a block style and other attributes
                style:block_style() comma() attributes:(attribute() ++ comma()) {
                    tracing::info!("Found block style with attributes: {:?}", style);
                    (false, true, Some(style), attributes)
                } /
                // The case in which there is a block style and no other attributes
                style:block_style() {
                    tracing::info!("Found block style: {:?}", style);
                    (false, true, Some(style), vec![])
                } /
                // The case in which there are only attributes
                attributes:(attribute() ** comma()) {
                    tracing::info!("Found attributes: {:?}", attributes);
                    (false, false, None, attributes)
                })
            close_square_bracket() end:position!() {
                let mut discrete = false;
                let mut style_found = false;
                let (empty, has_style, maybe_style, attributes) = content;
                let mut metadata = BlockMetadata::default();
                if let Some((maybe_attribute, id, roles, options)) = maybe_style {
                    if let Some(attribute_name) = maybe_attribute {
                        if attribute_name == "discrete" {
                            discrete = true;
                        } else if metadata.style.is_none() && has_style {
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
                for (k, v, pos) in attributes.into_iter().flatten() {
                    if k == RESERVED_NAMED_ATTRIBUTE_ID && metadata.id.is_none() {
                        let (id_start, id_end) = pos.unwrap_or((start, end));
                        metadata.id = Some(Anchor {
                            id: v.to_string(),
                            xreflabel: None,
                            location: state.create_location(id_start, id_end)
                        });
                    } else if let AttributeValue::String(v) = v {
                        let values = if v.starts_with('"') && v.ends_with('"') {
                            // Remove the quotes from the value, split by commas, and trim whitespace
                            v[1..v.len()-1].split(',').map(|s| s.trim().to_string()).collect()
                        } else {
                            vec![v]
                        };
                        if k == RESERVED_NAMED_ATTRIBUTE_ROLE {
                            for v in values {
                                metadata.roles.push(v);
                            }
                        } else if k == RESERVED_NAMED_ATTRIBUTE_OPTIONS {
                            for v in values {
                                metadata.options.push(v);
                            }
                        } else {
                            for v in values {
                                metadata.attributes.insert(k.to_string(), AttributeValue::String(v));
                            }
                        }
                    } else {
                        tracing::warn!("Unexpected attribute value type: {:?}", v);
                    }
                }
                (discrete, metadata)
            }

        // TODO(nlopes): This should return Vec<InlineNode>
        // Once I implement inlines_inner, I can come back here and fix.
        rule block_title() -> String
            = "." !['.' | ' '] title:$([^'\n']*) eol() {
                title.to_string()
            }

        rule open_square_bracket() = "["
        rule close_square_bracket() = "]"
        rule double_open_square_bracket() = "[["
        rule double_close_square_bracket() = "]]"
        rule comma() = ","
        rule period() = "."
        rule empty_style() = ""
        rule role() -> &'input str = $([^ ',' | ']' | '#' | '.' | '%']+)

        // The option rule is used to parse options in the form of "option=value" or
        // "%option" (we don't capture the % here).
        //
        // The option can be a single word or a quoted string. If it is a quoted string,
        // it can contain commas, which we then look for and extract the options in the
        // `attributs()` rule.
        rule option() -> &'input str =
            $(("\"" [^'"' | ']' | '#' | '.' | '%']+ "\"") / ([^'"' | ',' | ']' | '#' | '.' | '%']+))

        rule attribute_name() -> &'input str = $((['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_'])+)

        pub(crate) rule attribute() -> Option<(String, AttributeValue, Option<(usize, usize)>)>
            = att:named_attribute() { att }
              / att:positional_attribute_value() {
                  Some((att, AttributeValue::None, None))
              }

        // Add a simple ID rule
        rule id() -> String
            = id:$((['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_'])+) { id.to_string() }

        // TODO(nlopes): this should instead return an enum
        rule named_attribute() -> Option<(String, AttributeValue, Option<(usize, usize)>)>
            = "id" "=" start:position!() id:id() end:position!()
                { Some((RESERVED_NAMED_ATTRIBUTE_ID.to_string(), AttributeValue::String(id), Some((start, end)))) }
              / ("role" / "roles") "=" role:role()
                { Some((RESERVED_NAMED_ATTRIBUTE_ROLE.to_string(), AttributeValue::String(role.to_string()), None)) }
              / ("options" / "opts") "=" option:option()
                { Some((RESERVED_NAMED_ATTRIBUTE_OPTIONS.to_string(), AttributeValue::String(option.to_string()), None)) }
              / name:attribute_name() "=" value:named_attribute_value()
                { Some((name.to_string(), AttributeValue::String(value), None)) }

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
        pub(crate) rule block_style() -> (Option<String>, Option<Anchor>, Vec<String>, Vec<String>)
            = start:position!() content:(
                style:positional_attribute_value() shorthands:(
                    "#" id_start:position!() id:block_style_id() id_end:position!() { BlockStyle::Id(id.to_string(), Some((id_start, id_end)))}
                    / "." role:role() { BlockStyle::Role(role.to_string())}
                    / "%" option:option() { BlockStyle::Option(option.to_string())}
                )+ {
                    (Some(style), shorthands)
                } /
                style:positional_attribute_value() {
                    (Some(style), Vec::new())
                } /
                shorthands:(
                    "#" id_start:position!() id:block_style_id() id_end:position!() { BlockStyle::Id(id.to_string(), Some((id_start, id_end)))}
                    / "." role:role() { BlockStyle::Role(role.to_string())}
                    / "%" option:option() { BlockStyle::Option(option.to_string())}
                )+ {
                    (None, shorthands)
               }
            )
            end:position!() {
                let (style, shorthands) = content;
                let mut maybe_anchor = None;
                let mut roles = Vec::new();
                let mut options = Vec::new();
                for shorthand in shorthands {
                    match shorthand {
                        BlockStyle::Id(id, pos) => {
                            let (id_start, id_end) = pos.unwrap_or((start, end));
                            maybe_anchor = Some(Anchor {
                                id,
                                xreflabel: None,
                                location: state.create_location(id_start, id_end)
                            });
                        },
                        BlockStyle::Role(role) => roles.push(role),
                        BlockStyle::Option(option) => options.push(option),
                    }
                }
                (style, maybe_anchor, roles, options)
            }

        rule id_start_char() = ['A'..='Z' | 'a'..='z' | '_']

        rule block_style_id() -> &'input str = $(id_start_char() block_style_id_subsequent_char()*)

        rule block_style_id_subsequent_char() =
            ['A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-']

        rule named_attribute_value() -> String
            = "\"" inner:inner_attribute_value() "\"" { inner }
            / s:$((!(","/ "]") [_])+) { s.to_string() }

        rule positional_attribute_value() -> String
            = s:$((!("\"" / "," / "]" / "#" / "." / "%") [_])
                 (!("\"" / "," / "]" / "#" / "%" / "=" / ".") [_])* !"=")
        {
            tracing::debug!("Found positional attribute value: {}", s);
            s.to_string()
        }

        rule inner_attribute_value() -> String
            = s:$(("\\\"" / (!"\"" [_]))*) { s.to_string() }

        pub rule url() -> String = proto:proto() "://" path:path() { format!("{}{}{}", proto, "://", path) }

        rule proto() -> &'input str = $("https" / "http" / "ftp" / "irc" / "mailto")

        pub rule path() -> &'input str = $(['A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-' | '.' | '/' | '~' ]+)

        pub rule source() -> Source
            = source:
        (
            u:url() { Source::Url(u.to_string()) }
            / p:path() { Source::Path(p.to_string()) }
        )
        { source }

        rule digits() = ['0'..='9']+

        rule eol() = quiet!{ "\n" }

        rule newline_or_comment() = quiet!{ eol() / comment() }

        rule comment() = quiet!{ "//" [^'\n']+ }

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

        rule whitespace() = quiet!{ " " / "\t" }

        rule position() -> Position = offset:position!() {
            Position {
                offset,
                position: state.line_map.offset_to_position(offset)
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
        let mut state = ParserState::new(input);
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
                    absolute_end: 47,
                    start: crate::Position { line: 2, column: 3 },
                    end: crate::Position {
                        line: 2,
                        column: 16
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
        let mut state = ParserState::new(input);
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
        let mut state = ParserState::new(input);
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
        let mut state = ParserState::new(input);
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
        let mut state = ParserState::new(input);
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
        let mut state = ParserState::new(input);
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
        let mut state = ParserState::new(input);
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
        let mut state = ParserState::new(input);
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
        let mut state = ParserState::new(input);
        let result = document_parser::document_title(input, &mut state).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            InlineNode::PlainText(Plain {
                content: "Document Title".to_string(),
                location: Location {
                    absolute_start: 2,
                    absolute_end: 15,
                    start: crate::Position { line: 1, column: 3 },
                    end: crate::Position {
                        line: 1,
                        column: 16
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
        let mut state = ParserState::new(input);
        let result = document_parser::header(input, &mut state).unwrap().unwrap();
        assert_eq!(result.title.len(), 1);
        assert_eq!(
            result.title[0],
            InlineNode::PlainText(Plain {
                content: "Document Title".to_string(),
                location: Location {
                    absolute_start: 2,
                    absolute_end: 15,
                    start: crate::Position { line: 1, column: 3 },
                    end: crate::Position {
                        line: 1,
                        column: 16
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

    #[test]
    #[tracing_test::traced_test]
    fn test_document_empty_attribute_list() {
        let input = "[]";
        let mut state = ParserState::new(input);
        let (discrete, metadata) = document_parser::attributes(input, &mut state).unwrap();
        assert_eq!(discrete, false); // Not discrete
        assert_eq!(metadata.id, None);
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.is_empty());
        assert!(metadata.options.is_empty());
        assert!(metadata.attributes.is_empty());
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_empty_attribute_list_with_discrete() {
        let input = "[discrete]";
        let mut state = ParserState::new(input);
        let (discrete, metadata) = document_parser::attributes(input, &mut state).unwrap();
        assert!(discrete); // Should be discrete
        assert_eq!(metadata.id, None);
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.is_empty());
        assert!(metadata.options.is_empty());
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id() {
        let input = "[id=my-id,role=admin,options=read,options=write]";
        let mut state = ParserState::new(input);
        let (discrete, metadata) = document_parser::attributes(input, &mut state).unwrap();
        assert_eq!(discrete, false); // Not discrete
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "my-id".to_string(),
                xreflabel: None,
                location: Location {
                    absolute_start: 4,
                    absolute_end: 9,
                    start: crate::Position { line: 1, column: 5 },
                    end: crate::Position {
                        line: 1,
                        column: 10
                    }
                }
            })
        );
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.contains(&"admin".to_string()));
        assert!(metadata.options.contains(&"read".to_string()));
        assert!(metadata.options.contains(&"write".to_string()));
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id_mixed() {
        let input = "[astyle#myid.admin,options=read,options=write]";
        let mut state = ParserState::new(input);
        let (discrete, metadata) = document_parser::attributes(input, &mut state).unwrap();
        assert_eq!(discrete, false); // Not discrete
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "myid".to_string(),
                xreflabel: None,
                location: Location {
                    absolute_start: 8,
                    absolute_end: 12,
                    start: crate::Position { line: 1, column: 9 },
                    end: crate::Position {
                        line: 1,
                        column: 13
                    }
                }
            })
        );
        assert_eq!(metadata.style, Some("astyle".to_string()));
        assert!(metadata.roles.contains(&"admin".to_string()));
        assert!(metadata.options.contains(&"read".to_string()));
        assert!(metadata.options.contains(&"write".to_string()));
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id_mixed_with_quotes() {
        let input = "[astyle#myid.admin,options=\"read,write\"]";
        let mut state = ParserState::new(input);
        let (discrete, metadata) = document_parser::attributes(input, &mut state).unwrap();
        assert_eq!(discrete, false); // Not discrete
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "myid".to_string(),
                xreflabel: None,
                location: Location {
                    absolute_start: 8,
                    absolute_end: 12,
                    start: crate::Position { line: 1, column: 9 },
                    end: crate::Position {
                        line: 1,
                        column: 13
                    }
                }
            })
        );
        assert_eq!(metadata.style, Some("astyle".to_string()));
        assert!(metadata.roles.contains(&"admin".to_string()));
        assert!(metadata.options.contains(&"read".to_string()));
        assert!(metadata.options.contains(&"write".to_string()));
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_with_toc_block() {
        let input = "# This is a document\n:toc: macro\n\n[astyle#myid.admin,options=\"read,write\"]\ntoc::[]\n";
        let mut state = ParserState::new(input);
        let result = document_parser::document(input, &mut state)
            .unwrap()
            .unwrap();

        // Verify that we have a table of contents block
        assert_eq!(result.blocks.len(), 1);
        match &result.blocks[0] {
            Block::TableOfContents(toc) => {
                assert_eq!(toc.location.absolute_start, 34);
                assert_eq!(toc.location.absolute_end, 82);
            }
            _ => panic!("Expected TableOfContents block"),
        }
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_with_image_block() {
        let input = "image::sunset.jpg[alt=Sunset,width=300,height=400]\n";
        let mut state = ParserState::new(input);
        let result = document_parser::document(input, &mut state)
            .unwrap()
            .unwrap();
        // Verify that we have a table of contents block
        assert_eq!(result.blocks.len(), 1);
        matches!(&result.blocks[0], Block::Image(image)
                 if image.source == Source::Path("sunset.jpg".to_string())
                 && image.metadata.attributes.get("alt") == Some(&AttributeValue::String("Sunset".to_string()))
                 && image.metadata.attributes.get("width") == Some(&AttributeValue::String("300".to_string()))
                 && image.metadata.attributes.get("height") == Some(&AttributeValue::String("400".to_string()))
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_with_two_image_blocks() {
        let input = "image::sunset.jpg[alt=Sunset,width=300,height=400]\n\nimage::mountain.png[alt=Mountain,width=500,height=600]\n";
        let mut state = ParserState::new(input);
        let result = document_parser::document(input, &mut state)
            .unwrap()
            .unwrap();
        assert_eq!(result.blocks.len(), 2);
        matches!(&result.blocks[0], Block::Image(image)
                 if image.source == Source::Path("sunset.jpg".to_string())
                 && image.metadata.attributes.get("alt") == Some(&AttributeValue::String("Sunset".to_string()))
                 && image.metadata.attributes.get("width") == Some(&AttributeValue::String("300".to_string()))
                 && image.metadata.attributes.get("height") == Some(&AttributeValue::String("400".to_string()))
        );
        matches!(&result.blocks[1], Block::Image(image)
                 if image.source == Source::Path("mountain.png".to_string())
                 && image.metadata.attributes.get("alt") == Some(&AttributeValue::String("Mountain".to_string()))
                 && image.metadata.attributes.get("width") == Some(&AttributeValue::String("500".to_string()))
                 && image.metadata.attributes.get("height") == Some(&AttributeValue::String("600".to_string()))
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_with_two_video_ids() {
        let input = "video::Video1ID,Video2ID,Video3ID[youtube]\n";
        let mut state = ParserState::new(input);
        let result = document_parser::document(input, &mut state)
            .unwrap()
            .unwrap();
        assert_eq!(result.blocks.len(), 1);
        matches!(&result.blocks[0], Block::Video(video)
                 if video.sources == vec![Source::Path("Video1ID".to_string()),
                                          Source::Path("Video2ID".to_string()),
                                          Source::Path("Video3ID".to_string())]
                 && video.metadata.attributes.get("youtube") == Some(&AttributeValue::Bool(true))
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_with_page_breaks() {
        let input = "# a document with page breaks\n\n<<<\n\nThis is a new page.\n\n- - -\n\nAnd finally, the end.\n";
        let mut state = ParserState::new(input);
        let result = document_parser::document(input, &mut state)
            .unwrap()
            .unwrap();
        assert_eq!(result.blocks.len(), 4);
        matches!(&result.blocks[0], Block::PageBreak(page_break)
                 if page_break.location.absolute_start == 34
                 && page_break.location.absolute_end == 36
        );
        matches!(&result.blocks[1], Block::Paragraph(paragraph)
                 if paragraph.location.absolute_start == 38
                 && paragraph.location.absolute_end == 60
        );
        matches!(&result.blocks[2], Block::ThematicBreak(thematic_break)
                 if thematic_break.location.absolute_start == 62
                 && thematic_break.location.absolute_end == 64
        );
        matches!(&result.blocks[3], Block::Paragraph(paragraph)
                 if paragraph.location.absolute_start == 66
                 && paragraph.location.absolute_end == 90
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_with_admonition() {
        let input = "IMPORTANT: This is an important message.\n";
        let mut state = ParserState::new(input);
        let result = document_parser::document(input, &mut state)
            .unwrap()
            .unwrap();
        assert_eq!(result.blocks.len(), 1);
        matches!(&result.blocks[0], Block::Admonition(admonition)
                 if admonition.variant == AdmonitionVariant::Important
                 && admonition.blocks == vec![Block::Paragraph(Paragraph {
                     metadata: BlockMetadata {
                         anchors: vec![],
                         id: None,
                         style: None,
                         roles: vec![],
                         options: vec![],
                         attributes: DocumentAttributes::default()
                     },
                     title: vec![],
                     content: vec![InlineNode::PlainText(Plain {
                         content: "This is an important message.".to_string(),
                         location: Location {
                             absolute_start: 11,
                             absolute_end: 41,
                             start: crate::Position { line: 1, column: 12 },
                             end: crate::Position { line: 1, column: 42 }
                         }
                     })],
                     location: Location {
                         absolute_start: 11,
                         absolute_end: 41,
                         start: crate::Position { line: 1, column: 1 },
                         end: crate::Position { line: 1, column: 42 }
                     }
                 })]
        );
    }
}
