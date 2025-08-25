use crate::{
    error::Detail,
    grammar::LineMap,
    inline_preprocessing,
    model::{ListLevel, SectionLevel},
    Admonition, AdmonitionVariant, Anchor, AttributeValue, Audio, Author, Block, BlockMetadata,
    Bold, DelimitedBlock, DelimitedBlockType, DiscreteHeader, Document, DocumentAttribute,
    DocumentAttributes, Error, Header, Image, InlineMacro, InlineNode,
    InlinePreprocessorParserState, Italic, LineBreak, Link, ListItem, ListItemCheckedStatus,
    Location, Options, OrderedList, PageBreak, Paragraph, Plain, ProcessedContent, Raw, Section,
    Source, Table, TableColumn, TableOfContents, TableRow, ThematicBreak, UnorderedList, Video,
};

#[derive(Debug)]
pub(crate) struct ParserState {
    pub(crate) document_attributes: DocumentAttributes,
    pub(crate) line_map: LineMap,
    pub(crate) options: Options,
}

impl ParserState {
    pub(crate) fn new(input: &str) -> Self {
        Self {
            options: Options::default(),
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
    Title(Vec<InlineNode>),
}

#[derive(Debug, Default)]
// Used purely in the grammar to represent the parsed block details
struct BlockParsingMetadata {
    discrete: bool,
    metadata: BlockMetadata,
    title: Vec<InlineNode>,
    parent_section_level: Option<SectionLevel>,
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

pub(crate) fn parse_table_cell(
    content: &str,
    state: &mut ParserState,
    cell_start_offset: usize,
    parent_section_level: Option<SectionLevel>,
) -> TableColumn {
    let content = document_parser::blocks(content, state, cell_start_offset, parent_section_level)
        .expect("valid blocks inside table cell")
        .unwrap_or_else(|_e| {
            //TODO(nlopes): tracing::error!(e, "Error parsing table cell content as blocks");
            Vec::new()
        });
    TableColumn { content }
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

#[tracing::instrument(skip_all, fields(?state, start, ?content_start, end, offset, ?content))]
fn preprocess_inline_content(
    state: &ParserState,
    start: usize,
    content_start: &Position,
    end: usize,
    offset: usize,
    content: &str,
) -> Result<(Location, Location, ProcessedContent), Error> {
    // Create initial location for the entire content before inline processing
    let initial_location = state.create_location(start + offset, (end + offset).saturating_sub(1));
    // parse the inline content - this needs to be handed over to the inline preprocessing
    let mut inline_state = InlinePreprocessorParserState::new();

    // We adjust the start and end positions to account for the content start offset
    let location = state.create_location(
        content_start.offset + offset,
        (end + offset).saturating_sub(1),
    );
    inline_state.set_initial_position(&location, content_start.offset + offset);
    tracing::info!(
        ?inline_state,
        ?location,
        ?offset,
        ?content_start,
        ?end,
        "before inline preprocessing run"
    );

    let processed = inline_preprocessing::run(content, &state.document_attributes, &inline_state)?;
    Ok((initial_location, location, processed))
}

#[tracing::instrument(skip_all, fields(processed=?processed, block_metadata=?block_metadata))]
fn parse_inlines(
    processed: &ProcessedContent,
    block_metadata: &BlockParsingMetadata,
) -> Result<Vec<InlineNode>, Error> {
    let mut inline_peg_state = ParserState::new(&processed.text);
    Ok(document_parser::inlines(
        &processed.text,
        &mut inline_peg_state,
        0,
        block_metadata,
    )?)
}

/// Location mapping coordinate transformations during inline processing.
///
/// # Location Mapping Overview
///
/// The inline parser operates on preprocessed text that may have undergone attribute
/// substitutions and other transformations. This creates a complex coordinate mapping problem:
///
/// 1. **Original document coordinates**: Character positions in the raw AsciiDoc source
/// 2. **Preprocessed coordinates**: Character positions after attribute substitution/processing
/// 3. **Parsed inline coordinates**: Relative positions within the preprocessed content
///
/// ## Coordinate Transformation Pipeline
///
/// ```
/// Original:      "{greeting} _world_!"
/// Preprocessed:  "hello _world_!"
/// Parsed inline: ["hello ", ItalicText("world"), "!"]
/// ```
///
/// The mapping process:
/// 1. Take parsed inline locations (relative to preprocessed text)
/// 2. Convert to preprocessed absolute coordinates
/// 3. Use source map to find original document coordinates
/// 4. Convert to human-readable line/column positions
///
/// ## Special Cases
///
/// **Attribute Substitutions**: When `{greeting}` becomes `hello`, the location mapping
/// may collapse to a single point. We detect this and expand the location to cover the
/// full original attribute span for better error reporting and IDE support.
///
/// **Nested Content**: Formatted text like `**{greeting}**` requires mapping both the
/// outer formatting markers and inner content locations correctly.
/// Map a single location from preprocessed coordinates to original document coordinates.
///
/// This is the core coordinate transformation that:
/// 1. Converts preprocessed-relative offsets to document-absolute offsets
/// 2. Uses the preprocessor source map to find original positions
/// 3. Computes human-readable line/column positions
fn create_location_mapper<'a>(
    state: &'a ParserState,
    processed: &'a ProcessedContent,
    base_location: &'a Location,
) -> Box<dyn Fn(&Location) -> Location + 'a> {
    Box::new(move |loc: &Location| -> Location {
        tracing::info!(?base_location, ?loc, "mapping inline location");

        // Convert processed-relative absolute offsets into document-absolute offsets
        let processed_abs_start = base_location.absolute_start + loc.absolute_start;
        let processed_abs_end = base_location.absolute_start + loc.absolute_end;

        // Map those through the preprocessor source map back to original source
        let mapped_abs_start = processed.source_map.map_position(processed_abs_start);
        let mapped_abs_end = processed.source_map.map_position(processed_abs_end);

        // Compute human positions from the document's line map
        let start_pos = state.line_map.offset_to_position(mapped_abs_start);
        let end_pos = state.line_map.offset_to_position(mapped_abs_end);

        Location {
            absolute_start: mapped_abs_start,
            absolute_end: mapped_abs_end,
            start: start_pos,
            end: end_pos,
        }
    })
}

/// Apply attribute substitution location extension if needed.
///
/// When attribute substitutions collapse locations to a single point (e.g., `{attr}` -> `value`),
/// we need to extend the location back to cover the original attribute span for better UX.
fn extend_attribute_location_if_needed(
    state: &ParserState,
    processed: &ProcessedContent,
    mut location: Location,
) -> Location {
    // Check if location is collapsed and we have attribute replacements to consider
    if location.absolute_start == location.absolute_end
        && !processed.source_map.replacements.is_empty()
    {
        // Find the attribute replacement that contains this collapsed location
        if let Some(attr_replacement) = processed.source_map.replacements.iter().find(|rep| {
            rep.kind == crate::grammar::inline_preprocessor::ProcessedKind::Attribute
                && location.absolute_start >= rep.absolute_start
                && location.absolute_start < rep.processed_end
        }) {
            tracing::debug!(from=?location, to=?attr_replacement,
                "Extending collapsed location to full attribute span",
            );

            // Extend location to cover the full original attribute
            let start_pos = state
                .line_map
                .offset_to_position(attr_replacement.absolute_start);
            let end_pos = state
                .line_map
                .offset_to_position(attr_replacement.absolute_end);
            location = Location {
                absolute_start: attr_replacement.absolute_start,
                absolute_end: attr_replacement.absolute_end,
                start: start_pos,
                end: end_pos,
            };
        }
    }
    location
}

/// Map locations for inner content within formatted text (bold, italic, etc.).
///
/// This handles the complex case where formatted text contains nested content that may
/// include attribute substitutions requiring location extension.
fn map_inner_content_locations(
    content: Vec<InlineNode>,
    map_loc: &dyn Fn(&Location) -> Location,
    state: &ParserState,
    processed: &ProcessedContent,
) -> Vec<InlineNode> {
    content
        .into_iter()
        .map(|node| match node {
            InlineNode::PlainText(mut inner_plain) => {
                // Map to document coordinates first
                let mapped = map_loc(&inner_plain.location);
                // Apply attribute location extension if needed
                inner_plain.location =
                    extend_attribute_location_if_needed(state, processed, mapped);
                InlineNode::PlainText(inner_plain)
            }
            other => other,
        })
        .collect()
}

/// Map locations for formatted text nodes (bold, italic, etc.).
///
/// This handles both the outer formatting location and any inner content locations,
/// with special handling for attribute substitutions.
fn map_formatted_text_locations<T>(
    mut formatted_text: T,
    map_loc: &dyn Fn(&Location) -> Location,
    state: &ParserState,
    processed: &ProcessedContent,
    get_location: impl Fn(&T) -> &Location,
    set_location: impl Fn(&mut T, Location),
    get_content: impl Fn(&T) -> &Vec<InlineNode>,
    set_content: impl Fn(&mut T, Vec<InlineNode>),
) -> T {
    // Map outer location with attribute extension
    let mapped_outer = map_loc(get_location(&formatted_text));
    let extended_location = extend_attribute_location_if_needed(state, processed, mapped_outer);
    set_location(&mut formatted_text, extended_location);

    // Map inner content locations
    let mapped_content = map_inner_content_locations(
        get_content(&formatted_text).clone(),
        map_loc,
        state,
        processed,
    );
    set_content(&mut formatted_text, mapped_content);

    formatted_text
}

/// Map inline node locations from preprocessed coordinates to original document coordinates.
///
/// This is the main entry point for location mapping during inline processing. It handles
/// the complex coordinate transformations needed to map parsed inline content back to
/// original document positions while accounting for preprocessing changes like attribute
/// substitutions.
///
/// See the module-level documentation for a detailed explanation of the coordinate
/// transformation pipeline and special cases.
#[tracing::instrument(skip_all, fields(location=?location, processed=?processed, content=?content))]
fn map_inline_locations(
    state: &ParserState,
    processed: &ProcessedContent,
    content: &Vec<InlineNode>,
    location: &Location,
) -> Vec<InlineNode> {
    tracing::info!(?location, "mapping inline locations");

    let map_loc = create_location_mapper(state, processed, location);

    content
        .into_iter()
        .map(|inline| match inline {
            InlineNode::PlainText(plain) => InlineNode::PlainText(Plain {
                content: plain.content.clone(),
                location: map_loc(&plain.location),
            }),
            InlineNode::ItalicText(italic_text) => {
                let mapped = map_formatted_text_locations(
                    italic_text.clone(),
                    map_loc.as_ref(),
                    state,
                    processed,
                    |t| &t.location,
                    |t, loc| t.location = loc,
                    |t| &t.content,
                    |t, content| t.content = content,
                );
                InlineNode::ItalicText(mapped)
            }
            InlineNode::BoldText(bold_text) => {
                let mapped = map_formatted_text_locations(
                    bold_text.clone(),
                    map_loc.as_ref(),
                    state,
                    processed,
                    |t| &t.location,
                    |t, loc| t.location = loc,
                    |t| &t.content,
                    |t, content| t.content = content,
                );
                InlineNode::BoldText(mapped)
            }
            other => other.clone(),
        })
        .collect::<Vec<_>>()
}

/// Process inlines
#[tracing::instrument(skip_all, fields(?start, ?content_start, end, offset))]
fn process_inlines(
    state: &ParserState,
    block_metadata: &BlockParsingMetadata,
    start: usize,
    content_start: &Position,
    end: usize,
    offset: usize,
    content: &str,
) -> Result<(Vec<InlineNode>, Location), Error> {
    // Preprocess the inline content first
    let (initial_location, location, processed) =
        preprocess_inline_content(state, start, content_start, end, offset, content)?;
    let content = parse_inlines(&processed, block_metadata)?;
    let content = map_inline_locations(state, &processed, &content, &location);
    Ok((content, initial_location))
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
                // For documents that end with text content (like body-only), adjust the end position
                let document_end_offset = if blocks.is_empty() {
                    end.offset.saturating_sub(1)
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
                })
            }

