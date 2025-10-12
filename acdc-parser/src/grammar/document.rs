#![allow(clippy::too_many_arguments)]
use crate::{
    Admonition, AdmonitionVariant, Anchor, AttributeValue, Audio, Author, Autolink, Block,
    BlockMetadata, Bold, Button, CurvedApostrophe, CurvedQuotation, DelimitedBlock,
    DelimitedBlockType, DescriptionList, DescriptionListItem, DiscreteHeader, Document,
    DocumentAttribute, Error, Footnote, Form, Header, Highlight, Icon, Image, InlineMacro,
    InlineNode, Italic, Keyboard, LineBreak, Link, ListItem, ListItemCheckedStatus, Location, Menu,
    Monospace, OrderedList, PageBreak, Paragraph, Pass, PassthroughKind, Plain, Raw, Section,
    Source, StandaloneCurvedApostrophe, Subscript, Substitution, Superscript, Table,
    TableOfContents, TableRow, ThematicBreak, UnorderedList, Url, Video,
    error::Detail,
    grammar::{
        ParserState,
        author_revision::{RevisionInfo, generate_initials, process_revision_info},
        inline_preprocessing,
        inline_preprocessor::InlinePreprocessorParserState,
        inline_processing::{
            adjust_and_log_parse_error, parse_inlines, preprocess_inline_content, process_inlines,
        },
        location_mapping::map_inline_locations,
        table::parse_table_cell,
    },
    model::{ListLevel, SectionLevel},
};

#[derive(Debug)]
pub(crate) struct PositionWithOffset {
    pub(crate) offset: usize,
    pub(crate) position: crate::Position,
}

#[derive(Debug)]
// Used purely in the grammar to break down the block metadata lines into its different
// types.
enum BlockMetadataLine {
    Anchor(Anchor),
    Attributes((bool, BlockMetadata)),
    Title(Vec<InlineNode>),
}

#[derive(Debug, Default)]
// Used purely in the grammar to represent the parsed block details
pub(crate) struct BlockParsingMetadata {
    discrete: bool,
    pub(crate) metadata: BlockMetadata,
    title: Vec<InlineNode>,
    parent_section_level: Option<SectionLevel>,
    pub(crate) attribute_positions: std::collections::HashMap<String, (usize, usize)>,
}

#[derive(Debug)]
// Used purely inside the grammar to represent the style of a block
enum BlockStyle {
    Id(String, Option<(usize, usize)>),
    Role(String),
    Option(String),
}

const RESERVED_NAMED_ATTRIBUTE_ID: &str = "id";
const RESERVED_NAMED_ATTRIBUTE_ROLE: &str = "role";
const RESERVED_NAMED_ATTRIBUTE_OPTIONS: &str = "opts";

peg::parser! {
    pub(crate) grammar document_parser(state: &mut ParserState) for str {
        use std::str::FromStr;

        // We ignore empty lines before we set the start position of the document because
        // the asciidoc document should not consider empty lines at the beginning or end
        // of the file.
        pub(crate) rule document() -> Result<Document, Error>
        = eol()* start:position() newline_or_comment()* header:header() newline_or_comment()* blocks:blocks(0, None) end:position() (eol()* / ![_]) {
            let blocks = blocks?;
            let document_end_offset = end.offset.saturating_sub(1);
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
                    }} else {
                        start.position
                    },
                    end: if end.offset == 0 { crate::Position{
                        column: 0,
                        .. end.position
                    }} else {
                        state.line_map.offset_to_position(document_end_offset)
                    },
                },
                attributes: state.document_attributes.clone(),
                blocks,
                footnotes: state.footnote_tracker.footnotes.clone(),
                toc_entries: state.toc_tracker.entries.clone(),
            })
        }

        pub(crate) rule header() -> Option<Header>
            = start:position!()
            ((document_attribute() / comment()) (eol()+ / ![_]))*
            title_authors:(title_authors:title_authors() { title_authors })?
            (eol()+ (document_attribute() / comment()))*
            end:position!()
            (eol()*<,2> / ![_])
        {
            if let Some((title, subtitle, authors)) = title_authors {
                let mut location = state.create_location(start, end);
                location.absolute_end = location.absolute_end.saturating_sub(1);
                location.end.column = location.end.column.saturating_sub(1);
                Some(Header {
                    title,
                    subtitle,
                    authors,
                    location
                })
            } else {
                tracing::info!("No title or authors found in the document header.");
                None
            }
        }

        pub(crate) rule title_authors() -> (Vec<InlineNode>, Option<Vec<InlineNode>>, Vec<Author>)
        = title_and_subtitle:document_title() eol() authors:authors_and_revision() &(eol()+ / ![_])
        {
            let (title, subtitle) = title_and_subtitle;
            tracing::info!(?title, ?subtitle, ?authors, "Found title and authors in the document header.");
            (title, subtitle, authors)
        }
        / title_and_subtitle:document_title() &eol() {
            let (title, subtitle) = title_and_subtitle;
            tracing::info!(?title, ?subtitle, "Found title in the document header without authors.");
            (title, subtitle, vec![])
        }

        pub(crate) rule document_title() -> (Vec<InlineNode>, Option<Vec<InlineNode>>)
        = document_title_token() whitespace() start:position!() title:$([^'\n']*) end:position!()
        {
            let mut subtitle = None;
            let mut title_end = end;
            if let Some(subtitle_start) = title.rfind(':') {
                title_end = start+subtitle_start;
                subtitle = Some(vec![InlineNode::PlainText(Plain {
                    content: title[subtitle_start + 1..].trim().to_string(),
                    location: state.create_location(
                        title_end + 1,
                        end.saturating_sub(1),
                    ),
                })]);
            }
            let title_location = state.create_location(start, title_end.saturating_sub(1));
            (vec![InlineNode::PlainText(Plain {
                content: title[..title_end - start].trim().to_string(),
                location: title_location,
            })], subtitle)
        }

        rule document_title_token() = "=" / "#"

        rule authors_and_revision() -> Vec<Author>
            = authors:authors() (eol() revision())? {
                authors
            }

        pub(crate) rule authors() -> Vec<Author>
            = authors:(author() ++ (";" whitespace()*)) {
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
            = first:name_part() whitespace()+ middle:name_part() whitespace()+ last:$(name_part() ++ whitespace()) {
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
            = number:$("v"? digits() ++ ".") date:revision_date()? remark:revision_remark()? {
                let revision_info = RevisionInfo {
                    number: number.to_string(),
                    date: date.map(ToString::to_string),
                    remark: remark.map(ToString::to_string),
                };
                if revision_info.number.is_empty() {
                    // No revision number found, nothing to do
                    return;
                }
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
        = att:document_attribute_match() (&eol() / ![_])
        {
            tracing::info!(?att, "Found document attribute in the document header");
            let (key, value) = att;
            state.document_attributes.insert(key.to_string(), value);
        }

        pub(crate) rule blocks(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Vec<Block>, Error>
        = blocks:block(offset, parent_section_level)*
        {
            blocks.into_iter().collect::<Result<Vec<_>, Error>>()
        }


        pub(crate) rule block(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block, Error>
        = eol()* block:(
            document_attribute_block(offset) /
            &"[discrete" dh:discrete_header(offset) { dh } /
            !same_or_higher_level_section(offset, parent_section_level) section:section(offset, parent_section_level) { section } /
            block_generic(offset, parent_section_level)
        )
        {
            block
        }

        // Check if the upcoming content is a section at same or higher level (which should not be parsed as content)
        rule same_or_higher_level_section(offset: usize, parent_section_level: Option<SectionLevel>) -> ()
        = level:section_level(offset, parent_section_level)
        {?
            if let Some(parent_level) = parent_section_level {
                let upcoming_level = level.1 + 1; // Convert to 1-based
                if upcoming_level <= parent_level {
                    Ok(()) // This IS a same or higher level section, so the negative lookahead will fail
                } else {
                    Err("not a same or higher level section")
                }
            } else {
                Err("no parent section level to compare")
            }
        }

        rule discrete_header(offset: usize) -> Result<Block, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, None)
            {
                bm.map_err(|e| {
                    tracing::error!(?e, "error parsing block metadata");
                }).expect("block metadata errored out")
            }
        )
        section_level:section_level(offset, None) whitespace()
        title_start:position!() title:section_title(start, offset, &block_metadata) title_end:position!() end:position!() &eol()*<2,2>
        {
            let title = title?;
            tracing::info!(?block_metadata, ?title, ?title_start, ?title_end, "parsing discrete header block");

            let level = section_level.1;
            let location = state.create_location(start+offset, (end+offset).saturating_sub(1));

            Ok(Block::DiscreteHeader(DiscreteHeader {
                metadata: block_metadata.metadata,
                title,
                level,
                location,
            }))
        }

        pub(crate) rule document_attribute_block(offset: usize) -> Result<Block, Error>
        = start:position!() att:document_attribute_match() end:position!()
        {
            let (key, value) = att;
            state.document_attributes.insert(key.to_string(), value.clone());
            Ok(Block::DocumentAttribute(DocumentAttribute {
                name: key.to_string(),
                value,
                location: state.create_location(start+offset, end+offset)
            }))
        }

        pub(crate) rule section(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level)
            {
                bm.map_err(|e| {
                    tracing::error!(?e, "error parsing block metadata");
                }).expect("block metadata errored out")
            }
        )
        section_level_start:position!()
        section_level:section_level(offset, parent_section_level)
        section_level_end:position!()
        whitespace()
        title_start:position!()
        section_header:(title:section_title(start, offset, &block_metadata) title_end:position!() &eol()*<2,2> {
            let title = title?;
            let section_id = Section::generate_id(&block_metadata.metadata, &title).to_string();

            // Register section for TOC immediately after title is parsed, before content
            state.toc_tracker.register_section(title.clone(), section_level.1, section_id.clone());

            Ok::<(Vec<InlineNode>, String), Error>((title, section_id))
        })
        content:section_content(offset, Some(section_level.1+1))? end:position!()
        {
            let (title, section_id) = section_header?;
            tracing::info!(?offset, ?block_metadata, ?title, "parsing section block");

            // Validate section level against parent section level if any is provided
            if let Some(parent_level) = parent_section_level && (
                section_level.1 < parent_level  || section_level.1+1 > parent_level+1 || section_level.1 > 5) {
                    return Err(Error::NestedSectionLevelMismatch(
                        Detail { location: state.create_location(section_level_start + offset, (section_level_end + offset).saturating_sub(1)) },
                        section_level.1+1,
                        parent_level + 1,
                    ));
            }

            let level = section_level.1;
            let location = state.create_location(start+offset, (end+offset).saturating_sub(1));

            Ok(Block::Section(Section {
                metadata: block_metadata.metadata,
                title,
                level,
                content: content.unwrap_or(Ok(Vec::new()))?,
                location
            }))
        }

        rule block_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<BlockParsingMetadata, Error>
        = lines:(
            anchor:anchor() { Ok::<BlockMetadataLine, Error>(BlockMetadataLine::Anchor(anchor)) }
            / attr:attributes_line() { Ok(BlockMetadataLine::Attributes(attr)) }
            / title:title_line(offset) { Ok(BlockMetadataLine::Title(title?)) }
        )*
        {
            let mut metadata = BlockMetadata::default();
            let mut discrete = false;
            let mut title = Vec::new();

            for value in lines {
                let value = value?;
                match value {
                    BlockMetadataLine::Anchor(value) => metadata.anchors.push(value),
                    BlockMetadataLine::Attributes((attr_discrete, attr_metadata)) => {
                        discrete = attr_discrete;
                        metadata.id = attr_metadata.id;
                        metadata.style = attr_metadata.style;
                        metadata.roles.extend(attr_metadata.roles);
                        metadata.options.extend(attr_metadata.options);
                        metadata.attributes = attr_metadata.attributes;
                        metadata.positional_attributes = attr_metadata.positional_attributes;
                    },
                    BlockMetadataLine::Title(inner) => {
                        title = inner;
                    }
                    _ => unreachable!(),
                }
            }
            Ok(BlockParsingMetadata {
                discrete,
                metadata,
                title,
                parent_section_level,
                attribute_positions: std::collections::HashMap::new(),
            })
        }

        // A title line can be a simple title or a section title
        //
        // A title line is a line that starts with a period (.) followed by a non-whitespace character
        rule title_line(offset: usize) -> Result<Vec<InlineNode>, Error>
        = period() start:position() &(!(whitespace() / period())) title:$([^'\n']*) end:position!() eol()
        {
            tracing::info!(?title, ?start, ?end, "Found title line in block metadata");
            let block_metadata = BlockParsingMetadata::default();
            let (title, _) = process_inlines(state, &block_metadata, start.offset, &start, end, offset, title)?;
            Ok(title)
        }

        rule section_level(offset: usize, parent_section_level: Option<SectionLevel>) -> (&'input str, SectionLevel)
        = start:position() level:$(("=" / "#")*<1,6>) end:position!()
        {
            (level, level.len().try_into().unwrap_or(1)-1)
        }

        rule section_level_at_line_start(offset: usize, parent_section_level: Option<SectionLevel>) -> (&'input str, SectionLevel)
        = start:position() level:$(("=" / "#")*<1,6>) end:position!()
        {?
            // Only match section levels that are at the start of a line
            // Check if we're at the beginning of the input or after a newline
            let absolute_pos = start.offset + offset;
            let at_line_start = absolute_pos == 0 || {
                let prev_char_pos = absolute_pos.saturating_sub(1);
                state.input.chars().nth(prev_char_pos).is_some_and(|c| c == '\n')
            };

            if !at_line_start {
                return Err("section level must be at line start");
            }

            Ok((level, level.len().try_into().unwrap_or(1)-1))
        }

        rule section_title(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Vec<InlineNode>, Error>
        = title_start:position() title:$([^'\n']*) end:position!()
        {
            tracing::info!(?title, ?title_start, start, ?end, offset, "Found section title");
            let (content, _) = process_inlines(state, block_metadata, start, &title_start, end, offset, title)?;
            Ok(content)
        }

        rule section_content(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Vec<Block>, Error>
        = blocks(offset, parent_section_level) / { Ok(vec![]) }

        pub(crate) rule block_generic(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level)
            {
                bm.map_err(|e| {
                    tracing::error!(?e, "error parsing block metadata");
                }).expect("block metadata errored out")
            }
        )
        block:(
            delimited_block:delimited_block(start, offset, &block_metadata) { delimited_block }
            / image:image(start, offset, &block_metadata) { image }
            / audio:audio(start, offset, &block_metadata) { audio }
            / video:video(start, offset, &block_metadata) { video }
            / toc:toc(start, offset, &block_metadata) { toc }
            / thematic_break:thematic_break(start, offset, &block_metadata) { thematic_break }
            / page_break:page_break(start, offset, &block_metadata) { page_break }
            / list:list(start, offset, &block_metadata) { list }
            / paragraph:paragraph(start, offset, &block_metadata) { paragraph }
        ) {
            block
        }

        rule delimited_block(
            start: usize,
            offset: usize,
            block_metadata: &BlockParsingMetadata,
        ) -> Result<Block, Error>
        = comment_block(start, offset, block_metadata)
        / example_block(start, offset, block_metadata)
        / listing_block(start, offset, block_metadata)
        / literal_block(start, offset, block_metadata)
        / open_block(start, offset, block_metadata)
        / sidebar_block(start, offset, block_metadata)
        / table_block(start, offset, block_metadata)
        / pass_block(start, offset, block_metadata)
        / quote_block(start, offset, block_metadata)

        // Delimiter recognition rules
        rule comment_delimiter() -> &'input str = delim:$("/"*<4,>) { delim }
        rule example_delimiter() -> &'input str = delim:$("="*<4,>) { delim }
        rule listing_delimiter() -> &'input str = delim:$("-"*<4,>) { delim }
        rule literal_delimiter() -> &'input str = delim:$("."*<4,>) { delim }
        rule open_delimiter() -> &'input str = delim:$("-"*<2,> / "~"*<4,>) { delim }
        rule sidebar_delimiter() -> &'input str = delim:$("*"*<4,>) { delim }
        rule table_delimiter() -> &'input str = delim:$((['|' | ',' | ':' | '!'] "="*<3,>)) { delim }
        rule pass_delimiter() -> &'input str = delim:$("+"*<4,>) { delim }
        rule markdown_code_delimiter() -> &'input str = delim:$("`"*<3,>) { delim }
        rule quote_delimiter() -> &'input str = delim:$("_"*<4,>) { delim }

        rule until_comment_delimiter() -> &'input str
            = content:$((!(eol() comment_delimiter()) [_])*)
        {
            content
        }

        rule until_example_delimiter() -> &'input str
            = content:$((!(eol() example_delimiter()) [_])*)
        {
            content
        }

        rule until_listing_delimiter() -> &'input str
            = content:$((!(eol() listing_delimiter()) [_])*)
        {
            content
        }

        rule until_literal_delimiter() -> &'input str
            = content:$((!(eol() literal_delimiter()) [_])*)
        {
            content
        }

        rule until_open_delimiter() -> &'input str
        = content:$((!(eol() open_delimiter()) [_])*)
        {
            content
        }

        rule until_sidebar_delimiter() -> &'input str
            = content:$((!(eol() sidebar_delimiter()) [_])*)
        {
            content
        }

        rule until_table_delimiter() -> &'input str
            = content:$((!(eol() table_delimiter()) [_])*)
        {
            content
        }

        rule until_pass_delimiter() -> &'input str
            = content:$((!(eol() pass_delimiter()) [_])*)
        {
            content
        }

        rule until_quote_delimiter() -> &'input str
            = content:$((!(eol() quote_delimiter()) [_])*)
        {
            content
        }

        rule until_markdown_code_delimiter() -> &'input str
            = content:$((!(eol() markdown_code_delimiter()) [_])*)
        {
            content
        }

        rule markdown_language() -> &'input str
            = lang:$((['a'..='z'] / ['A'..='Z'] / ['0'..='9'] / "_" / "+" / "-")+)
        {
            lang
        }

        rule example_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = open_delim:example_delimiter() eol()
        content_start:position!() content:until_example_delimiter() content_end:position!()
        eol() close_delim:example_delimiter() end:position!()
        {
            tracing::info!(?start, ?offset, ?content_start, ?block_metadata, ?content, "Parsing example block");

            if open_delim != close_delim {
                return Err(Error::MismatchedDelimiters("example".to_string()));
            }
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_location(start+offset, (end+offset).saturating_sub(1));

            let blocks = if content.trim().is_empty() {
                Vec::new()
            } else {
                document_parser::blocks(content, state, content_start+offset, block_metadata.parent_section_level).unwrap_or_else(|e| {
                    adjust_and_log_parse_error(&e, content, content_start+offset, state, "Error parsing example content as blocks in example block");
                    Ok(Vec::new())
                })?
            };

            // We want to detect if this is an admonition block. We do that by checking if
            // we have a style that matches an admonition variant.
            if let Some(ref style) = block_metadata.metadata.style &&
            let Ok(admonition_variant) = AdmonitionVariant::from_str(style) {
                tracing::debug!(?admonition_variant, "Detected admonition block with variant");
                let mut metadata = block_metadata.metadata.clone();
                metadata.style = None; // Clear style to avoid confusion
                return Ok(Block::Admonition(Admonition {
                    variant: admonition_variant,
                    blocks,
                    metadata,
                    title: block_metadata.title.clone(),
                    location,
                }));
            }

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedExample(blocks),
                title: block_metadata.title.clone(),
                location,
            }))
        }

        rule comment_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
            = open_delim:comment_delimiter() eol()
            content_start:position!() content:until_comment_delimiter() content_end:position!()
            eol() close_delim:comment_delimiter() end:position!()
        {
            if open_delim != close_delim {
                return Err(Error::MismatchedDelimiters("comment".to_string()));
            }
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();

            let location = state.create_location((start+offset), (end+offset).saturating_sub(1));
            let content_location = state.create_location(content_start+offset, (content_end+offset).saturating_sub(1));

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata,
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedComment(vec![InlineNode::PlainText(Plain {
                    content: content.to_string(),
                    location: content_location,
                })]),
                title: block_metadata.title.clone(),
                location,
            }))
        }

        rule listing_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
            = traditional_listing_block(start, offset, block_metadata)
            / markdown_listing_block(start, offset, block_metadata)

        rule traditional_listing_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
            = open_delim:listing_delimiter() eol()
            content_start:position!() content:until_listing_delimiter() content_end:position!()
            eol() close_delim:listing_delimiter() end:position!()
        {
            if open_delim != close_delim {
                return Err(Error::MismatchedDelimiters("listing".to_string()));
            }
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_location(start+offset, (end+offset).saturating_sub(1));
            let content_location = state.create_location(content_start+offset, (content_end+offset).saturating_sub(1));

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedListing(vec![InlineNode::PlainText(Plain {
                    content: content.to_string(),
                    location: content_location,
                })]),
                title: block_metadata.title.clone(),
                location,
            }))
        }

        rule markdown_listing_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
            = open_delim:markdown_code_delimiter() lang:markdown_language()? eol()
            content_start:position!() content:until_markdown_code_delimiter() content_end:position!()
            eol() close_delim:markdown_code_delimiter() end:position!()
        {
            if open_delim != close_delim {
                return Err(Error::MismatchedDelimiters("listing".to_string()));
            }
            let mut metadata = block_metadata.metadata.clone();

            // If we captured a language, add it as a positional attribute
            if let Some(language) = lang {
                metadata.positional_attributes.insert(0, language.to_string());
            }

            metadata.move_positional_attributes_to_attributes();
            let location = state.create_location(start+offset, (end+offset).saturating_sub(1));
            let content_location = state.create_location(content_start+offset, (content_end+offset).saturating_sub(1));

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedListing(vec![InlineNode::PlainText(Plain {
                    content: content.to_string(),
                    location: content_location,
                })]),
                title: block_metadata.title.clone(),
                location,
            }))
        }

        pub(crate) rule literal_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        =
        open_delim:literal_delimiter()
        eol()
        content_start:position!() content:until_literal_delimiter() content_end:position!()
        eol()
        close_delim:literal_delimiter()
        end:position!()
        {
            if open_delim != close_delim {
                return Err(Error::MismatchedDelimiters("literal".to_string()));
            }
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_location(start+offset, (end+offset).saturating_sub(1));
            let content_location = state.create_location(content_start+offset, (content_end+offset).saturating_sub(1));

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata,
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedLiteral(vec![InlineNode::PlainText(Plain {
                    content: content.to_string(),
                    location: content_location,
                })]),
                title: block_metadata.title.clone(),
                location,
            }))
        }

        rule open_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
            = open_delim:open_delimiter() eol()
            content_start:position!() content:until_open_delimiter() content_end:position!()
            eol() close_delim:open_delimiter() end:position!()
        {
            if open_delim != close_delim {
                return Err(Error::MismatchedDelimiters("open".to_string()));
            }
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_location(start+offset, (end+offset).saturating_sub(1));

            let blocks = if content.trim().is_empty() {
                Vec::new()
            } else {
                let content_location = state.create_location(content_start+offset, (content_end+offset).saturating_sub(1));
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
                title: block_metadata.title.clone(),
                location,
            }))
        }

        rule sidebar_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
            = open_delim:sidebar_delimiter() eol()
            content_start:position!() content:until_sidebar_delimiter() content_end:position!()
            eol() close_delim:sidebar_delimiter() end:position!()
        {
            tracing::info!(?start, ?offset, ?content_start, ?block_metadata, ?content, "Parsing sidebar block");

            if open_delim != close_delim {
                return Err(Error::MismatchedDelimiters("sidebar".to_string()));
            }
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_location(start+offset, (end+offset).saturating_sub(1));

            let blocks = if content.trim().is_empty() {
                Vec::new()
            } else {
                document_parser::blocks(content, state, content_start+offset, block_metadata.parent_section_level).unwrap_or_else(|e| {
                    adjust_and_log_parse_error(&e, content, content_start+offset, state, "Error parsing sidebar content as blocks");
                    Ok(Vec::new())
                })?
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedSidebar(blocks),
                title: block_metadata.title.clone(),
                location,
            }))
        }

        rule table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = table_start:position!() open_delim:table_delimiter() eol()
        content_start:position!() content:until_table_delimiter() content_end:position!()
        eol() close_delim:table_delimiter() end:position!()
        {
            if open_delim != close_delim {
                return Err(Error::MismatchedDelimiters("table".to_string()));
            }
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_location(start+offset, (end+offset).saturating_sub(1));
            let table_location = state.create_location(table_start+offset, (end+offset).saturating_sub(1));
            let content_location = state.create_location(content_start+offset, (content_end+offset).saturating_sub(1));

            let separator = if let Some(AttributeValue::String(sep)) = block_metadata.metadata.attributes.get("separator") {
                sep.clone()
            } else {
                let mut separator = "|".to_string();
                if let Some(AttributeValue::String(format)) = block_metadata.metadata.attributes.get("format") {
                    separator = match format.as_str() {
                        "csv" => ",".to_string(),
                        "dsv" => ":".to_string(),
                        "tsv" => "\t".to_string(),
                        format => unimplemented!("unkown table format: {format}"),
                    };
                }
                separator
            };

            let ncols = if let Some(AttributeValue::String(cols)) = block_metadata.metadata.attributes.get("cols") {
                Some(cols.split(',').count())
            } else {
                None
            };

            // Set this to true if the user mandates it!
            let mut has_header = block_metadata.metadata.options.contains(&String::from("header"));
            let raw_rows = Table::parse_rows_with_positions(content, &separator, &mut has_header, content_start+offset);

            // If the user forces a noheader, we should not have a header, so after we've
            // tried to figure out if there are any headers, we should set it to false one
            // last time.
            if block_metadata.metadata.options.contains(&String::from("noheader")) {
                has_header = false;
            }
            let has_footer = block_metadata.metadata.options.contains(&String::from("footer"));

            let mut header = None;
            let mut footer = None;
            let mut rows = Vec::new();

            for (i, row) in raw_rows.iter().enumerate() {
                let columns = row
                .iter()
                .filter(|(cell, _, _)| !cell.is_empty())
                .map(|(cell, start, _end)| parse_table_cell(cell, state, *start, block_metadata.parent_section_level))
                .collect::<Result<Vec<_>, _>>()?;
                // validate that if we have ncols we have the same number of columns in each row
                if let Some(ncols) = ncols
                && columns.len() != ncols
                {
                    return Err(Error::InvalidTableColumnLength(columns.len(), ncols));
                }

                // if we have a header, we need to add the columns we have to the header
                if has_header {
                    header = Some(TableRow { columns });
                    has_header = false;
                    continue;
                }

                // if we have a footer, we need to add the columns we have to the footer
                if has_footer && i == raw_rows.len() - 1 {
                    footer = Some(TableRow { columns });
                    continue;
                }

                // if we get here, these columns are a row
                rows.push(TableRow { columns });
            }

            let table = Table {
                header,
                footer,
                rows,
                location: table_location.clone(),
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedTable(table),
                title: block_metadata.title.clone(),
                location,
            }))
        }

        rule pass_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
            = open_delim:pass_delimiter() eol()
            content_start:position!() content:until_pass_delimiter() content_end:position!()
            eol() close_delim:pass_delimiter() end:position!()
        {
            if open_delim != close_delim {
                return Err(Error::MismatchedDelimiters("pass".to_string()));
            }
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_location(start+offset, (end+offset).saturating_sub(1));
            let content_location = state.create_location(content_start+offset, (content_end+offset).saturating_sub(1));

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedPass(vec![InlineNode::RawText(Raw {
                    content: content.to_string(),
                    location: content_location,
                })]),
                title: block_metadata.title.clone(),
                location,
            }))
        }

        rule quote_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
            = open_delim:quote_delimiter() eol()
            content_start:position!() content:until_quote_delimiter() content_end:position!()
            eol() close_delim:quote_delimiter() end:position!()
        {
            if open_delim != close_delim {
                return Err(Error::MismatchedDelimiters("quote".to_string()));
            }
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_location(start+offset, (end+offset).saturating_sub(1));
            let content_location = state.create_location(content_start+offset, (content_end+offset).saturating_sub(1));

            let inner = if let Some(ref style) = metadata.style {
                if style == "verse" {
                    DelimitedBlockType::DelimitedVerse(vec![InlineNode::PlainText(Plain {
                        content: content.to_string(),
                        location: content_location.clone(),
                    })])
                } else {
                    let blocks = document_parser::blocks(content, state, content_start+offset, block_metadata.parent_section_level).unwrap_or_else(|e| {
                        adjust_and_log_parse_error(&e, content, content_start+offset, state, "Error parsing example content as blocks in quote block");
                        Ok(Vec::new())
                    })?;
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
                title: block_metadata.title.clone(),
                location,
            }))
        }

        rule toc(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = "toc::[]" end:position!()
        {
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            tracing::info!("Found Table of Contents block");
            Ok(Block::TableOfContents(TableOfContents {
                metadata: metadata.clone(),
                location: state.create_location(start+offset, end+offset),
            }))
        }

        rule image(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = "image::" source:source() attributes:attributes() end:position!()
        {
            let (_discrete, metadata_from_attributes, _title_position) = attributes;
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            if let Some(style) = metadata.style {
                metadata.style = None; // Clear style to avoid confusion
                metadata.attributes.insert("alt".to_string(), AttributeValue::String(style.clone()));
            }
            if metadata.positional_attributes.len() >= 2 {
                metadata.attributes.insert("height".to_string(), AttributeValue::String(metadata.positional_attributes.remove(1)));
            }
            if !metadata.positional_attributes.is_empty() {
                metadata.attributes.insert("width".to_string(), AttributeValue::String(metadata.positional_attributes.remove(0)));
            }
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Image(Image {
                title,
                source,
                metadata,
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),

            }))
        }

        rule audio(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = "audio::" source:source() attributes:attributes() end:position!()
        {
            let (_discrete, metadata_from_attributes, _title_position) = attributes;
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Audio(Audio {
                title,
                source,
                metadata,
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            }))
        }

        // The video block is similar to the audio and image blocks, but it supports
        // multiple sources. This is for example to allow passing multiple youtube video
        // ids to form a playlist.
        rule video(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = "video::" sources:(source() ** comma()) attributes:attributes() end:position!()
        {
            let (_discrete, metadata_from_attributes, _title_position) = attributes;
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            if let Some(style) = metadata.style {
                metadata.style = None;
                if style == "youtube" || style == "vimeo" {
                    tracing::debug!(?metadata, "transforming video metadata style into attribute");
                    metadata.attributes.insert(style.to_string(), AttributeValue::Bool(true));
                } else {
                    // assume poster
                    tracing::debug!(?metadata, "transforming video metadata style into attribute, assuming poster");
                    metadata.attributes.insert("poster".to_string(), AttributeValue::String(style.clone()));
                }
            }
            if metadata.positional_attributes.len() >= 2 {
                metadata.attributes.insert("height".to_string(), AttributeValue::String(metadata.positional_attributes.remove(1)));
            }
            if !metadata.positional_attributes.is_empty() {
                metadata.attributes.insert("width".to_string(), AttributeValue::String(metadata.positional_attributes.remove(0)));
            }
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Video(Video {
                title,
                sources,
                metadata,
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            }))
        }

        rule thematic_break(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
            = ("'''"
               // Below are the markdown-style thematic breaks
               / "---"
               / "- - -"
               / "***"
               / "* * *"
            ) end:position!()
        {
            tracing::info!("Found thematic break block");
            Ok(Block::ThematicBreak(ThematicBreak {
                anchors: block_metadata.metadata.anchors.clone(), // TODO(nlopes): should this simply be metadata?
                title: Vec::new(), // TODO(nlopes): Handle thematic break titles
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            }))
        }

        rule page_break(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
            = "<<<" end:position!() &eol()*<2,2>
        {
            tracing::info!("Found page break block");
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();

            Ok(Block::PageBreak(PageBreak {
                title: block_metadata.title.clone(),
                metadata,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        rule list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = unordered_list(start, offset, block_metadata) / ordered_list(start, offset, block_metadata) / description_list(start, offset, block_metadata)

        rule unordered_list_marker() -> &'input str = $("*"+ / "-")

        rule ordered_list_marker() -> &'input str = $(digits()? "."+)

        rule description_list_marker() -> &'input str = $("::::" / ":::" / "::" / ";;")

        rule section_level_marker() -> &'input str = $(("=" / "#")+)

        rule unordered_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = &(unordered_list_marker() whitespace()) content:list_item(offset, block_metadata)+ end:position!()
        {
            tracing::info!(?content, "Found unordered list block");
            // TODO(nlopes): this is very very inneficient and silly - right now I'm just
            // trying to remove all .unwraps so this is fine. Will come back to this once
            // I'm going for optimisations.
            let content: Vec<_> = content.into_iter().collect::<Result<_, Error>>()?;
            let end = content.last().map_or(end, |(_, item_end)| *item_end);
            let items: Vec<ListItem> = content.into_iter().map(|(item, end)| item).collect();
            let marker = items.first().map_or(String::new(), |item| item.marker.clone());

            Ok(Block::UnorderedList(UnorderedList {
                title: block_metadata.title.clone(),
                metadata: block_metadata.metadata.clone(),
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        rule ordered_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = &(ordered_list_marker() whitespace()) content:list_item(offset, block_metadata)+ end:position!()
        {
            tracing::info!(?content, "Found ordered list block");
            // TODO(nlopes): this is very very inneficient and silly - right now I'm just
            // trying to remove all .unwraps so this is fine. Will come back to this once
            // I'm going for optimisations.
            let content: Vec<_> = content.into_iter().collect::<Result<_, Error>>()?;
            let end = content.last().map_or(end, |(_, item_end)| *item_end);
            let items: Vec<ListItem> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or(String::new(), |item| item.marker.clone());

            Ok(Block::OrderedList(OrderedList {
                title: block_metadata.title.clone(),
                metadata: block_metadata.metadata.clone(),
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        rule list_item(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<(ListItem, usize), Error>
        = start:position!()
        marker:(unordered_list_marker() / ordered_list_marker())
        whitespace()
        checked:checklist_item()?
        list_content_start:position()
        list_item:$((!(&(eol()+ (unordered_list_marker() / ordered_list_marker() / check_start_of_description_list() / section_level_marker())) / eol()*<2,> / ![_]) [_])+)
        end:position!() (eol()+ / ![_])
        {
            tracing::info!(%list_item, %marker, ?checked, "found list item");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let (content, _) = process_inlines(state, block_metadata, start, &list_content_start, end, offset, list_item)?;
            let end = end.saturating_sub(1);


            Ok((ListItem {
                content,
                level,
                marker: marker.to_string(),
                checked,
                location: state.create_location(start+offset, end+offset),
            }, end))
        }

        rule checklist_item() -> ListItemCheckedStatus
            = checked:(("[x]" / "[X]" / "[*]") { ListItemCheckedStatus::Checked } / "[ ]" { ListItemCheckedStatus::Unchecked }) whitespace()
        {
            checked
        }

        rule check_start_of_description_list()
        = &((!(description_list_marker() (eol() / " ")) [_])+ description_list_marker())

        rule description_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = check_start_of_description_list()
        first_item:description_list_item(offset, block_metadata)
        additional_items:description_list_additional_items(offset, block_metadata)*
        end:position!()
        {
            tracing::info!("Found description list block with auto-attachment support");
            let mut items = vec![first_item?];

            for additional in additional_items {
                items.push(additional?);
            }

            let actual_end = items.last().map_or(end, |item| {
                let loc_end = item.location.absolute_end;
                loc_end - offset
            });

            Ok(Block::DescriptionList(DescriptionList {
                title: block_metadata.title.clone(),
                metadata: block_metadata.metadata.clone(),
                items,
                location: state.create_location(start+offset, actual_end+offset),
            }))
        }

        // Parse additional description list items (after potential auto-attached content)
        rule description_list_additional_items(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<DescriptionListItem, Error>
        = eol()*
        check_start_of_description_list()
        item:description_list_item(offset, block_metadata)
        {
            tracing::info!("Found additional description list item");
            item
        }

        rule description_list_item(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<DescriptionListItem, Error>
        = start:position!()
        term:$((!(description_list_marker() (eol() / " ") / eol()*<2,2>) [_])+)
        delimiter:description_list_marker()
        whitespace()?
        principal_start:position!()
        principal_content:$((!eol() [_])*)
        // No contiguous lines - they would be parsed as separate blocks or items
        // Now handle auto-attachment and explicit continuation
        attached_content:description_list_attached_content(offset, block_metadata)*
        end:position!()
        {
            tracing::info!(%term, %delimiter, "parsing description list item with auto-attachment");

            let term = document_parser::inlines(term.trim(), state, start+offset, block_metadata)
                .unwrap_or_else(|e| {
                    adjust_and_log_parse_error(&e, term.trim(), start+offset, state, "Error parsing term as inline content");
                    vec![]
                });

            let principal_text = if principal_content.trim().is_empty() {
                Vec::new()
            } else {
                // Parse as inline content
                document_parser::inlines(principal_content.trim(), state, principal_start+offset, block_metadata)
                    .unwrap_or_else(|e| {
                        adjust_and_log_parse_error(&e, principal_content.trim(), principal_start+offset, state, "Error parsing principal text as inline content");
                        vec![]
                    })
            };

            // Collect all attached blocks (auto-attached and explicitly continued)
            let mut description = Vec::new();
            for content in attached_content {
                match content {
                    Ok(blocks) => description.extend(blocks),
                    Err(e) => {
                        tracing::error!(?e, "Error processing attached content");
                    }
                }
            }

            Ok(DescriptionListItem {
                anchors: vec![],
                term,
                delimiter: delimiter.to_string(),
                principal_text,
                description,
                location: state.create_location(start+offset, end+offset),
            })
        }

        rule description_list_attached_content(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Vec<Block>, Error>
        = eol() content:(
            // Explicit continuation - this uses +, allows any content including delimited
            // blocks
            description_list_explicit_continuation(offset, block_metadata)
            // Auto-attach lists (even with blank lines before them)
            / description_list_auto_attached_list(offset, block_metadata)
        )
        {
            content
        }

        rule description_list_auto_attached_list(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Vec<Block>, Error>
        = eol()* // Consume any blank lines before the list
        &((unordered_list_marker() / ordered_list_marker()) whitespace())
        list_start:position!()
        list:(unordered_list(list_start, offset, block_metadata) / ordered_list(list_start, offset, block_metadata))
        {
            tracing::info!("Auto-attaching list to description list item");
            Ok(vec![list?])
        }

        rule description_list_explicit_continuation(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Vec<Block>, Error>
        = "+" eol()
        continuation_start:position!()
        // Capture lines until we see another description list item
        // This consumes everything until it hits a line that starts with a description list term
        content:$((!(eol() check_start_of_description_list()) [_])*)
        end:position!()
        {
            tracing::info!(?content, start = ?continuation_start, ?end, "Explicit continuation content");

            let trimmed = content.trim_end();
            if trimmed.is_empty() {
                Ok(Vec::new())
            } else {
                document_parser::blocks(trimmed, state, continuation_start+offset, block_metadata.parent_section_level)
                    .unwrap_or_else(|e| {
                        adjust_and_log_parse_error(&e, trimmed, continuation_start+offset, state, "Error parsing continuation content");
                        Ok(Vec::new())
                    })
            }
        }

        pub(crate) rule inlines(offset: usize, block_metadata: &BlockParsingMetadata) -> Vec<InlineNode>
        = (non_plain_text(offset, block_metadata) / plain_text(offset, block_metadata))+

        rule non_plain_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = inline:(
            cross_reference_shorthand:cross_reference_shorthand(offset) { cross_reference_shorthand }
            / cross_reference_macro:cross_reference_macro(offset) { cross_reference_macro }
            / hard_wrap:hard_wrap(offset) { hard_wrap }
            / &"footnote:" footnote:footnote(offset, block_metadata) { footnote }
            / image:inline_image(offset, block_metadata) { image }
            / icon:inline_icon(offset, block_metadata) { icon }
            / keyboard:inline_keyboard(offset) { keyboard }
            / button:inline_button(offset) { button }
            / menu:inline_menu(offset) { menu }
            / url_macro:url_macro(offset, block_metadata) { url_macro }
            / pass:inline_pass(offset) { pass }
            / link_macro:link_macro(offset) { link_macro }
            / inline_autolink:inline_autolink(offset) { inline_autolink }
            / inline_line_break:inline_line_break(offset) { inline_line_break }
            / bold_text_unconstrained:bold_text_unconstrained(offset, block_metadata) { bold_text_unconstrained }
            / bold_text_constrained:bold_text_constrained(offset, block_metadata) { bold_text_constrained }
            / italic_text_unconstrained:italic_text_unconstrained(offset, block_metadata) { italic_text_unconstrained }
            / italic_text_constrained:italic_text_constrained(offset, block_metadata) { italic_text_constrained }
            / monospace_text_unconstrained:monospace_text_unconstrained(offset, block_metadata) { monospace_text_unconstrained }
            / monospace_text_constrained:monospace_text_constrained(offset, block_metadata) { monospace_text_constrained }
            / highlight_text_unconstrained:highlight_text_unconstrained(offset, block_metadata) { highlight_text_unconstrained }
            / highlight_text_constrained:highlight_text_constrained(offset, block_metadata) { highlight_text_constrained }
            / superscript_text:superscript_text(offset, block_metadata) { superscript_text }
            / subscript_text:subscript_text(offset, block_metadata) { subscript_text }
            / curved_quotation_text:curved_quotation_text(offset, block_metadata) { curved_quotation_text }
            / curved_apostrophe_text:curved_apostrophe_text(offset, block_metadata) { curved_apostrophe_text }
            / standalone_curved_apostrophe:standalone_curved_apostrophe(offset, block_metadata) { standalone_curved_apostrophe }
            ) {
                inline
            }

        rule footnote(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = footnote_match:footnote_match(offset, block_metadata)
        {?
            let (start, id, content_start, content_str, end) = footnote_match;

            tracing::info!(?id, content = %content_str, "Found footnote inline");

            // If content_str is empty or only whitespace, we should not try to process
            // inlines, it just means this footnote has no content and therefore the user
            // has already added the content in a footnote with the same id but with
            // content.
            let content = if content_str.trim().is_empty() {
                vec![]
            } else {
                let (content, _) = process_inlines(state, block_metadata, content_start.offset, &content_start, end, offset, &content_str).map_err(|e| {
                tracing::error!(?e, "could not process footnote content");
                "could not process footnote content"
                })?;
                content
            };

            let mut footnote = Footnote {
                id: id.clone(),
                content,
                number: 0, // Will be set by register_footnote
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            };
            state.footnote_tracker.push(&mut footnote);

            Ok(InlineNode::Macro(InlineMacro::Footnote(footnote)))
        }

        rule footnote_match(offset: usize, block_metadata: &BlockParsingMetadata) -> (usize, Option<String>, PositionWithOffset, String, usize)
        = start:position!()
        "footnote:"
        // TODO(nlopes): we should change this so that we require an id if content is empty
        id:id()? "[" content_start:position() content:balanced_bracket_content() "]"
        end:position!()
        {
            (start, id, content_start, content.to_string(), end)

        }

        /// Parse content that may contain balanced square brackets (general case)
        /// This is used for footnotes, link titles and button labels
        rule balanced_bracket_content() -> String
        = content:$(balanced_bracket_content_part()*) { content.to_string() }

        /// Individual parts of balanced bracket content - either regular text or nested brackets
        rule balanced_bracket_content_part() -> String
        = nested_brackets:("[" inner:balanced_bracket_content() "]" { format!("[{inner}]") })
        / regular_text:$([^('[' | ']')]+) { regular_text.to_string() }

        /// Parse link/URL title content that may contain balanced brackets
        ///
        /// This is similar to balanced_bracket_content but stops at comma and attribute
        /// patterns
        ///
        /// Supports two formats:
        /// 1. **Quoted text**: `"any text including 'quotes' and ,commas"`
        /// 2. **Unquoted text**: `any text until , or ] or name=value`
        ///
        /// Unlike block attributes, link titles can contain:
        /// - Single quotes: `link:file[see the 'source' code]`
        /// - Periods: `link:file[version 1.2.3 notes]`
        /// - Hash symbols: `link:file[C# programming guide]`
        /// - Other special characters that would terminate block attribute parsing
        ///
        /// The unquoted parsing stops at:
        /// - `,` (start of attributes)
        /// - `]` (end of link)
        /// - `name=` patterns (attribute definitions)
        rule link_title() -> String
        = "\"" title:$((!"\"" [_])*) "\"" { title.to_string() }
        / parts:$(balanced_link_title_part()+) { parts.to_string() }

        /// Parse parts of link title content
        rule balanced_link_title_part() -> String
        = nested_brackets:("[" inner:balanced_bracket_content() "]" { format!("[{inner}]") })
        / regular_text:$((!("," whitespace()* (attribute_name() "=" / "]")) [^'[' | ']'])+) { regular_text.to_string() }

        rule inline_pass(offset: usize) -> InlineNode
        = start:position!()
        "pass:"
        substitutions:($([^(']' | ',')]+) ** comma())
        "["
        content:$([^']']+)
        "]"
        end:position!()
        {
            tracing::info!(?content, "Found pass inline");
            InlineNode::Macro(InlineMacro::Pass(Pass {
                text: Some(content.trim().to_string()),
                substitutions: substitutions.into_iter().map(|s| Substitution::from(s.trim())).collect(),
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
                kind: PassthroughKind::Macro,
            }))
        }

        rule inline_menu(offset: usize) -> InlineNode
        = start:position!()
        "menu:"
        target:$([^'[']+)
        "["
        items:((item:$([^(']' | '>')]+) { item.trim().to_string() }) ** (">" whitespace()?))
        "]"
        end:position!()
        {
            tracing::info!(%target, ?items, "Found menu inline");
            InlineNode::Macro(InlineMacro::Menu(Menu {
                target: target.to_string(),
                items,
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            }))
        }

        rule inline_button(offset: usize) -> InlineNode
        = start:position!()
        "btn:[" label:$balanced_bracket_content() "]" end:position!()
        {
            tracing::info!(?label, "Found button inline");
            InlineNode::Macro(InlineMacro::Button(Button {
                label: label.trim().to_string(),
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            }))
        }

        rule inline_keyboard(offset: usize) -> InlineNode
        = start:position!()
        "kbd:["
        keys:((key:$([^(']' | '+' | ',')]+) { key.trim().to_string() }) ** (("," / "+") whitespace()?))
        "]"
        end:position!()
        {
            tracing::info!(?keys, "Found keyboard inline");
            InlineNode::Macro(InlineMacro::Keyboard(Keyboard {
                keys,
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            }))
        }

        /// Parse URL macros with attribute handling.
        ///
        /// URL macros have the format: `https://example.com[text,attr1=value1,attr2=value2]`
        ///
        /// This is similar to link macros but the URL is directly specified rather than
        /// using the `link:` prefix.
        rule url_macro(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position()
        target:url()
        "["
        content:(
            title:link_title() attributes:("," att:attribute() { att })* {
                (Some(title), attributes.into_iter().flatten().collect::<Vec<_>>())
            } /
            attributes:(att:attribute() comma()? { att })* {
                (None, attributes.into_iter().flatten().collect::<Vec<_>>())
            }
        )
        "]"
        end:position!()
        {
            tracing::info!(?target, "Found url macro");
            let (text, attributes) = content;
            let mut metadata = BlockMetadata::default();
            for (k, v, _pos) in attributes {
                if let AttributeValue::String(v) = v {
                    metadata.attributes.insert(k, AttributeValue::String(v));
                }
            }
            let text = if let Some(text) = text {
                process_inlines(state, block_metadata, start.offset, &start, end, offset, &text).map_err(|e| {
                    tracing::error!(?e, "could not process url macro text");
                }).expect("could not process url macro text").0
            } else {
                vec![]
            };
            InlineNode::Macro(InlineMacro::Url(Url {
                text,
                target: Source::Url(target),
                attributes: metadata.attributes.clone(),
                location: state.create_location(start.offset+offset, (end+offset).saturating_sub(1)),
            }))
        }

        rule inline_autolink(offset: usize) -> InlineNode
        = start:position!()
        url:(
            "<" url:url() ">" { url }
            / url:url() { url }
        )
        end:position!()
        {
            tracing::info!(?url, "Found autolink inline");
            InlineNode::Macro(InlineMacro::Autolink(Autolink {
                url: Source::Url(url),
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            }))
        }

        rule inline_line_break(offset: usize) -> InlineNode
        = start:position!() " +" end:position!() eol()
        {
            tracing::info!("Found inline line break");
            InlineNode::LineBreak(LineBreak {
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            })
        }

        rule hard_wrap(offset: usize) -> InlineNode
            = start:position!() " + \\" end:position!() &eol()
        {
            tracing::info!("Found hard wrap inline");
            InlineNode::LineBreak(LineBreak {
                location: state.create_location((start+offset), (end + offset).saturating_sub(1)),
            })
        }

        rule inline_icon(offset: usize, _block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position() "icon:" source:source() attributes:attributes() end:position!()
        {
            let (_discrete, metadata, _title_position) = attributes;
            let mut metadata = metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            InlineNode::Macro(InlineMacro::Icon(Icon {
                target: source,
                attributes: metadata.attributes.clone(),
                location: state.create_location(start.offset+offset, (end+offset).saturating_sub(1)),

            }))
        }

        rule inline_image(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position() "image:" source:source() attributes:attributes() end:position!()
        {?
            let (_discrete, metadata, title_position) = attributes;
            let mut metadata = metadata.clone();
            let mut title = Vec::new();
            if let Some(style) = metadata.style {
                metadata.style = None; // Clear style to avoid confusion
                metadata.attributes.insert("alt".to_string(), AttributeValue::String(style.clone()));
            }
            if metadata.positional_attributes.len() >= 2 {
                metadata.attributes.insert("height".to_string(), AttributeValue::String(metadata.positional_attributes.remove(1)));
            }
            if !metadata.positional_attributes.is_empty() {
                metadata.attributes.insert("width".to_string(), AttributeValue::String(metadata.positional_attributes.remove(0)));
            }
            metadata.move_positional_attributes_to_attributes();
            if let Some(AttributeValue::String(content)) = metadata.attributes.get("title") {
                if let Some((title_start, title_end)) = title_position {
                    // Use the captured position from the named_attribute rule
                    let title_start_pos = PositionWithOffset {
                        offset: title_start,
                        position: state.line_map.offset_to_position(title_start),
                    };
                    title = process_inlines(state, block_metadata, title_start, &title_start_pos, title_end, offset, content).map_err(|e| {
                        tracing::error!(?e, "could not process title in inline image macro");
                        "could not process title in inline image macro"
                    })?.0;
                }
                metadata.attributes.remove("title");
            }

            Ok(InlineNode::Macro(InlineMacro::Image(Box::new(Image {
                title,
                source,
                metadata: metadata.clone(),
                location: state.create_location(start.offset+offset, (end+offset).saturating_sub(1)),

            }))))
        }

        /// Parse link macros with custom attribute handling.
        ///
        /// Link macros have the format: `link:target[text,attr1=value1,attr2=value2]`
        ///
        /// ## Why Custom Parsing is Required
        ///
        /// Link attributes cannot use the generic `attributes()` rule because:
        ///
        /// 1. **Different Character Rules**: Link text can contain single quotes (`'`) and other
        ///    characters that are treated as delimiters in block attributes. For example:
        ///    - `link:file.adoc[see the 'quoted' text]` - single quotes are valid in link text
        ///
        /// 2. **Text vs Attributes**: The first element in link brackets is display text,
        ///    not an attribute. Block attributes expect all content to be attribute
        ///    definitions or block style.
        ///
        /// 3. **Delimiter Precedence**: In links, commas separate text from attributes, while in
        ///    block attributes, the first positional value is treated as a style/role.
        ///
        /// ## Parsing Strategy
        ///
        /// 1. **Try text + attributes**: `link_title()` followed by comma-separated attributes
        /// 2. **Fallback to attributes only**: If no valid title is found, parse as pure attributes
        ///
        /// The `link_title()` rule handles both quoted (`"text"`) and unquoted text, stopping at:
        /// - Commas (indicating start of attributes)
        /// - Closing brackets (end of link)
        /// - Attribute patterns (`name=value`)
        ///
        /// This approach isolates link parsing from block attribute parsing, preventing
        /// regressions in other parts of the parser while correctly handling edge cases
        /// like quotes, special characters, and mixed content.
        rule link_macro(offset: usize) -> InlineNode
        = start:position!() "link:" target:source() "[" content:(
            title:link_title() attributes:("," att:attribute() { att })* {
                (Some(title), attributes.into_iter().flatten().collect::<Vec<_>>())
            } /
            attributes:(att:attribute() comma()? { att })* {
                (None, attributes.into_iter().flatten().collect::<Vec<_>>())
            }
        ) "]" end:position!()
        {?
            tracing::info!(?target, ?content, "Found link macro inline");
            let (text, attributes) = content;
            let mut metadata = BlockMetadata::default();
            for (k, v, _pos) in attributes {
                if let AttributeValue::String(v) = v {
                    metadata.attributes.insert(k, AttributeValue::String(v));
                }
            }
            Ok(InlineNode::Macro(InlineMacro::Link(Link {
                text,
                target,
                attributes: metadata.attributes.clone(),
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            })))
        }

        /// Parse cross-reference shorthand syntax: <<id>> or <<id,custom text>>
        rule cross_reference_shorthand(offset: usize) -> InlineNode
        = start:position!() shorthand:cross_reference_shorthand_pattern() end:position!()
        {?
            let (target, text) = shorthand;
            let target_str = target.trim().to_string();
            let text = text.map(|t| t.trim().to_string());
            tracing::info!(?target_str, ?text, "Found cross-reference shorthand");
            Ok(InlineNode::Macro(InlineMacro::CrossReference(crate::model::CrossReference {
                target: target_str,
                text,
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            })))
        }

        /// Pattern for cross-reference shorthand: <<id>> or <<id,custom text>>
        rule cross_reference_shorthand_pattern() -> (String, Option<String>)
        = "<<" target:$(['a'..='z' | 'A'..='Z' | '_'] ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']*) content:("," text:$((!">>" [_])+) { text })? ">>"
        {
            (target.to_string(), content.map(std::string::ToString::to_string))
        }

        /// Parse cross-reference macro syntax: xref:id[text]
        rule cross_reference_macro(offset: usize) -> InlineNode
        = start:position!() "xref:" target:source() "[" text:$((!"]" [_])*) "]" end:position!()
        {?
            let target_str = target.to_string();
            let text_str = if text.is_empty() { None } else { Some(text.to_string()) };
            tracing::info!(?target_str, ?text_str, "Found cross-reference macro");
            Ok(InlineNode::Macro(InlineMacro::CrossReference(crate::model::CrossReference {
                target: target_str,
                text: text_str,
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            })))
        }

        /// Match cross-reference shorthand syntax without consuming: <<id>> or <<id,text>>
        rule cross_reference_shorthand_match() -> ()
        = cross_reference_shorthand_pattern()
        { }

        /// Match cross-reference macro syntax without consuming: xref:id[text]
        rule cross_reference_macro_match()
        = "xref:" source() "[" (!"]" [_])* "]"

        rule bold_text_unconstrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = start:position() "**" content_start:position() content:$((!(eol() / ![_] / "**") [_])+) "**" end:position!()
        {?
            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, "Found unconstrained bold text inline");
            let (content, location) = process_inlines(state, block_metadata, content_start.offset, &content_start, end - 2, offset, content).map_err(|e| {
                tracing::error!(?e, "could not process unconstrained bold text content");
                "could not process unconstrained bold text content"
            })?;
            Ok(InlineNode::BoldText(Bold {
                content,
                role: None, // TODO(nlopes): Handle roles (come from attributes list)
                form: Form::Unconstrained,
                location: state.create_location(start.offset + offset, (end + offset).saturating_sub(1)),
            }))
        }

        rule bold_text_constrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position!() content_start:position() "*" content:$([^('*' | ' ' | '\t' | '\n')] [^'*']*) "*"
          end:position!() &([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!'] / ![_])
        {?
            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + offset;
            let valid_boundary = absolute_pos == 0 || {
              let prev_char_pos = absolute_pos.saturating_sub(1);
              state.input.chars().nth(prev_char_pos).is_none_or(|c| {
                matches!(c, ' ' | '\t' | '\n' | '\r')
              })
            };

            if !valid_boundary {
                tracing::debug!(absolute_pos, prev_char = ?state.input.chars().nth(absolute_pos.saturating_sub(1)), "Invalid word boundary for constrained bold");
                return Err("invalid word boundary for constrained bold");
            }

            tracing::info!(?offset, ?content, "Found constrained bold text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let (content, _) = process_inlines(state, block_metadata, start + 1, &adjusted_content_start, end - 1, offset, content).map_err(|e| {
                tracing::error!(?e, "could not process constrained bold text content");
                "could not process constrained bold text content"
            })?;

            Ok(InlineNode::BoldText(Bold {
                content,
                role: None, // TODO(nlopes): Handle roles (come from attributes list)
                form: Form::Constrained,
                location: state.create_location(start + offset, (end + offset).saturating_sub(1)),
            }))
        }

        rule italic_text_constrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position!() content_start:position() "_" content:$([^('_' | ' ' | '\t' | '\n')] [^'_']*) "_"
          end:position!() &([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!'] / ![_])
        {?
            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + offset;
            let valid_boundary = absolute_pos == 0 || {
              let prev_char_pos = absolute_pos.saturating_sub(1);
              state.input.chars().nth(prev_char_pos).is_none_or(|c| {
                matches!(c, ' ' | '\t' | '\n' | '\r')
              })
            };

            if !valid_boundary {
                return Err("invalid word boundary for constrained italic");
            }

            tracing::info!(?offset, ?content, "Found constrained italic text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let (content, _) = process_inlines(state, block_metadata, start + 1, &adjusted_content_start, end - 1, offset, content).map_err(|e| {
                tracing::error!(?e, "could not process constrained italic text content");
                "could not process constrained italic text content"
            })?;
            Ok(InlineNode::ItalicText(Italic {
                content,
                role: None, // TODO(nlopes): Handle roles (come from attributes list)
                form: Form::Constrained,
                location: state.create_location(start + offset, (end + offset).saturating_sub(1)),
            }))
        }

        rule bold_text_constrained_match() -> ()
        = pos:position!() "*" [^('*' | ' ' | '\t' | '\n')] [^'*']* "*" ([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | '<' | '>'] / ![_])
        {?
            // Check if we're at start OR preceded by word boundary (no asterisk)
            let valid_boundary = pos == 0 || {
              let prev_char_pos = pos.saturating_sub(1);
              state.input.chars().nth(prev_char_pos).is_none_or(|c| {
                matches!(c, ' ' | '\t' | '\n' | '\r')
              })
            };

            if valid_boundary { Ok(()) } else { Err("invalid word boundary") }
        }

        rule italic_text_constrained_match() -> ()
        = pos:position!() "_" [^('_' | ' ' | '\t' | '\n')] [^'_']* "_" ([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | '<' | '>'] / ![_])
        {?
            // Check if we're at start OR preceded by word boundary (no underscore)
            let valid_boundary = pos == 0 || {
                let prev_char_pos = pos.saturating_sub(1);
                state.input.chars().nth(prev_char_pos).is_none_or(|c| {
                    matches!(c, ' ' | '\t' | '\n' | '\r')
                })
            };

            if valid_boundary { Ok(()) } else { Err("invalid word boundary") }
        }

        rule italic_text_unconstrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = start:position() "__" content_start:position() content:$((!(eol() / ![_] / "__") [_])+) "__" end:position!()
        {?
            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, "Found unconstrained italic text inline");
            let (content, location) = process_inlines(state, block_metadata, content_start.offset, &content_start, end - 2, offset, content).map_err(|e| {
                tracing::error!(?e, "could not process unconstrained italic text content");
                "could not process unconstrained italic text content"
            })?;
            Ok(InlineNode::ItalicText(Italic {
                content,
                role: None, // TODO(nlopes): Handle roles (come from attributes list)
                form: Form::Unconstrained,
                location: state.create_location(start.offset + offset, (end + offset).saturating_sub(1)),
            }))
        }

        rule monospace_text_unconstrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = start:position() "``" content_start:position() content:$((!(eol() / ![_] / "``") [_])+) "``" end:position!()
        {?
            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, "Found unconstrained monospace text inline");
            let (content, location) = process_inlines(state, block_metadata, content_start.offset, &content_start, end - 2, offset, content).map_err(|e| {
                tracing::error!(?e, "could not process unconstrained monospace text content");
                "could not process unconstrained monospace text content"
            })?;
            Ok(InlineNode::MonospaceText(Monospace {
                content,
                role: None, // TODO(nlopes): Handle roles (come from attributes list)
                form: Form::Unconstrained,
                location: state.create_location(start.offset + offset, (end + offset).saturating_sub(1)),
            }))
        }

        rule monospace_text_constrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position!() content_start:position() "`" content:$([^('`' | ' ' | '\t' | '\n')] [^'`']*) "`"
          end:position!() &([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!'] / ![_])
        {?
            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + offset;
            let valid_boundary = absolute_pos == 0 || {
              let prev_char_pos = absolute_pos.saturating_sub(1);
              state.input.chars().nth(prev_char_pos).is_none_or(|c| {
                matches!(c, ' ' | '\t' | '\n' | '\r')
              })
            };
            if !valid_boundary {
                return Err("monospace must be at word boundary");
            }
            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, "Found constrained monospace text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let (content, _) = process_inlines(state, block_metadata, start + 1, &adjusted_content_start, end - 1, offset, content).map_err(|e| {
                tracing::error!(?e, "could not process constrained monospace text content");
                "could not process constrained monospace text content"
            })?;
            Ok(InlineNode::MonospaceText(Monospace {
                content,
                role: None, // TODO(nlopes): Handle roles (come from attributes list)
                form: Form::Constrained,
                location: state.create_location(start + offset, (end + offset).saturating_sub(1)),
            }))
        }

        rule monospace_text_constrained_match() -> ()
        = pos:position!() "`" !(['\''] / ['"']) [^('`' | ' ' | '\t' | '\n')] [^'`']* "`" ([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!'] / ![_])
        {?
            // Check if we're at start OR preceded by word boundary (no backtick)
            let valid_boundary = pos == 0 || {
              let prev_char_pos = pos.saturating_sub(1);
              state.input.chars().nth(prev_char_pos).is_none_or(|c| {
                matches!(c, ' ' | '\t' | '\n' | '\r')
              })
            };

            if !valid_boundary {
                return Err("monospace must be at word boundary");
            }
            Ok(())
        }

        rule highlight_text_unconstrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = start:position() "##" content_start:position() content:$((!(eol() / ![_] / "##") [_])+) "##" end:position!()
        {?
            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, "Found unconstrained highlight text inline");
            let (content, location) = process_inlines(state, block_metadata, content_start.offset, &content_start, end - 2, offset, content).map_err(|e| {
                tracing::error!(?e, "could not process unconstrained highlight text content");
                "could not process unconstrained highlight text content"
            })?;
            Ok(InlineNode::HighlightText(Highlight {
                content,
                role: None, // TODO(nlopes): Handle roles (come from attributes list)
                form: Form::Unconstrained,
                location: state.create_location(start.offset + offset, (end + offset).saturating_sub(1)),
            }))
        }

        rule highlight_text_constrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position!() content_start:position() "#" content:$([^('#' | ' ' | '\t' | '\n')] [^'#']*) "#"
          end:position!() &([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!'] / ![_])
        {?
            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + offset;
            let prev_char_pos = absolute_pos.saturating_sub(1);
            let prev_char = state.input.chars().nth(prev_char_pos);
            let valid_boundary = absolute_pos == 0 || {
              prev_char.is_none_or(|c| {
                matches!(c, ' ' | '\t' | '\n' | '\r')
              })
            };
            if !valid_boundary {
                return Err("highlight must be at word boundary");
            }
            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, "Found constrained highlight text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let (content, _) = process_inlines(state, block_metadata, start + 1, &adjusted_content_start, end - 1, offset, content).map_err(|e| {
                tracing::error!(?e, "could not process constrained highlight text content");
                "could not process constrained highlight text content"
            })?;
            Ok(InlineNode::HighlightText(Highlight {
                content,
                role: None, // TODO(nlopes): Handle roles (come from attributes list)
                form: Form::Constrained,
                location: state.create_location(start + offset, (end + offset).saturating_sub(1)),
            }))
        }

        rule highlight_text_constrained_match() -> ()
        = pos:position!() "#" [^('#' | ' ' | '\t' | '\n')] [^'#']* "#" ([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!'] / ![_])
        {?
            // Check if we're at start OR preceded by word boundary (no hash)
            let valid_boundary = pos == 0 || {
              let prev_char_pos = pos.saturating_sub(1);
              state.input.chars().nth(prev_char_pos).is_none_or(|c| {
                matches!(c, ' ' | '\t' | '\n' | '\r')
              })
            };

            if !valid_boundary {
                return Err("highlight must be at word boundary");
            }
            Ok(())
        }

        /// Parse superscript text (^text^)
        rule superscript_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = start:position() "^" content_start:position() content:$([^('^' | ' ' | '\t' | '\n')]+) "^" end:position!()
        {?
            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, "Found superscript text inline");
            let (content, location) = process_inlines(state, block_metadata, content_start.offset, &content_start, end - 1, offset, content).map_err(|e| {
                tracing::error!(?e, "could not process superscript text content");
                "could not process superscript text content"
            })?;
            Ok(InlineNode::SuperscriptText(Superscript {
                content,
                role: None,
                form: Form::Unconstrained,
                location: state.create_location(start.offset + offset, (end + offset).saturating_sub(1)),
            }))
        }

        /// Parse subscript text (~text~)
        rule subscript_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = start:position() "~" content_start:position() content:$([^('~' | ' ' | '\t' | '\n')]+) "~" end:position!()
        {?
            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, "Found subscript text inline");
            let (content, location) = process_inlines(state, block_metadata, content_start.offset, &content_start, end - 1, offset, content).map_err(|e| {
                tracing::error!(?e, "could not process subscript text content");
                "could not process subscript text content"
            })?;
            Ok(InlineNode::SubscriptText(Subscript {
                content,
                role: None,
                form: Form::Unconstrained,
                location: state.create_location(start.offset + offset, (end + offset).saturating_sub(1)),
            }))
        }

        /// Parse curved quotation text (`"text"`)
        rule curved_quotation_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = start:position() "\"`" content_start:position() content:$((!("`\"") [_])+) "`\"" end:position!()
        {?
            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, "Found curved quotation text inline");
            let (content, location) = process_inlines(state, block_metadata, content_start.offset, &content_start, end - 2, offset, content).map_err(|e| {
                tracing::error!(?e, "could not process curved quotation text content");
                "could not process curved quotation text content"
            })?;
            Ok(InlineNode::CurvedQuotationText(CurvedQuotation {
                content,
                role: None,
                form: Form::Unconstrained,
                location: state.create_location(start.offset + offset, (end + offset).saturating_sub(1)),
            }))
        }

        /// Parse curved apostrophe text (`'text'`)
        rule curved_apostrophe_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = start:position() "'`" content_start:position() content:$((!("`'") [_])+) "`'" end:position!()
        {?
            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, "Found curved apostrophe text inline");
            let (content, location) = process_inlines(state, block_metadata, content_start.offset, &content_start, end - 2, offset, content).map_err(|e| {
                tracing::error!(?e, "could not process curved apostrophe text content");
                "could not process curved apostrophe text content"
            })?;
            Ok(InlineNode::CurvedApostropheText(CurvedApostrophe {
                content,
                role: None,
                form: Form::Unconstrained,
                location: state.create_location(start.offset + offset, (end + offset).saturating_sub(1)),
            }))
        }

        /// Parse standalone curved apostrophe (`')
        rule standalone_curved_apostrophe(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = start:position() "`'" end:position!()
        {?
            tracing::info!(?start, ?end, ?offset, "Found standalone curved apostrophe inline");
            Ok(InlineNode::StandaloneCurvedApostrophe(StandaloneCurvedApostrophe {
                location: state.create_location(start.offset + offset, (end + offset).saturating_sub(1)),
            }))
        }

        // TODO(nlopes): what do I do with the attributes here?
        rule plain_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start_pos:position!()
        attributes:attributes()?
        content:$((!(eol()*<2,> / ![_] / cross_reference_shorthand_match() / cross_reference_macro_match() / hard_wrap(offset) / footnote_match(offset, block_metadata) / inline_image(start_pos, block_metadata) / inline_icon(start_pos, block_metadata) / inline_keyboard(start_pos) / inline_button(start_pos) / inline_menu(start_pos) / url_macro(start_pos, block_metadata) / inline_pass(start_pos) / link_macro(start_pos) / inline_autolink(start_pos) / inline_line_break(start_pos) / bold_text_unconstrained(start_pos, block_metadata) / bold_text_constrained_match() / italic_text_unconstrained(start_pos, block_metadata) / italic_text_constrained_match() / monospace_text_unconstrained(start_pos, block_metadata) / monospace_text_constrained_match() / highlight_text_unconstrained(start_pos, block_metadata) / highlight_text_constrained_match() / superscript_text(start_pos, block_metadata) / subscript_text(start_pos, block_metadata) / curved_quotation_text(start_pos, block_metadata) / curved_apostrophe_text(start_pos, block_metadata) / standalone_curved_apostrophe(start_pos, block_metadata)) [_])+)
        end:position!()
        {
            tracing::info!(?content, "Found plain text inline");
            InlineNode::PlainText(Plain {
                content: content.to_string(),
                location: state.create_location(start_pos+offset, (end+offset).saturating_sub(1)),
            })
        }

        rule paragraph(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = admonition:admonition()?
        content_start:position()
        content:$((!(
            eol()*<2,>
            / eol()* ![_]
            / eol() example_delimiter()
            / eol() list(start, offset, block_metadata)
            / eol()* &(section_level_at_line_start(offset, None) (whitespace() / eol() / ![_]))
        ) [_])+)
        end:position!()
        {
            let (initial_location, location, processed) = preprocess_inline_content(state, start, &content_start, end, offset, content)?;
            if processed.text.starts_with(' ') {
                tracing::debug!(?processed, "preprocessed inline content starts with a space - switching to literal block");
                let mut metadata = block_metadata.metadata.clone();
                metadata.move_positional_attributes_to_attributes();
                metadata.style = Some("literal".to_string());
                return Ok(Block::Paragraph(Paragraph {
                    content: vec![InlineNode::PlainText(Plain {
                        content: processed.text,
                        location,
                    })],
                    metadata,
                    title: block_metadata.title.clone(),
                    location: initial_location,
                }));
            }
            let content = parse_inlines(&processed, state, block_metadata, &location)?;
            let content = map_inline_locations(state, &processed, &content, &location);

            // Title should either be an attribute named title, or the title parsed from the block metadata
            let title = if let Some(AttributeValue::String(title)) = block_metadata.metadata.attributes.get("title") {
                vec![InlineNode::PlainText(Plain {
                    content: title.clone(),
                    location: state.create_location(start+offset, (start+offset).saturating_add(title.len()).saturating_sub(1)),
                })]
            } else {
                block_metadata.title.clone()
            };

            if let Some(variant) = admonition {
                let Ok(variant) = AdmonitionVariant::from_str(&variant) else {
                    tracing::error!(%variant, "invalid admonition variant");
                    return Err(Error::InvalidAdmonitionVariant(variant) );
                };
                tracing::info!(%variant, "found admonition block with variant");
                Ok(Block::Admonition(Admonition{
                    metadata: block_metadata.metadata.clone(),
                    title: block_metadata.title.clone(),
                    blocks: vec![Block::Paragraph(Paragraph {
                        content,
                        metadata: block_metadata.metadata.clone(),
                        title: Vec::new(), // TODO(nlopes): Handle paragraph titles
                        location: state.create_location(content_start.offset+offset, end.saturating_sub(1)),
                    })],
                    location: state.create_location(offset, end.saturating_sub(1)),
                    variant,

                }))
            } else {
                tracing::info!(?content, ?location, "found paragraph block");
                Ok(Block::Paragraph(Paragraph {
                    content,
                    metadata: block_metadata.metadata.clone(),
                    title,
                    location: initial_location,
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
                start:position!() double_open_square_bracket() id:$([^'\'' | ',' | ']']+) comma() reftext:$([^']']+) double_close_square_bracket() eol() end:position!() {
                (start, id, Some(reftext), end)
            } /
            start:position!() double_open_square_bracket() id:$([^'\'' | ',' | ']']+) double_close_square_bracket() eol() end:position!() {
                (start, id, None, end)
            } /
            start:position!() open_square_bracket() "#" id:$([^'\'' | ',' | ']']+) comma() reftext:$([^']']+) close_square_bracket() eol() end:position!() {
                (start, id, Some(reftext), end)
            } /
            start:position!() open_square_bracket() "#" id:$([^'\'' | ',' | ']']+) close_square_bracket() eol() end:position!() {
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
                let (discrete, metadata, _title_position) = attributes;
                (discrete, metadata)
            }

        pub(crate) rule attributes() -> (bool, BlockMetadata, Option<(usize, usize)>)
            = start:position!() open_square_bracket() content:(
                // The case in which we keep the style empty
                attributes:(comma() att:attribute() { att })+ {
                    tracing::info!(?attributes, "Found empty style with attributes");
                    (true, false, None, attributes)
                } /
                // The case in which there is a block style and other attributes
                style:block_style() attributes:(comma() att:attribute() { att })+ {
                    tracing::info!(?style, ?attributes, "Found block style with attributes");
                    (false, true, Some(style), attributes)
                } /
                // The case in which there is a block style and no other attributes
                style:block_style() {
                    tracing::info!(?style, "Found block style");
                    (false, true, Some(style), vec![])
                } /
                // The case in which there are only attributes
                attributes:(att:attribute() comma()? { att })* {
                    tracing::info!(?attributes, "Found attributes");
                    (false, false, None, attributes)
                })
            close_square_bracket() end:position!() {
                let mut discrete = false;
                let mut style_found = false;
                let (empty, has_style, maybe_style, attributes) = content;
                let mut metadata = BlockMetadata::default();
                if let Some((maybe_style_name, id, roles, options)) = maybe_style {
                    if let Some(style_name) = maybe_style_name {
                        if style_name == "discrete" {
                            discrete = true;
                        } else if metadata.style.is_none() && has_style {
                            metadata.style = Some(style_name);
                            style_found = true;
                        } else {
                            metadata.attributes.insert(style_name, AttributeValue::None);
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
                let mut title_position = None;
                for (i, (k, v, pos)) in attributes.into_iter().flatten().enumerate() {
                    if k == RESERVED_NAMED_ATTRIBUTE_ID && metadata.id.is_none() {
                        let (id_start, id_end) = pos.unwrap_or((start, end));
                        metadata.id = Some(Anchor {
                            id: v.to_string(),
                            xreflabel: None,
                            location: state.create_location(id_start, id_end)
                        });
                    } else if (k == RESERVED_NAMED_ATTRIBUTE_ROLE || k == RESERVED_NAMED_ATTRIBUTE_OPTIONS) && let AttributeValue::String(v) = v {
                        // When the key is "role" or "options", we need to handle the case
                        // where the value is a quoted, comma-separated list of values.
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
                    } else if let AttributeValue::String(v) = v {
                        // We special case the "title" attribute to capture its position.
                        // An example where this is needed is in the inline image macro.
                        //
                        // I really don't like how this flows and one day I'll probably
                        // refactor this.
                        if k == "title" && let Some(title_pos) = pos {
                            title_position = Some(title_pos);
                        }
                        metadata.attributes.insert(k.to_string(), AttributeValue::String(v));
                    } else if v == AttributeValue::None && pos.is_none() {
                        metadata.positional_attributes.push(k);
                        tracing::warn!("Unexpected attribute value type: {:?}", v);
                    }
                }
                (discrete, metadata, title_position)
            }

        rule open_square_bracket() = "["
        rule close_square_bracket() = "]"
        rule double_open_square_bracket() = "[["
        rule double_close_square_bracket() = "]]"
        rule comma() = ","
        rule period() = "."
        rule empty_style() = ""
        rule role() -> &'input str = $([^(',' | ']' | '#' | '.' | '%')]+)

        // The option rule is used to parse options in the form of "option=value" or
        // "%option" (we don't capture the % here).
        //
        // The option can be a single word or a quoted string. If it is a quoted string,
        // it can contain commas, which we then look for and extract the options in the
        // `attributes()` rule.
        rule option() -> &'input str =
        $(("\"" [^('"' | ']' | '#' | '.' | '%')]+ "\"") / ([^('"' | ',' | ']' | '#' | '.' | '%')]+))

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
              / name:attribute_name() "=" start:position!() value:named_attribute_value() end:position!()
                { Some((name.to_string(), AttributeValue::String(value), Some((start, end)))) }

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
                style:positional_attribute_value() !"=" {
                    tracing::info!(%style, "Found block style without shorthands");
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
        = &"\"" inner:inner_attribute_value()
        {
            tracing::debug!(%inner, "Found named attribute value (inner)");
            inner.to_string()
        }
        / s:$([^(',' | '"' | ']')]+)
        {
            tracing::debug!(%s, "Found named attribute value");
            s.to_string()
        }

        rule positional_attribute_value() -> String
        = s:$([^('"' | ',' | ']' | '#' | '.' | '%')] [^(',' | ']' | '#' | '.' | '%' | '=')]*)
        {
            tracing::debug!(%s, "Found positional attribute value");
            s.to_string()
        }

        rule inner_attribute_value() -> String
        = s:$("\"" [^('"' | ']')]* "\"") { s.to_string() }

        pub rule url() -> String = proto:proto() "://" path:path() { format!("{}{}{}", proto, "://", path) }

        rule proto() -> &'input str = $("https" / "http" / "ftp" / "irc" / "mailto")

        pub rule path() -> String = path:$(['A'..='Z' | '{' | '}' | 'a'..='z' | '0'..='9' | '_' | '-' | '.' | '/' | '~' | '?' | '&' | '=' ]+)
        {?
            let mut inline_state = InlinePreprocessorParserState::new();
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|e| {
                tracing::error!(?e, "could not preprocess path");
                "could not preprocess path"
            })?;
            Ok(processed.text)
        }

        pub rule source() -> Source
            = source:
        (
            u:url() { Source::Url(u.to_string()) }
            / p:path() { Source::Path(p.to_string()) }
        )
        { source }

        rule digits() = ['0'..='9']+

        rule eol() = quiet!{ "\n" }

        rule newline_or_comment() = quiet!{ eol() / (comment() (eol() / ![_])) }

        rule comment() = quiet!{ "//" [^'\n']+ (&eol() / ![_]) }

        rule document_attribute_key_match() -> (bool, &'input str)
        = ":"
        key:(
            "!" key:$([^':']+) { (true, key) }
            / key:$([^('!' | ':')]+) "!" { (true, key) }
            / key:$([^':']+) { (false, key) }
        )
        ":" &" "?
        {
            key
        }
        / expected!("document attribute key starting with ':'")

        rule document_attribute_match() -> (&'input str, AttributeValue)
        = key:document_attribute_key_match() maybe_value:(" " value:$(!(&((eol() document_attribute_key_match()) / eol()*<2,> / ![_])) [_])+ { value })?
        {
            let (unset, key) = key;
            if unset {
                // e.g: :!background: or :background!:
                (key, AttributeValue::Bool(false))
            } else if let Some(value) = maybe_value {
                let value = value.join("");
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

        rule position() -> PositionWithOffset = offset:position!() {
            PositionWithOffset {
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
    fn test_document() -> Result<(), Error> {
        let input = "// this comment line is ignored
= Document Title
Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>
v2.9, 01-09-2024: Fall incarnation
:description: The document's description.
:sectanchors:
:url-repo: https://my-git-repo.com";
        let mut state = ParserState::new(input);
        let result = document_parser::document(input, &mut state)??;
        let header = result.header.expect("document has a header");
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
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_authors() -> Result<(), Error> {
        let input =
            "Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>";
        let mut state = ParserState::new(input);
        let result = document_parser::authors(input, &mut state)?;

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
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_author() -> Result<(), Error> {
        let input = "Norberto M. Lopes supa dough <nlopesml@gmail.com>";
        let mut state = ParserState::new(input);
        let result = document_parser::author(input, &mut state)?;
        assert_eq!(result.first_name, "Norberto");
        assert_eq!(result.middle_name, Some("M.".to_string()));
        assert_eq!(result.last_name, "Lopes supa dough");
        assert_eq!(result.initials, "NML");
        assert_eq!(result.email, Some("nlopesml@gmail.com".to_string()));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_revision_full() -> Result<(), Error> {
        let input = "v2.9, 01-09-2024: Fall incarnation";
        let mut state = ParserState::new(input);
        document_parser::revision(input, &mut state)?;
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
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_revision_with_date_no_remark() -> Result<(), Error> {
        let input = "v2.9, 01-09-2024";
        let mut state = ParserState::new(input);
        document_parser::revision(input, &mut state)?;
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("v2.9".to_string()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("01-09-2024".to_string()))
        );
        assert_eq!(state.document_attributes.get("revremark"), None);
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_revision_no_date_with_remark() -> Result<(), Error> {
        let input = "v2.9: Fall incarnation";
        let mut state = ParserState::new(input);
        document_parser::revision(input, &mut state)?;
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("v2.9".to_string()))
        );
        assert_eq!(state.document_attributes.get("revdate"), None);
        assert_eq!(
            state.document_attributes.get("revremark"),
            Some(&AttributeValue::String("Fall incarnation".to_string()))
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_revision_no_date_no_remark() -> Result<(), Error> {
        let input = "v2.9";
        let mut state = ParserState::new(input);
        document_parser::revision(input, &mut state)?;
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("v2.9".to_string()))
        );
        assert_eq!(state.document_attributes.get("revdate"), None);
        assert_eq!(state.document_attributes.get("revremark"), None);
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_title() -> Result<(), Error> {
        let input = "= Document Title";
        let mut state = ParserState::new(input);
        let result = document_parser::document_title(input, &mut state)?;
        assert_eq!(result.0.len(), 1);
        assert_eq!(
            result.0[0],
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
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_title_and_subtitle() -> Result<(), Error> {
        let input = "= Document Title: And a subtitle";
        let mut state = ParserState::new(input);
        let result = document_parser::document_title(input, &mut state)?;
        assert_eq!(
            result,
            (
                vec![InlineNode::PlainText(Plain {
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
                })],
                Some(vec![InlineNode::PlainText(Plain {
                    content: "And a subtitle".to_string(),
                    location: Location {
                        absolute_start: 17,
                        absolute_end: 31,
                        start: crate::Position {
                            line: 1,
                            column: 18
                        },
                        end: crate::Position {
                            line: 1,
                            column: 32
                        },
                    }
                })])
            )
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_header_with_title_and_authors() -> Result<(), Error> {
        let input = "= Document Title
Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>";
        let mut state = ParserState::new(input);
        let result = document_parser::header(input, &mut state)?.expect("header should be present");
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
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_empty_attribute_list() -> Result<(), Error> {
        let input = "[]";
        let mut state = ParserState::new(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete); // Not discrete
        assert_eq!(metadata.id, None);
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.is_empty());
        assert!(metadata.options.is_empty());
        assert!(metadata.attributes.is_empty());
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_empty_attribute_list_with_discrete() -> Result<(), Error> {
        let input = "[discrete]";
        let mut state = ParserState::new(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(discrete); // Should be discrete
        assert_eq!(metadata.id, None);
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.is_empty());
        assert!(metadata.options.is_empty());
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id() -> Result<(), Error> {
        let input = "[id=my-id,role=admin,options=read,options=write]";
        let mut state = ParserState::new(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete); // Not discrete
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
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id_mixed() -> Result<(), Error> {
        let input = "[astyle#myid.admin,options=read,options=write]";
        let mut state = ParserState::new(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete); // Not discrete
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
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id_mixed_with_quotes() -> Result<(), Error> {
        let input = "[astyle#myid.admin,options=\"read,write\"]";
        let mut state = ParserState::new(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete); // Not discrete
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
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_toc_simple() -> Result<(), Error> {
        let input =
            "= Document Title\n\n== Section 1\n\nSome content.\n\n== Section 2\n\nMore content.";
        let mut state = ParserState::new(input);
        let result = document_parser::document(input, &mut state)??;

        // Check that TOC entries were generated
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
        let mut state = ParserState::new(input);
        let result = document_parser::document(input, &mut state)??;

        // Check that TOC entries were generated and ordered correctly
        assert_eq!(result.toc_entries.len(), 3);
        assert_eq!(result.toc_entries[0].id, "_section_a");
        assert_eq!(result.toc_entries[1].id, "_section_a_1");
        assert_eq!(result.toc_entries[2].id, "_section_b");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_toc_empty_document() -> Result<(), Error> {
        let input = "= Document Title\n\nJust some content without sections.";
        let mut state = ParserState::new(input);
        let result = document_parser::document(input, &mut state)??;
        assert_eq!(result.toc_entries.len(), 0);
        Ok(())
    }
}