        pub(crate) rule header() -> Option<Header>
            = start:position!()
            ((document_attribute() / comment()) (eol() / ![_]))*
            title_authors:(title_authors:title_authors() { title_authors })?
            (eol() (document_attribute() / comment()))*
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
        = blocks:(block(offset, parent_section_level) ** (eol()*<2,> / ![_]))
        {
            blocks.into_iter().map(|b| b).collect::<Result<Vec<_>, Error>>()
        }


        pub(crate) rule block(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block, Error>
        = eol()* block:(
            document_attribute_block(offset) /
            &"[discrete" dh:discrete_header(offset) { dh } /
            !same_or_higher_level_section(offset, parent_section_level) section:section(offset, parent_section_level) { section } /
            block_generic(offset, parent_section_level)
        )
        {
            Ok(block?)
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
        = start:position!() block_metadata:block_metadata(offset, None)
        section_level:section_level(offset, None) whitespace()
        title_start:position!() title:section_title(start, offset, &block_metadata) title_end:position!() end:position!() &eol()*<2,2>
        {
            tracing::info!(?block_metadata, ?title, ?title_start, ?title_end, "parsing discrete header block");

            let level = section_level.1;
            let location = state.create_location(start+offset, (end+offset).saturating_sub(1));

            Ok(Block::DiscreteHeader(DiscreteHeader {
                anchors: block_metadata.metadata.anchors,
                title,
                level,
                location,
            }))
        }

        pub(crate) rule document_attribute_block(offset: usize) -> Result<Block, Error>
        = start:position!() att:document_attribute_match() end:position!()
        {
            let (key, value) = att;
            Ok(Block::DocumentAttribute(DocumentAttribute {
                name: key.to_string(),
                value,
                location: state.create_location(start+offset, end+offset)
            }))
        }

        pub(crate) rule section(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block, Error>
        = start:position!() block_metadata:block_metadata(offset, parent_section_level)
        section_level_start:position!()
        section_level:section_level(offset, parent_section_level)
        section_level_end:position!()
        whitespace()
        title_start:position!() title:section_title(start, offset, &block_metadata) title_end:position!() &eol()*<2,2>
        content:section_content(offset, Some(section_level.1+1))? end:position!()
        {
            tracing::info!(?offset, ?block_metadata, ?title, ?title_start, ?title_end, "parsing section block");

            // Validate section level against parent section level if any is provided
            if let Some(parent_level) = parent_section_level {
                if section_level.1+1 <= parent_level {
                    return Err(Error::NestedSectionLevelMismatch(
                        Detail { location: state.create_location(section_level_start + offset, (section_level_end + offset).saturating_sub(1)) },
                        section_level.1+1,
                        parent_level + 1,
                    ));
                } else if section_level.1+1 > parent_level + 1 {
                    return Err(Error::NestedSectionLevelMismatch(
                        Detail { location: state.create_location(section_level_start + offset, (section_level_end + offset).saturating_sub(1)) },
                        section_level.1+1,
                        parent_level + 1,
                    ));
                } else if section_level.1+1 > 6 {
                    // Maximum section level is 6 - this should be a different error entirely
                    return Err(Error::NestedSectionLevelMismatch(
                        Detail { location: state.create_location(section_level_start + offset, (section_level_end + offset).saturating_sub(1)) },
                        section_level.1+1,
                        parent_level + 1,
                    ));
                }
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

        rule block_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> BlockParsingMetadata
        = lines:(
            anchor:anchor() { BlockMetadataLine::Anchor(anchor) }
            / attr:attributes_line() { BlockMetadataLine::Attributes(attr) }
            / title:title_line(offset) { BlockMetadataLine::Title(title) }
        )*
        {
            let mut metadata = BlockMetadata::default();
            let mut discrete = false;
            let mut title = Vec::new();

            for value in lines {
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
            BlockParsingMetadata {
                discrete,
                metadata,
                title,
                parent_section_level,
            }
        }

        // A title line can be a simple title or a section title
        //
        // A title line is a line that starts with a period (.) followed by a non-whitespace character
        rule title_line(offset: usize) -> Vec<InlineNode>
        = period() start:position() &(!whitespace()) title:$([^'\n']*) end:position!() eol()
        {?
            tracing::info!(?title, ?start, ?end, "Found title line in block metadata");
            let block_metadata = BlockParsingMetadata::default();
            let (title, _) = process_inlines(&state, &block_metadata, start.offset, &start, end, offset, &title).unwrap();
            Ok(title)
        }

        rule section_level(offset: usize, parent_section_level: Option<SectionLevel>) -> (&'input str, SectionLevel)
        = start:position() level:$(("=" / "#")*<1,6>) end:position!()
        {
            (level, level.len().try_into().unwrap_or(1)-1)
        }

        rule section_title(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Vec<InlineNode>
        = title_start:position() title:$([^'\n']*) end:position!()
        {?
            tracing::info!(?title, ?title_start, start, ?end, offset, "Found section title");
            let (content, _) = process_inlines(&state, block_metadata, start, &title_start, end, offset, title).unwrap();
            Ok(content)
        }

        rule section_content(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Vec<Block>, Error>
        = blocks(offset, parent_section_level)

        pub(crate) rule block_generic(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block, Error>
        = start:position!() block_metadata:block_metadata(offset, parent_section_level) block:(
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
                tracing::info!(?block_metadata, ?block, "parsing generic block");
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
                    tracing::error!("Error parsing example content as blocks: {}", e);
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

        pub(crate) rule literal_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = open_delim:literal_delimiter() eol()
        content_start:position!() content:until_literal_delimiter() content_end:position!()
        eol() close_delim:literal_delimiter() end:position!()
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
                    tracing::error!("Error parsing sidebar content as blocks: {}", e);
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
                .collect::<Vec<_>>();
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

        rule image(start: usize, offset: usize, _block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = "image::" source:source() attributes:attributes() end:position!()
        {
            let (_discrete, metadata) = attributes;
            let mut metadata = metadata.clone();
            if let Some(style) = metadata.style {
                metadata.style = None; // Clear style to avoid confusion
                metadata.attributes.insert("alt".to_string(), AttributeValue::String(style.clone()));
            }
            if metadata.positional_attributes.len() >= 2 {
                metadata.attributes.insert("height".to_string(), AttributeValue::String(metadata.positional_attributes.remove(1)));
            }
            if metadata.positional_attributes.len() >= 1 {
                metadata.attributes.insert("width".to_string(), AttributeValue::String(metadata.positional_attributes.remove(0)));
            }
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Image(Image {
                title: Vec::new(), // TODO(nlopes): Handle image titles
                source,
                metadata: metadata.clone(),
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),

            }))
        }

        rule audio(start: usize, offset: usize, _block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = "audio::" source:source() attributes:attributes() end:position!()
        {
            let (_discrete, metadata) = attributes;
            let mut metadata = metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Audio(Audio {
                title: Vec::new(), // TODO(nlopes): Handle audio titles
                source,
                metadata,
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            }))
        }

        // The video block is similar to the audio and image blocks, but it supports
        // multiple sources. This is for example to allow passing multiple youtube video
        // ids to form a playlist.
        rule video(start: usize, offset: usize, _block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = "video::" sources:(source() ** comma()) attributes:attributes() end:position!()
        {
            let (_discrete, metadata) = attributes;
            let mut metadata = metadata.clone();
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
            if metadata.positional_attributes.len() >= 1 {
                metadata.attributes.insert("width".to_string(), AttributeValue::String(metadata.positional_attributes.remove(0)));
            }
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Video(Video {
                title: Vec::new(), // TODO(nlopes): Handle video titles
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
        = unordered_list(start, offset, block_metadata) / ordered_list(start, offset, block_metadata)

        rule unordered_list_marker() -> &'input str = $("*"+ / "-")

        rule ordered_list_marker() -> &'input str = $(digits()? "."+)

        rule unordered_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = &(unordered_list_marker() whitespace()) content:list_item(offset, block_metadata)+ end:position!()
        {
            tracing::info!(?content, "Found unordered list block");
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

        rule list_item(offset: usize, block_metadata: &BlockParsingMetadata) -> (ListItem, usize)
        = start:position!()
        marker:(unordered_list_marker() / ordered_list_marker())
        whitespace()
        checked:checklist_item()?
        list_content_start:position()
        list_item:$((!(&(eol()+ (unordered_list_marker() / ordered_list_marker())) / ![_]) [_])+)
        end:position!() (eol()+ / ![_])
        {?
            tracing::info!(%list_item, %marker, ?checked, "found list item");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1)).map_err(|_| "could not parse depth from marker")?;
            let (content, _) = process_inlines(&state, block_metadata, start, &list_content_start, end, offset, list_item).unwrap();
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

        pub(crate) rule inlines(offset: usize, block_metadata: &BlockParsingMetadata) -> Vec<InlineNode>
        = inlines:(
            non_plain_text(offset, block_metadata)
            / plain_text(offset, block_metadata))+
        {
            tracing::info!(?offset, ?inlines, "Found inlines");
            inlines
        }

        rule non_plain_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = inline:(
            hard_wrap:hard_wrap(offset) { hard_wrap }
            / image:inline_image(offset, block_metadata) { image }
            /*
            icon_inline |
            keyboard_inline |
            btn_inline |
            menu_inline |
            url_macro |
            link_macro |
            autolink |
            pass_inline |
            placeholder |
             */
            / link_macro:link_macro(offset) { link_macro }
            / inline_line_break:inline_line_break(offset) { inline_line_break }
            / bold_text_unconstrained:bold_text_unconstrained(offset, block_metadata) { bold_text_unconstrained }
            / italic_text_unconstrained:italic_text_unconstrained(offset, block_metadata) { italic_text_unconstrained }
            ) {
                inline
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

        rule inline_image(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position() "image:" source:source() attributes:attributes() end:position!()
        {
            let (_discrete, metadata) = attributes;
            let mut metadata = metadata.clone();
            let mut title = Vec::new();
            if let Some(style) = metadata.style {
                metadata.style = None; // Clear style to avoid confusion
                metadata.attributes.insert("alt".to_string(), AttributeValue::String(style.clone()));
            }
            if metadata.positional_attributes.len() >= 2 {
                metadata.attributes.insert("height".to_string(), AttributeValue::String(metadata.positional_attributes.remove(1)));
            }
            if metadata.positional_attributes.len() >= 1 {
                metadata.attributes.insert("width".to_string(), AttributeValue::String(metadata.positional_attributes.remove(0)));
            }
            metadata.move_positional_attributes_to_attributes();
            if let Some(AttributeValue::String(content)) = metadata.attributes.get("title") {
                dbg!(&content);
                title = process_inlines(&state, block_metadata, start.offset, &start, end, offset, content).unwrap().0;
                dbg!(&title);
                //metadata.attributes.remove("title");
            }

            InlineNode::Macro(InlineMacro::Image(Box::new(Image {
                title,
                source,
                metadata: metadata.clone(),
                location: state.create_location(start.offset+offset, (end+offset).saturating_sub(1)),

            })))
        }

        rule link_macro(offset: usize) -> InlineNode
        = start:position!() "link:" target:source()
        // "["
        // content:(
        //     title:link_title() attributes:("," ++ attribute()) { (Some(title), attributes) }
        //     / title:link_title() { (Some(title), vec![]) }
        //     / attributes:(attribute() ** comma()) { (None, attributes) }
        // )
        // "]"
        attributes:attributes()
        end:position!()
        {?
            tracing::info!(?target, ?attributes, "Found link macro inline");
            let (_discrete, metadata) = attributes;
            let mut metadata = metadata.clone();
            let text = metadata.style.clone();
            metadata.style = None; // Clear style to avoid confusion
            Ok(InlineNode::Macro(InlineMacro::Link(Link {
                text,
                target,
                attributes: metadata.attributes.clone(),
                location: state.create_location(start+offset, (end+offset).saturating_sub(1)),
            })))
        }

        rule bold_text_unconstrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = start:position() "**" content:$((!(eol() / ![_] / "**") [_])+) "**" end:position!()
        {?
            tracing::info!(?start, ?end, ?offset, ?content, "Found unconstrained bold text inline");
            let (content, location) = process_inlines(&state, block_metadata, start.offset, &start, end, offset, content).unwrap();
            Ok(InlineNode::BoldText(Bold {
                content,
                role: None, // TODO(nlopes): Handle roles (come from attributes list)
                location,
            }))
        }

        rule italic_text_unconstrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = start:position() "__" content:$((!(eol() / ![_] / "__") [_])+) "__" end:position!()
        {?
            tracing::info!(?offset, ?content, "Found unconstrained italic text inline");
            let (content, location) = process_inlines(&state, block_metadata, start.offset, &start, end, offset, content).unwrap();
            Ok(InlineNode::ItalicText(Italic {
                content,
                role: None, // TODO(nlopes): Handle roles (come from attributes list)
                location,
            }))
        }

        rule plain_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start_pos:position!()
        attributes:attributes()?
        content:$((!(eol()*<2,> / ![_] / hard_wrap(offset) / non_plain_text(offset, block_metadata) / bold_text_unconstrained(start_pos, block_metadata) / italic_text_unconstrained(start_pos, block_metadata)) [_])+)
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
            / eol()* &(section_level(offset, None) (whitespace() / eol() / ![_]))
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
                    title: Vec::new(), // TODO(nlopes): Handle paragraph titles
                    location: initial_location,
                }));
            }
            let content = parse_inlines(&processed, block_metadata)?;
            let content = map_inline_locations(state, &processed, &content, &location);

            if let Some(variant) = admonition {
                let Ok(variant) = AdmonitionVariant::from_str(&variant) else {
                    tracing::error!(%variant, "invalid admonition variant");
                    return Err(Error::InvalidAdmonitionVariant(variant) );
                };
                tracing::info!(%variant, "found admonition block with variant");
                Ok(Block::Admonition(Admonition{
                    metadata: block_metadata.metadata.clone(),
                    title: Vec::new(), // TODO(nlopes): Handle admonition titles
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
                    title: Vec::new(), // TODO(nlopes): Handle paragraph titles
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
                let (discrete, metadata) = attributes;
                (discrete, metadata)
            }

        pub(crate) rule attributes() -> (bool, BlockMetadata)
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
                        metadata.attributes.insert(k.to_string(), AttributeValue::String(v));
                    } else if v == AttributeValue::None && pos.is_none() {
                        metadata.positional_attributes.push(k);
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
        = s:$([^('"' | ',' | ']' | '#' | '.' | '%' | '=')]+)
        {
            tracing::debug!(%s, "Found positional attribute value");
            s.to_string()
        }

        rule inner_attribute_value() -> String
        = s:$("\"" [^('"' | ']')]* "\"") { s.to_string() }

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
        assert_eq!(result.location.absolute_end, 30);
        assert_eq!(result.location.start.line, 1);
        assert_eq!(result.location.start.column, 1);
        assert_eq!(result.location.end.line, 1);
        assert_eq!(result.location.end.column, 31);
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_title() {
        let input = "= Document Title";
        let mut state = ParserState::new(input);
        let result = document_parser::document_title(input, &mut state).unwrap();
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
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_title_and_subtitle() {
        let input = "= Document Title: And a subtitle";
        let mut state = ParserState::new(input);
        let result = document_parser::document_title(input, &mut state).unwrap();
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
        assert!(!discrete); // Not discrete
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
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id_mixed() {
        let input = "[astyle#myid.admin,options=read,options=write]";
        let mut state = ParserState::new(input);
        let (discrete, metadata) = document_parser::attributes(input, &mut state).unwrap();
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
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id_mixed_with_quotes() {
        let input = "[astyle#myid.admin,options=\"read,write\"]";
        let mut state = ParserState::new(input);
        let (discrete, metadata) = document_parser::attributes(input, &mut state).unwrap();
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
        assert!(matches!(&result.blocks[0], Block::Image(image)
                 if image.source == Source::Path("sunset.jpg".to_string())
                 && image.metadata.attributes.get("alt") == Some(&AttributeValue::String("Sunset".to_string()))
                 && image.metadata.attributes.get("width") == Some(&AttributeValue::String("300".to_string()))
                 && image.metadata.attributes.get("height") == Some(&AttributeValue::String("400".to_string()))
        ));
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
        assert!(matches!(&result.blocks[0], Block::Image(image)
                 if image.source == Source::Path("sunset.jpg".to_string())
                 && image.metadata.attributes.get("alt") == Some(&AttributeValue::String("Sunset".to_string()))
                 && image.metadata.attributes.get("width") == Some(&AttributeValue::String("300".to_string()))
                 && image.metadata.attributes.get("height") == Some(&AttributeValue::String("400".to_string()))
        ));
        assert!(matches!(&result.blocks[1], Block::Image(image)
                 if image.source == Source::Path("mountain.png".to_string())
                 && image.metadata.attributes.get("alt") == Some(&AttributeValue::String("Mountain".to_string()))
                 && image.metadata.attributes.get("width") == Some(&AttributeValue::String("500".to_string()))
                 && image.metadata.attributes.get("height") == Some(&AttributeValue::String("600".to_string()))
        ));
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
        assert!(matches!(&result.blocks[0], Block::Video(video)
                 if video.sources == vec![Source::Path("Video1ID".to_string()),
                                          Source::Path("Video2ID".to_string()),
                                          Source::Path("Video3ID".to_string())]
                 && video.metadata.attributes.get("youtube") == Some(&AttributeValue::Bool(true))
        ));
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
        assert!(matches!(&result.blocks[0], Block::PageBreak(page_break)
                 if page_break.location.absolute_start == 31
                 && page_break.location.absolute_end == 34
        ));
        assert!(matches!(&result.blocks[1], Block::Paragraph(paragraph)
                 if paragraph.location.absolute_start == 36
                 && paragraph.location.absolute_end == 54
        ));
        assert!(
            matches!(&result.blocks[2], Block::ThematicBreak(thematic_break)
                     if thematic_break.location.absolute_start == 57
                     && thematic_break.location.absolute_end == 61
            )
        );
        assert!(matches!(&result.blocks[3], Block::Paragraph(paragraph)
                 if paragraph.location.absolute_start == 64
                 && paragraph.location.absolute_end == 84
        ));
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
        assert!(matches!(&result.blocks[0], Block::Admonition(admonition)
                 if admonition.variant == AdmonitionVariant::Important
                 && admonition.blocks == vec![Block::Paragraph(Paragraph {
                     metadata: BlockMetadata::default(),
                     title: vec![],
                     content: vec![InlineNode::PlainText(Plain {
                         content: "This is an important message.".to_string(),
                         location: Location {
                             absolute_start: 11,
                             absolute_end: 39,
                             start: crate::Position { line: 1, column: 12 },
                             end: crate::Position { line: 1, column: 40 }
                         }
                     })],
                     location: Location {
                         absolute_start: 11,
                         absolute_end: 39,
                         start: crate::Position { line: 1, column: 12 },
                         end: crate::Position { line: 1, column: 40 }
                     }
                 })]
        ));
        assert_eq!(
            result.location,
            Location {
                absolute_start: 0,
                absolute_end: 39,
                start: crate::Position { line: 1, column: 1 },
                end: crate::Position {
                    line: 1,
                    column: 40
                }
            }
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_regular_paragraph() {
        let input = "This is a regular paragraph.\n";
        let mut state = ParserState::new(input);
        let result = document_parser::document(input, &mut state)
            .unwrap()
            .unwrap();
        assert_eq!(result.blocks.len(), 1);
        assert!(matches!(&result.blocks[0], Block::Paragraph(paragraph)
                 if paragraph.content == vec![InlineNode::PlainText(Plain {
                     content: "This is a regular paragraph.".to_string(),
                     location: Location {
                         absolute_start: 0,
                         absolute_end: 27,
                         start: crate::Position { line: 1, column: 1 },
                         end: crate::Position { line: 1, column: 28 }
                     }
                 })]
        ));
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_two_regular_paragraphs() {
        let input = "This is a regular paragraph.\n\nAnd another paragraph.";
        let mut state = ParserState::new(input);
        let result = document_parser::document(input, &mut state)
            .unwrap()
            .unwrap();
        assert_eq!(result.blocks.len(), 2);
        assert!(matches!(&result.blocks[0], Block::Paragraph(paragraph)
        if paragraph.content == vec![InlineNode::PlainText(Plain {
            content: "This is a regular paragraph.".to_string(),
            location: Location {
                absolute_start: 0,
                absolute_end: 27,
                start: crate::Position { line: 1, column: 1 },
                end: crate::Position { line: 1, column: 28 }
            }
        })]));
        assert!(matches!(&result.blocks[1], Block::Paragraph(paragraph)
            if paragraph.content == vec![InlineNode::PlainText(Plain {
                content: "And another paragraph.".to_string(),
                location: Location {
                    absolute_start: 30,
                    absolute_end: 51,
                    start: crate::Position { line: 3, column: 1 },
                    end: crate::Position { line: 3, column: 22 }
                }
            })]
        ));
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_discrete_heading() {
        let input = "[discrete]\n== A Discrete Heading\n\nA paragraph";
        let mut state = ParserState::new(input);
        let result = document_parser::document(input, &mut state)
            .unwrap()
            .unwrap();
        assert_eq!(result.blocks.len(), 2);
        assert!(matches!(&result.blocks[0], Block::DiscreteHeader(heading)
                 if heading.level == 1
                 && heading.title == vec![InlineNode::PlainText(Plain {
                     content: "A Discrete Heading".to_string(),
                     location: Location {
                         absolute_start: 14,
                         absolute_end: 31,
                         start: crate::Position { line: 2, column: 4 },
                         end: crate::Position { line: 2, column: 21 }
                     }
                 })]
        ));
        assert!(matches!(&result.blocks[1], Block::Paragraph(paragraph)
            if paragraph.content == vec![InlineNode::PlainText(Plain {
                content: "A paragraph".to_string(),
                location: Location {
                    absolute_start: 34,
                    absolute_end: 44,
                    start: crate::Position { line: 4, column: 1 },
                    end: crate::Position { line: 4, column: 11 }
                }
            })]
        ));
    }
}
