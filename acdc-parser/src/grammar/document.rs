#![allow(clippy::too_many_arguments)]
use crate::{
    Admonition, AdmonitionVariant, Anchor, AttributeValue, Audio, Author, Autolink, Block,
    BlockMetadata, Bold, Button, CalloutList, Comment, CurvedApostrophe, CurvedQuotation,
    DelimitedBlock, DelimitedBlockType, DescriptionList, DescriptionListItem, DiscreteHeader,
    Document, DocumentAttribute, Error, Footnote, Form, Header, Highlight, ICON_SIZES, Icon, Image,
    InlineMacro, InlineNode, Italic, Keyboard, LineBreak, Link, ListItem, ListItemCheckedStatus,
    Location, Mailto, Menu, Monospace, OrderedList, PageBreak, Paragraph, Pass, PassthroughKind,
    Plain, Raw, Section, Source, SourceLocation, StandaloneCurvedApostrophe, Stem, StemContent,
    StemNotation, Subscript, Substitution, Subtitle, Superscript, Table, TableOfContents, TableRow,
    ThematicBreak, Title, UnorderedList, Url, Verbatim, Video,
    grammar::{
        ParserState,
        attributes::AttributeEntry,
        inline_preprocessing,
        inline_preprocessor::InlinePreprocessorParserState,
        inline_processing::{
            adjust_and_log_parse_error, parse_inlines, preprocess_inline_content, process_inlines,
        },
        location_mapping::map_inline_locations,
        manpage::{
            derive_manpage_header_attrs, derive_name_section_attrs, extract_plain_text,
            is_manpage_doctype,
        },
        revision::{RevisionInfo, process_revision_info},
        table::parse_table_cell,
    },
    model::{ListLevel, Locateable, SectionLevel},
};

use super::setext;

#[derive(Debug)]
pub(crate) struct PositionWithOffset {
    pub(crate) offset: usize,
    pub(crate) position: crate::Position,
}

#[derive(Debug)]
// Used purely in the grammar to break down the block metadata lines into its different
// types.
enum BlockMetadataLine<'input> {
    Anchor(Anchor),
    Attributes((bool, BlockMetadata)),
    Title(Title),
    DocumentAttribute(&'input str, AttributeValue),
}

#[derive(Debug)]
// Used purely in the grammar to break down header metadata lines (anchors and attributes
// that appear before the document title).
enum HeaderMetadataLine {
    Anchor(Anchor),
    Attributes((bool, BlockMetadata)),
}

#[derive(Debug, Default)]
// Used purely in the grammar to represent the parsed block details
pub(crate) struct BlockParsingMetadata {
    pub(crate) metadata: BlockMetadata,
    title: Title,
    parent_section_level: Option<SectionLevel>,
}

#[derive(Debug)]
/// Attribute shorthand syntax: .role, #id, %option
/// Used for both block-level attributes and inline formatting attributes
enum Shorthand {
    Id(String),
    Role(String),
    Option(String),
}

const RESERVED_NAMED_ATTRIBUTE_ID: &str = "id";
const RESERVED_NAMED_ATTRIBUTE_ROLE: &str = "role";
const RESERVED_NAMED_ATTRIBUTE_OPTIONS: &str = "opts";

pub(crate) fn match_constrained_boundary(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t'
            | b'\n'
            | b'\r'
            | b'('
            | b'{'
            | b'['
            | b')'
            | b'}'
            | b']'
            | b'/'
            | b'-'
            | b'|'
            | b','
            | b';'
            | b'.'
            | b'?'
            | b'!'
            | b'\''
            | b'"'
            | b'<'
            | b'>'
    )
}

/// Helper to check delimiter matching and return error if mismatched
fn check_delimiters(
    open: &str,
    close: &str,
    block_type: &str,
    detail: SourceLocation,
) -> Result<(), Error> {
    if open == close {
        Ok(())
    } else {
        Err(Error::mismatched_delimiters(detail, block_type))
    }
}

/// Macro to handle inline processing errors with logging
macro_rules! process_inlines_or_err {
    ($call:expr, $msg:literal) => {
        $call.map_err(|e| {
            tracing::error!(?e, $msg);
            $msg
        })
    };
}

/// Helper to create `SourceLocation` from a Location and file path
fn create_source_location(location: Location, file: Option<std::path::PathBuf>) -> SourceLocation {
    SourceLocation {
        file,
        positioning: crate::Positioning::Location(location),
    }
}

fn get_literal_paragraph(
    state: &ParserState,
    content: &str,
    start: usize,
    end: usize,
    offset: usize,
    block_metadata: &BlockParsingMetadata,
) -> Block {
    tracing::debug!(
        content,
        "paragraph starts with a space - switching to literal block"
    );
    let mut metadata = block_metadata.metadata.clone();
    metadata.move_positional_attributes_to_attributes();
    metadata.style = Some("literal".to_string());
    let location = state.create_block_location(start, end, offset);

    // Strip leading space from each line ONLY if ALL lines consistently have leading space
    // This matches asciidoctor's behavior
    let lines: Vec<&str> = content.lines().collect();
    let all_lines_have_leading_space = lines
        .iter()
        .all(|line| line.is_empty() || line.starts_with(' '));

    let content = if all_lines_have_leading_space {
        lines
            .iter()
            .map(|line| line.strip_prefix(' ').unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        content.to_string()
    };

    tracing::debug!(
        content,
        all_lines_have_leading_space,
        "created literal paragraph"
    );
    Block::Paragraph(Paragraph {
        content: vec![InlineNode::PlainText(Plain {
            content,
            location: location.clone(),
        })],
        metadata,
        title: block_metadata.title.clone(),
        location,
    })
}

/// Assembles principal text from first line and continuation lines.
/// Used by list item parsing rules to combine multi-line content.
fn assemble_principal_text(first_line: &str, continuation_lines: &[&str]) -> String {
    if continuation_lines.is_empty() {
        first_line.to_string()
    } else {
        format!("{first_line}\n{}", continuation_lines.join("\n"))
    }
}

/// Calculates the end position for a list item based on its principal text.
/// Returns `start` if empty, otherwise one less than `first_line_end`.
const fn calculate_item_end(
    principal_text_is_empty: bool,
    start: usize,
    first_line_end: usize,
) -> usize {
    if principal_text_is_empty {
        start
    } else {
        first_line_end.saturating_sub(1)
    }
}

peg::parser! {
    pub(crate) grammar document_parser(state: &mut ParserState) for str {
        use std::str::FromStr;
        use crate::model::Substitute;

        // We ignore empty lines before we set the start position of the document because
        // the asciidoc document should not consider empty lines at the beginning or end
        // of the file.
        //
        // We also ignore comments before the header - maybe we should change this but as
        // it stands in our current model, it makes no sense to have comments in the
        // blocks as it is a completely separate part of the document.
        pub(crate) rule document() -> Result<Document, Error>
        = eol()* start:position() comments_before_header:comment_line_block(0)* header_result:header() blocks:blocks(0, None) end:position() (eol()* / ![_]) {
            let header = header_result?;
            let blocks = comments_before_header.into_iter().collect::<Result<Vec<_>, Error>>()?.into_iter().chain(blocks?).collect();

            // Ensure end offset is on a valid UTF-8 boundary
            let mut document_end_offset = end.offset;
            if document_end_offset > state.input.len() {
                document_end_offset = state.input.len();
            }
            // If not on a boundary, round forward to the next boundary
            while document_end_offset < state.input.len() && !state.input.is_char_boundary(document_end_offset) {
                document_end_offset += 1;
            }
            // Then decrement by one byte to get the last byte of content
            let document_end_offset = if document_end_offset == 0 {
                0
            } else {
                crate::grammar::utf8_utils::safe_decrement_offset(&state.input, document_end_offset)
            };

            // Ensure the invariant: absolute_start <= absolute_end
            let (absolute_start, absolute_end) = if start.offset > document_end_offset {
                // This can happen with whitespace-only input where eol()* consumes all content
                // In this case, treat as an empty document at the start position
                (start.offset, start.offset)
            } else {
                (start.offset, document_end_offset)
            };

            // Special case for truly empty input: TCK expects column 0
            // Only for zero-byte input, not whitespace-only
            let (start_position, end_position) = if state.input.is_empty() || (absolute_start == 0 && absolute_end == 0) {
                // Whitespace-only documents should use column 1
                (
                    crate::Position { line: 1, column: 0 },
                    crate::Position { line: 1, column: 0 }
                )
            } else {
                (
                    start.position,
                    state.line_map.offset_to_position(absolute_end, &state.input)
                )
            };

            Ok(Document {
                name: "document".to_string(),
                r#type: "block".to_string(),
                header,
                location: Location {
                    absolute_start,
                    absolute_end,
                    start: start_position,
                    end: end_position,
                },
                attributes: state.document_attributes.clone(),
                blocks,
                footnotes: state.footnote_tracker.footnotes.clone(),
                toc_entries: state.toc_tracker.entries.clone(),
            })
        }

        pub(crate) rule header() -> Result<Option<Header>, Error>
            = start:position!()
            ((document_attribute() / comment()) (eol()+ / ![_]))*
            // Parse header metadata (anchors and attributes) before the document title
            metadata:header_metadata()
            title_authors:(title_authors:title_authors() { title_authors })?
            (eol()+ (document_attribute() / comment()))*
            end:position!()
            (eol()*<,2> / ![_])
        {
            if let Some((title, subtitle, authors)) = title_authors {
                let mut location = state.create_location(start, end);
                // Decrement end by one character (for byte offset, use safe UTF-8 decrement)
                location.absolute_end = crate::grammar::utf8_utils::safe_decrement_offset(&state.input, location.absolute_end);
                location.end.column = location.end.column.saturating_sub(1);
                let header = Header {
                    metadata,
                    title,
                    subtitle,
                    authors,
                    location,
                };

                // Derive manpage attributes from header if doctype=manpage
                // This must happen during parsing so {mantitle} etc. work in body
                if is_manpage_doctype(&state.document_attributes) {
                    derive_manpage_header_attrs(
                        Some(&header),
                        &mut state.document_attributes,
                        state.options.strict,
                        state.current_file.as_deref(),
                    )?;
                }

                Ok(Some(header))
            } else {
                tracing::info!("No title or authors found in the document header.");
                Ok(None)
            }
        }

        /// Parse block metadata lines (anchors and attributes) that can appear before a document title.
        /// Only consumes metadata if followed by a document title to avoid stealing attributes
        /// meant for the first block when there's no document title.
        rule header_metadata() -> BlockMetadata
            = lines:(
                anchor:anchor() { HeaderMetadataLine::Anchor(anchor) }
                / attr:attributes_line() { HeaderMetadataLine::Attributes(attr) }
            )+ &document_title()
            {
                let mut metadata = BlockMetadata::default();

                for line in lines {
                    match line {
                        HeaderMetadataLine::Anchor(anchor) => metadata.anchors.push(anchor),
                        HeaderMetadataLine::Attributes((_, attr_metadata)) => {
                            // Merge attribute metadata - last one wins for id/style
                            if attr_metadata.id.is_some() {
                                metadata.id = attr_metadata.id;
                            }
                            if attr_metadata.style.is_some() {
                                metadata.style = attr_metadata.style;
                            }
                            metadata.roles.extend(attr_metadata.roles);
                            metadata.options.extend(attr_metadata.options);
                            metadata.attributes = attr_metadata.attributes;
                            metadata.positional_attributes = attr_metadata.positional_attributes;
                        }
                    }
                }
                metadata
            }
            / { BlockMetadata::default() }

        pub(crate) rule title_authors() -> (Title, Option<Subtitle>, Vec<Author>)
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

        pub(crate) rule document_title() -> (Title, Option<Subtitle>)
        = document_title_atx()
        / document_title_setext()

        /// ATX-style document title: `= Title` or `# Title`
        rule document_title_atx() -> (Title, Option<Subtitle>)
        = document_title_token() whitespace() start:position!() title:$([^'\n']*) end:position!()
        {
            let mut subtitle = None;
            let mut title_end = end;
            if let Some(subtitle_start) = title.rfind(':') {
                title_end = start+subtitle_start;
                subtitle = Some(Subtitle::new(vec![InlineNode::PlainText(Plain {
                    content: title[subtitle_start + 1..].trim().to_string(),
                    location: state.create_location(
                        title_end + 1,
                        end.saturating_sub(1),
                    ),
                })]));
            }
            let title_location = state.create_location(start, title_end.saturating_sub(1));
            (Title::new(vec![InlineNode::PlainText(Plain {
                content: title[..title_end - start].trim().to_string(),
                location: title_location,
            })]), subtitle)
        }

        /// Setext-style document title: Title underlined with `=` characters
        ///
        /// ```text
        /// Document Title
        /// ==============
        /// ```
        ///
        /// The underline must be within ±2 characters of the title width.
        /// Only enabled when the setext feature is compiled in AND the runtime
        /// option is enabled.
        rule document_title_setext() -> (Title, Option<Subtitle>)
        = start:position!() title:$([^'\n']+) end:position!() eol()
          underline:$("="+) &(eol() / ![_])
        {?
            // Check if setext mode is enabled
            if !setext::is_enabled(state) {
                return Err("setext mode not enabled");
            }

            let title_text = title.trim();
            let title_width = title_text.chars().count();
            let underline_width = underline.chars().count();

            // Check underline width tolerance (±2 characters)
            if !setext::width_ok(title_width, underline_width) {
                return Err("underline width out of tolerance");
            }

            // Check underline is level 0 (document title uses =)
            if !underline.starts_with('=') {
                return Err("document title must use = underline");
            }

            // Parse subtitle (text after last colon)
            let mut subtitle = None;
            let mut title_content = title_text.to_string();
            if let Some(subtitle_start) = title_text.rfind(':') &&
            let Some(subtitle_text) = title_text.get(subtitle_start + 1..) {
                let subtitle_text = subtitle_text.trim();
                if !subtitle_text.is_empty() {
                    if let Some(text) = title_text.get(..subtitle_start) {
                        title_content = text.trim().to_string();
                    }
                    subtitle = Some(Subtitle::new(vec![InlineNode::PlainText(Plain {
                        content: subtitle_text.to_string(),
                        location: state.create_location(
                            start + subtitle_start + 1,
                            end.saturating_sub(1),
                        ),
                    })]));
                }
            }

            let title_location = state.create_location(start, end.saturating_sub(1));
            Ok((Title::new(vec![InlineNode::PlainText(Plain {
                content: title_content,
                location: title_location,
            })]), subtitle))
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
            Author::new(first, Some(middle), Some(last))
        }
        / first:name_part() whitespace()+ last:name_part() {
            Author::new(first, None, Some(last))
        }
        / first:name_part() {
            Author::new(first, None, None)
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
            let AttributeEntry{key, value, set} = att;
            tracing::info!(%set, %key, %value, "Found document attribute in the document header");
            state.document_attributes.set(key.into(), value);
        }

        pub(crate) rule blocks(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Vec<Block>, Error>
        = blocks:block(offset, parent_section_level)*
        {
            blocks.into_iter().collect::<Result<Vec<_>, Error>>()
        }


        pub(crate) rule block(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block, Error>
        = eol()*
        // First check: if we're at a same-or-higher-level section, fail the entire block
        // This prevents section content from consuming sibling/parent sections as paragraphs
        !same_or_higher_level_section(offset, parent_section_level)
        block:(
            comment_line_block(offset) /
            document_attribute_block(offset) /
            &"[discrete" dh:discrete_header(offset) { dh } /
            section:section(offset, parent_section_level) { section } /
            // Try setext-style sections (only enabled with setext feature + runtime flag)
            section_setext:section_setext(offset, parent_section_level) { section_setext } /
            block_generic(offset, parent_section_level)
        )
        {
            block
        }

        /// Single-line comment that becomes a block in the AST.
        /// Line comments begin with `//` (but not `///` or `////` which are block comment delimiters).
        rule comment_line_block(offset: usize) -> Result<Block, Error>
        = start:position!() "//" !("/") content:$([^'\n']*) end:position!() (eol() / ![_])
        {
            Ok(Block::Comment(Comment {
                content: content.to_string(),
                location: state.create_location(start + offset, end + offset),
            }))
        }

        // Check if the upcoming content is a section at same or higher level (which
        // should not be parsed as content)
        //
        // This rule skips optional metadata (anchors, attributes, etc.) before checking
        // the section level, so that `[[anchor]]\n== Section` is correctly identified as
        // a sibling section.
        //
        // Checks both ATX-style (= or #) and setext-style (underlined) sections.
        rule same_or_higher_level_section(offset: usize, parent_section_level: Option<SectionLevel>) -> ()
        = (anchor() / attributes_line() / document_attribute_line() / title_line(offset))*
          (
            // ATX-style section check
            level:section_level(offset, parent_section_level)
            {?
                if let Some(parent_level) = parent_section_level {
                    let upcoming_level = level.1 + 1; // Convert to 1-based
                    if upcoming_level <= parent_level {
                        Ok(()) // This IS a same or higher level section
                    } else {
                        Err("not a same or higher level section")
                    }
                } else {
                    Err("no parent section level to compare")
                }
            }
            /
            // Setext-style section check (title followed by underline)
            &setext_section_lookahead(parent_section_level)
          )

        /// Lookahead rule to detect setext sections at same or higher level.
        /// Used by same_or_higher_level_section to properly terminate sections.
        rule setext_section_lookahead(parent_section_level: Option<SectionLevel>) -> ()
        = title:$([^'\n']+) eol() underline:$(['-' | '~' | '^' | '+']+) &(eol() / ![_])
        {?
            // Only check if setext mode is enabled
            if !setext::is_enabled(state) {
                return Err("setext mode not enabled");
            }

            // Validate underline width
            let title_width = title.trim().chars().count();
            let underline_width = underline.chars().count();
            if !setext::width_ok(title_width, underline_width) {
                return Err("underline width out of tolerance");
            }

            // Get level from underline character
            let underline_char = underline.chars().next().ok_or("empty underline")?;
            let level = setext::char_to_level(underline_char).ok_or("invalid setext char")?;

            // Level 0 (=) is document title, not section
            if level == 0 {
                return Err("not a section, seems like you're trying to define a document title");
            }

            // Check if this is a same-or-higher level section
            if let Some(parent_level) = parent_section_level {
                if level <= parent_level {
                    Ok(()) // This IS a same or higher level setext section
                } else {
                    Err("not a same or higher level section")
                }
            } else {
                Err("no parent section level to compare")
            }
        }

        rule discrete_header(offset: usize) -> Result<Block, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, None) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in discrete_header");
                "block metadata parse error"
            })
        })
        section_level:section_level(offset, None) whitespace()
        title_start:position!() title:section_title(offset, &block_metadata) title_end:position!() end:position!() &eol()*<1,2>
        {
            let title = title?;
            tracing::info!(?block_metadata, ?title, ?title_start, ?title_end, "parsing discrete header block");

            let level = section_level.1;
            let location = state.create_block_location(start, end, offset);

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
            let AttributeEntry{ key, value, .. } = att;
            state.document_attributes.set(key.into(), value.clone());
            Ok(Block::DocumentAttribute(DocumentAttribute {
                name: key.into(),
                value,
                location: state.create_location(start+offset, end+offset)
            }))
        }

        pub(crate) rule section(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in section");
                "block metadata parse error"
            })
        })
        section_level_start:position!()
        section_level:section_level(offset, parent_section_level)
        section_level_end:position!()
        whitespace()
        title_start:position!()
        section_header:(title:section_title(offset, &block_metadata) title_end:position!() &(eol()*<1,2> / ![_]) {
            let title = title?;
            let section_id = Section::generate_id(&block_metadata.metadata, &title).to_string();

            // Extract xreflabel from the last anchor (same anchor used for section ID)
            // This matches asciidoctor behavior: [[id,xreflabel]] provides custom cross-reference text
            let xreflabel = block_metadata.metadata.anchors.last().and_then(|a| a.xreflabel.clone());

            // Register section for TOC immediately after title is parsed, before content
            state.toc_tracker.register_section(title.clone(), section_level.1, section_id.clone(), xreflabel);

            Ok::<(Title, String), Error>((title, section_id))
        })
        content:section_content(offset, Some(section_level.1+1))? end:position!()
        {
            let (title, section_id) = section_header?;
            tracing::info!(?offset, ?block_metadata, ?title, "parsing section block");

            // Validate section level against parent section level if any is provided
            if let Some(parent_level) = parent_section_level && (
                section_level.1 < parent_level  || section_level.1+1 > parent_level+1 || section_level.1 > 5) {
                    return Err(Error::NestedSectionLevelMismatch(
                        Box::new(create_source_location(state.create_block_location(section_level_start, section_level_end, offset), state.current_file.clone())),
                        section_level.1+1,
                        parent_level + 1,
                    ));
            }

            let level = section_level.1;
            let location = state.create_block_location(start, end, offset);

            // Derive manname/manpurpose from NAME section in manpage documents
            //
            // This must happen before subsequent sections are parsed so {manname} works
            // in SYNOPSIS, DESCRIPTION, etc.
            if level == 1 && is_manpage_doctype(&state.document_attributes) {
                let title_text = extract_plain_text(&title);
                if title_text.eq_ignore_ascii_case("NAME")
                    && let Some(Ok(ref blocks)) = content
                    && let Some(Block::Paragraph(para)) = blocks.first()
                {
                    let para_text = extract_plain_text(&para.content);
                    derive_name_section_attrs(&para_text, &mut state.document_attributes);
                }
            }

            Ok(Block::Section(Section {
                metadata: block_metadata.metadata,
                title,
                level,
                content: content.unwrap_or(Ok(Vec::new()))?,
                location
            }))
        }

        /// Setext-style section header: Title underlined with `-`, `~`, `^`, or `+`
        ///
        /// ```text
        /// Section Title
        /// -------------
        /// ```
        ///
        /// The underline character determines the section level:
        /// - `-` = Level 1
        /// - `~` = Level 2
        /// - `^` = Level 3
        /// - `+` = Level 4
        ///
        /// The underline must be within ±2 characters of the title width.
        /// Only enabled when the setext feature is compiled in AND the runtime
        /// option is enabled.
        /// Parse a setext section level from the underline character.
        /// Returns the level (1-4) corresponding to -, ~, ^, +
        rule setext_section_level(title_width: usize, parent_section_level: Option<SectionLevel>) -> u8
        = underline:$(['-' | '~' | '^' | '+']+) &(eol() / ![_])
        {?
            // Check if setext mode is enabled
            if !setext::is_enabled(state) {
                return Err("setext mode not enabled");
            }

            let underline_width = underline.chars().count();

            // Check underline width tolerance (±2 characters)
            if !setext::width_ok(title_width, underline_width) {
                return Err("underline width out of tolerance");
            }

            // Get the underline character and determine section level
            let underline_char = underline.chars().next().ok_or("empty underline")?;
            let level = setext::char_to_level(underline_char).ok_or("invalid setext underline character")?;

            // Document title (level 0) uses =, not allowed here
            if level == 0 {
                return Err("use = underline for document title, not section");
            }

            // Validate section level against parent section level if any is provided
            if let Some(parent_level) = parent_section_level
                && (level < parent_level || level > parent_level + 1 || level > 5)
            {
                return Err("section level mismatch with parent");
            }

            Ok(level)
        }

        pub(crate) rule section_setext(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in section_setext");
                "block metadata parse error"
            })
        })
        title_start:position() title:$([^'\n']+) title_end:position!() eol()
        setext_level:setext_section_level(title.trim().chars().count(), parent_section_level)
        section_header:({
            // Parse the title using inline processing
            match process_inlines(state, &block_metadata, &title_start, title_end, offset, title) {
                Ok(processed_title) => {
                    let processed_title = Title::new(processed_title);
                    let section_id = Section::generate_id(&block_metadata.metadata, &processed_title).to_string();

                    // Extract xreflabel from the last anchor
                    let xreflabel = block_metadata.metadata.anchors.last().and_then(|a| a.xreflabel.clone());

                    // Register section for TOC
                    state.toc_tracker.register_section(processed_title.clone(), setext_level, section_id.clone(), xreflabel);

                    Ok::<(Title, String), Error>((processed_title, section_id))
                }
                Err(e) => Err(e),
            }
        })
        content:section_content(offset, Some(setext_level + 1))? end:position!()
        {
            let (title, _section_id) = section_header?;
            let location = state.create_block_location(start, end, offset);

            // Derive manname/manpurpose from NAME section in manpage documents
            if setext_level == 1 && is_manpage_doctype(&state.document_attributes) {
                let title_text = extract_plain_text(&title);
                if title_text.eq_ignore_ascii_case("NAME")
                    && let Some(Ok(ref blocks)) = content
                    && let Some(Block::Paragraph(para)) = blocks.first()
                {
                    let para_text = extract_plain_text(&para.content);
                    derive_name_section_attrs(&para_text, &mut state.document_attributes);
                }
            }

            Ok(Block::Section(Section {
                metadata: block_metadata.metadata,
                title,
                level: setext_level,
                content: content.unwrap_or(Ok(Vec::new()))?,
                location,
            }))
        }

        rule block_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<BlockParsingMetadata, Error>
        = lines:(
            anchor:anchor() { Ok::<BlockMetadataLine<'input>, Error>(BlockMetadataLine::Anchor(anchor)) }
            / attr:attributes_line() { Ok::<BlockMetadataLine<'input>, Error>(BlockMetadataLine::Attributes(attr)) }
            / doc_attr:document_attribute_line() { Ok::<BlockMetadataLine<'input>, Error>(BlockMetadataLine::DocumentAttribute(doc_attr.key, doc_attr.value)) }
            / title:title_line(offset) { title.map(BlockMetadataLine::Title) }
        )*
        {
            let mut metadata = BlockMetadata::default();
            let mut discrete = false;
            let mut title = Title::default();

            for line in lines {
                // Skip errors from title parsing (e.g., empty titles like "." + newline)
                let Ok(value) = line else {
                    tracing::warn!(?line, "failed to parse block metadata line, skipping");
                    continue
                };
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
                    BlockMetadataLine::DocumentAttribute(key, value) => {
                        // Set the document attribute immediately so it's available for
                        // subsequent attribute references (e.g., in title lines)
                        state.document_attributes.set(key.into(), value);
                    },
                    BlockMetadataLine::Title(inner) => {
                        title = inner;
                    }
                }
            }
            Ok(BlockParsingMetadata {
                metadata,
                title,
                parent_section_level,
            })
        }

        // A title line can be a simple title or a section title
        //
        // A title line is a line that starts with a period (.) followed by a non-whitespace character
        rule title_line(offset: usize) -> Result<Title, Error>
        = period() start:position() title:$(![' ' | '\t' | '\n' | '\r' | '.'] [^'\n']*) end:position!() eol()
        {
            tracing::info!(?title, ?start, ?end, "Found title line in block metadata");
            let block_metadata = BlockParsingMetadata::default();
            let title = process_inlines(state, &block_metadata, &start, end, offset, title)?;
            Ok(title.into())
        }

        // A document attribute line in block metadata context
        // This allows document attributes to be set between block attributes and the block content
        // Uses the same parsing logic as document attributes in the header
        rule document_attribute_line() -> AttributeEntry<'input>
        = attr:document_attribute_match() eol()
        {
            tracing::info!(?attr, "Found document attribute in block metadata");
            attr
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
                let prev_byte_pos = absolute_pos.saturating_sub(1);
                state.input.as_bytes().get(prev_byte_pos).is_some_and(|&b| b == b'\n')
            };

            if !at_line_start {
                return Err("section level must be at line start");
            }

            Ok((level, level.len().try_into().unwrap_or(1)-1))
        }

        rule section_title(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Title, Error>
        = title_start:position() title:$([^'\n']*) end:position!()
        {
            tracing::info!(?title, ?title_start, ?end, offset, "Found section title");
            let content = process_inlines(state, block_metadata, &title_start, end, offset, title)?;
            Ok(Title::new(content))
        }

        rule section_content(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Vec<Block>, Error>
        = blocks(offset, parent_section_level) / { Ok(vec![]) }

        pub(crate) rule block_generic(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in block_generic");
                "block metadata parse error"
            })
        })
        block:(
            delimited_block:delimited_block(start, offset, &block_metadata) { delimited_block }
            / image:image(start, offset, &block_metadata) { image }
            / audio:audio(start, offset, &block_metadata) { audio }
            / video:video(start, offset, &block_metadata) { video }
            / toc:toc(start, offset, &block_metadata) { toc }
            / thematic_break:thematic_break(start, offset, &block_metadata) { thematic_break }
            / page_break:page_break(start, offset, &block_metadata) { page_break }
            / list:list(start, offset, &block_metadata) { list }
            / quoted_paragraph:quoted_paragraph(start, offset, &block_metadata) { quoted_paragraph }
            / markdown_blockquote:markdown_blockquote(start, offset, &block_metadata) { markdown_blockquote }
            / paragraph:paragraph(start, offset, &block_metadata) { paragraph }
        ) {
            block
        }

        // Block parsing for continuation context - lists inside continuations cannot consume
        // further continuations (those belong to the parent item that started the continuation)
        rule block_in_continuation(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in block_in_continuation");
                "block metadata parse error"
            })
        })
        block:(
            delimited_block:delimited_block(start, offset, &block_metadata) { delimited_block }
            / image:image(start, offset, &block_metadata) { image }
            / audio:audio(start, offset, &block_metadata) { audio }
            / video:video(start, offset, &block_metadata) { video }
            / toc:toc(start, offset, &block_metadata) { toc }
            / thematic_break:thematic_break(start, offset, &block_metadata) { thematic_break }
            / page_break:page_break(start, offset, &block_metadata) { page_break }
            // Lists in continuation context cannot consume further continuations
            / list:list_with_continuation(start, offset, &block_metadata, false) { list }
            / quoted_paragraph:quoted_paragraph(start, offset, &block_metadata) { quoted_paragraph }
            / markdown_blockquote:markdown_blockquote(start, offset, &block_metadata) { markdown_blockquote }
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
        rule open_delimiter() -> &'input str = delim:$("-"*<2,2> / "~"*<4,>) { delim }
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

            check_delimiters(open_delim, close_delim, "example", create_source_location(state.create_block_location(start, end, offset), state.current_file.clone()))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);

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
                metadata.style = None; // Clear style to avoid confusion (reuse existing clone)
                return Ok(Block::Admonition(Admonition::new(admonition_variant, blocks, location).with_metadata(metadata).with_title(block_metadata.title.clone())));
            }

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata, // Use the existing clone instead of cloning again
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
            check_delimiters(open_delim, close_delim, "comment", create_source_location(state.create_block_location(start, end, offset), state.current_file.clone()))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();

            let location = state.create_block_location(start, end, offset);
            let content_location = state.create_block_location(content_start, content_end, offset);

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
            check_delimiters(open_delim, close_delim, "listing", create_source_location(state.create_block_location(start, end, offset), state.current_file.clone()))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let content_location = state.create_block_location(content_start, content_end, offset);

            let (resolved_content, callout_numbers) = resolve_verbatim_callouts(content);
            state.last_block_was_verbatim = true;
            state.last_verbatim_callouts = callout_numbers;

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedListing(vec![InlineNode::VerbatimText(Verbatim {
                    content: resolved_content,
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
            check_delimiters(open_delim, close_delim, "listing", create_source_location(state.create_block_location(start, end, offset), state.current_file.clone()))?;
            let mut metadata = block_metadata.metadata.clone();

            // If we captured a language, add it as a positional attribute
            if let Some(language) = lang {
                metadata.positional_attributes.insert(0, language.to_string());
            }

            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let content_location = state.create_block_location(content_start, content_end, offset);

            let (resolved_content, callout_numbers) = resolve_verbatim_callouts(content);
            state.last_block_was_verbatim = true;
            state.last_verbatim_callouts = callout_numbers;

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedListing(vec![InlineNode::VerbatimText(Verbatim {
                    content: resolved_content,
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
            check_delimiters(open_delim, close_delim, "literal", create_source_location(state.create_block_location(start, end, offset), state.current_file.clone()))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let content_location = state.create_block_location(content_start, content_end, offset);

            let (resolved_content, callout_numbers) = resolve_verbatim_callouts(content);
            state.last_block_was_verbatim = true;
            state.last_verbatim_callouts = callout_numbers;

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata,
                delimiter: open_delim.to_string(),
                inner: DelimitedBlockType::DelimitedLiteral(vec![InlineNode::VerbatimText(Verbatim {
                    content: resolved_content,
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
            check_delimiters(open_delim, close_delim, "open", create_source_location(state.create_block_location(start, end, offset), state.current_file.clone()))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);

            let blocks = if content.trim().is_empty() {
                Vec::new()
            } else {
                document_parser::blocks(content, state, content_start+offset, block_metadata.parent_section_level).unwrap_or_else(|e| {
                    adjust_and_log_parse_error(&e, content, content_start+offset, state, "Error parsing content as blocks in open block");
                    Ok(Vec::new())
                })?
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

            check_delimiters(open_delim, close_delim, "sidebar", create_source_location(state.create_block_location(start, end, offset), state.current_file.clone()))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);

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
            check_delimiters(open_delim, close_delim, "table", create_source_location(state.create_block_location(start, end, offset), state.current_file.clone()))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let table_location = state.create_block_location(table_start, end, offset);
            let content_location = state.create_block_location(content_start, content_end, offset);

            let separator = if let Some(AttributeValue::String(sep)) = block_metadata.metadata.attributes.get("separator") {
                sep.clone()
            } else if let Some(AttributeValue::String(format)) = block_metadata.metadata.attributes.get("format") {
                match format.as_str() {
                    "csv" => ",",
                    "dsv" => ":",
                    "tsv" => "\t",
                    unknown_format => {
                        tracing::warn!(format = %unknown_format, "unknown table format, using default separator");
                        "|"
                    }
                }.to_string()
            } else {
                "|".to_string()
            };

            let (ncols, column_formats) = if let Some(AttributeValue::String(cols)) = block_metadata.metadata.attributes.get("cols") {
                // Parse cols attribute
                // Full syntax: [multiplier*][halign][valign][width][style]
                // Examples: "3*", "^.>2a", "2*>.^1m", "<,^,>", "15%,30%,55%"
                let mut specs = Vec::new();

                for part in cols.split(',') {
                    let s = part.trim().trim_matches('"');

                    // Check for "N*" notation (e.g., "3*" means 3 columns with same spec)
                    let (multiplier, spec_str) = if let Some(pos) = s.find('*') {
                        let mult_str = &s[..pos];
                        let mult = mult_str.parse::<usize>().unwrap_or(1);
                        (mult, &s[pos + 1..])
                    } else {
                        (1, s)
                    };

                    let mut halign = crate::HorizontalAlignment::default();
                    let mut valign = crate::VerticalAlignment::default();
                    let mut width = crate::ColumnWidth::default();
                    let mut style = crate::ColumnStyle::default();

                    // Parse style (last character if it's a letter: a, d, e, h, l, m, s)
                    let spec_str = if let Some(last_char) = spec_str.chars().last() {
                        match last_char {
                            'a' => { style = crate::ColumnStyle::AsciiDoc; &spec_str[..spec_str.len()-1] }
                            'd' => { style = crate::ColumnStyle::Default; &spec_str[..spec_str.len()-1] }
                            'e' => { style = crate::ColumnStyle::Emphasis; &spec_str[..spec_str.len()-1] }
                            'h' => { style = crate::ColumnStyle::Header; &spec_str[..spec_str.len()-1] }
                            'l' => { style = crate::ColumnStyle::Literal; &spec_str[..spec_str.len()-1] }
                            'm' => { style = crate::ColumnStyle::Monospace; &spec_str[..spec_str.len()-1] }
                            's' => { style = crate::ColumnStyle::Strong; &spec_str[..spec_str.len()-1] }
                            _ => spec_str
                        }
                    } else {
                        spec_str
                    };

                    // Parse vertical alignment markers: .<, .^, .>
                    if spec_str.contains(".<") {
                        valign = crate::VerticalAlignment::Top;
                    } else if spec_str.contains(".^") {
                        valign = crate::VerticalAlignment::Middle;
                    } else if spec_str.contains(".>") {
                        valign = crate::VerticalAlignment::Bottom;
                    }

                    // Parse horizontal alignment markers: <, ^, > (not preceded by .)
                    for (i, c) in spec_str.char_indices() {
                        let prev_char = if i > 0 { spec_str.chars().nth(i - 1) } else { None };
                        if prev_char == Some('.') {
                            continue; // This is a vertical alignment marker
                        }
                        match c {
                            '<' => halign = crate::HorizontalAlignment::Left,
                            '^' => halign = crate::HorizontalAlignment::Center,
                            '>' => halign = crate::HorizontalAlignment::Right,
                            _ => {}
                        }
                    }

                    // Parse width: integer (proportional), percentage, or ~ (auto)
                    // The ~ (tilde) for auto-width was added in Asciidoctor 1.5.7
                    // See: https://github.com/asciidoctor/asciidoctor/issues/1844
                    // Remove alignment markers to find the width
                    let width_str: String = spec_str.chars()
                        .filter(|c| !matches!(c, '<' | '^' | '>' | '.'))
                        .collect();
                    if !width_str.is_empty() {
                        if width_str == "~" {
                            width = crate::ColumnWidth::Auto;
                        } else if width_str.ends_with('%') {
                            if let Ok(pct) = width_str.trim_end_matches('%').parse::<u32>() {
                                width = crate::ColumnWidth::Percentage(pct);
                            }
                        } else if let Ok(prop) = width_str.parse::<u32>() {
                            width = crate::ColumnWidth::Proportional(prop);
                        }
                    }

                    // Add the spec for each column in the multiplier (including defaults)
                    let spec = crate::ColumnFormat { halign, valign, width, style };
                    for _ in 0..multiplier {
                        specs.push(spec.clone());
                    }
                }

                (Some(specs.len()), specs)
            } else {
                (None, Vec::new())
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

                // Calculate row line number from first cell for better error reporting
                let row_line = if let Some(first) = row.first() {
                    state.create_location(first.1, first.2).start.line
                } else {
                    table_location.start.line  // Fallback if row is empty (shouldn't happen)
                };

                // validate that if we have ncols we have the same number of columns in each row
                if let Some(ncols) = ncols
                && columns.len() != ncols
                {
                    tracing::warn!(
                        actual = columns.len(),
                        expected = ncols,
                        line = row_line,
                        "table row has incorrect column count, skipping row"
                    );
                    continue;
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
                columns: column_formats,
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
            check_delimiters(open_delim, close_delim, "pass", create_source_location(state.create_block_location(start, end, offset), state.current_file.clone()))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let content_location = state.create_block_location(content_start, content_end, offset);

            // Check if this is a stem block
            let inner = if let Some(ref style) = metadata.style {
                if style == "stem" {
                    // Get notation from :stem: document attribute
                    let notation = match state.document_attributes.get("stem") {
                        Some(AttributeValue::String(s)) => {
                            StemNotation::from_str(s).unwrap_or(StemNotation::Latexmath)
                        }
                        Some(AttributeValue::Bool(true) | AttributeValue::None) => {
                            StemNotation::Latexmath
                        }
                        _ => StemNotation::Latexmath,
                    };
                    metadata.style = None; // Clear style to avoid confusion
                    DelimitedBlockType::DelimitedStem(StemContent {
                        content: content.to_string(),
                        notation,
                    })
                } else {
                    DelimitedBlockType::DelimitedPass(vec![InlineNode::RawText(Raw {
                        content: content.to_string(),
                        location: content_location,
                    })])
                }
            } else {
                DelimitedBlockType::DelimitedPass(vec![InlineNode::RawText(Raw {
                    content: content.to_string(),
                    location: content_location,
                })])
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim.to_string(),
                inner,
                title: block_metadata.title.clone(),
                location,
            }))
        }

        rule quote_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
            = open_delim:quote_delimiter() eol()
            content_start:position!() content:until_quote_delimiter() content_end:position!()
            eol() close_delim:quote_delimiter() end:position!()
        {
            check_delimiters(open_delim, close_delim, "quote", create_source_location(state.create_block_location(start, end, offset), state.current_file.clone()))?;
            let mut metadata = block_metadata.metadata.clone();
            // Extract quote/verse attribution from positional attributes
            //
            // [quote, attribution(author), citation] or
            // [verse, attribution(author), citation]
            if metadata.positional_attributes.len() >= 2 {
                metadata.attributes.insert("citation".into(),
                    AttributeValue::String(metadata.positional_attributes.remove(1).trim().to_string()));
            }
            if !metadata.positional_attributes.is_empty() {
                metadata.attributes.insert("attribution".into(),
                    AttributeValue::String(metadata.positional_attributes.remove(0).trim().to_string()));
            }
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let content_location = state.create_block_location(content_start, content_end, offset);

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
                    document_parser::blocks(content, state, content_start+offset, block_metadata.parent_section_level).unwrap_or_else(|e| {
                        adjust_and_log_parse_error(&e, content, content_start+offset, state, "Error parsing content as blocks in quote block");
                        Ok(Vec::new())
                    })?
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
        = "toc::" attributes:attributes() end:position!()
        {
            let (_discrete, metadata_from_attributes, _title_position) = attributes;
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            metadata.move_positional_attributes_to_attributes();
            tracing::info!("Found Table of Contents block");
            Ok(Block::TableOfContents(TableOfContents {
                metadata,
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
                metadata.attributes.insert("alt".into(), AttributeValue::String(style.clone()));
            }
            if metadata.positional_attributes.len() >= 2 {
                metadata.attributes.insert("height".into(), AttributeValue::String(metadata.positional_attributes.remove(1)));
            }
            if !metadata.positional_attributes.is_empty() {
                metadata.attributes.insert("width".into(), AttributeValue::String(metadata.positional_attributes.remove(0)));
            }
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Image(Image {
                title,
                source,
                metadata,
                location: state.create_block_location(start, end, offset),

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
                location: state.create_block_location(start, end, offset),
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
                    metadata.attributes.insert(style.clone(), AttributeValue::Bool(true));
                } else {
                    // assume poster
                    tracing::debug!(?metadata, "transforming video metadata style into attribute, assuming poster");
                    metadata.attributes.insert("poster".into(), AttributeValue::String(style.clone()));
                }
            }
            if metadata.positional_attributes.len() >= 2 {
                metadata.attributes.insert("height".into(), AttributeValue::String(metadata.positional_attributes.remove(1)));
            }
            if !metadata.positional_attributes.is_empty() {
                metadata.attributes.insert("width".into(), AttributeValue::String(metadata.positional_attributes.remove(0)));
            }
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Video(Video {
                title,
                sources,
                metadata,
                location: state.create_block_location(start, end, offset),
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
                title: block_metadata.title.clone(),
                location: state.create_block_location(start, end, offset),
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
        = list_with_continuation(start, offset, block_metadata, true)

        // Parameterized list rule - allow_continuation controls whether list items can consume
        // explicit continuations. Set to false when parsing lists inside continuation blocks
        // to prevent nested lists from consuming parent-level continuations.
        rule list_with_continuation(start: usize, offset: usize, block_metadata: &BlockParsingMetadata, allow_continuation: bool) -> Result<Block, Error>
        = callout_list(start, offset, block_metadata)
        / unordered_list(start, offset, block_metadata, false, allow_continuation)
        / ordered_list(start, offset, block_metadata, false, allow_continuation)
        / description_list(start, offset, block_metadata)

        rule unordered_list_marker() -> &'input str = $("*"+ / "-")

        rule ordered_list_marker() -> &'input str = $(digits()? "."+)

        rule description_list_marker() -> &'input str = $("::::" / ":::" / "::" / ";;")

        rule callout_list_marker() -> &'input str = $("<" (digits() / ".") ">")

        rule section_level_marker() -> &'input str = $(("=" / "#")+)

        // Helper rule to check if we're at the start of a new list item (lookahead)
        rule at_list_item_start() = whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace()

        // Helper rule to check if we're at the start of a section heading (lookahead)
        // This is used to terminate list continuations when a section follows
        rule at_section_start() = ("=" / "#")+ " "

        // Helper rule to check if we're at an ordered list marker ahead (after newlines)
        rule at_ordered_marker_ahead() = eol()+ whitespace()* ordered_list_marker()

        // Helper rule to check if we're at an unordered list marker ahead (after newlines)
        rule at_unordered_marker_ahead() = eol()+ whitespace()* unordered_list_marker()

        // Helper rule to check if we're at a root-level (non-indented) ordered marker (current position)
        rule at_root_ordered_marker() = !whitespace() ordered_list_marker()

        // Helper rule to check if we're at a root-level (non-indented) unordered marker (current position)
        rule at_root_unordered_marker() = !whitespace() unordered_list_marker()

        // Helper rule to check if we're at a shallower unordered marker
        // Used to terminate nested lists when a blank line precedes a shallower item
        // Same-level markers continue the list as siblings; only shallower markers end it
        rule at_shallower_unordered_marker(base_marker: &str)
        = whitespace()* marker:unordered_list_marker() whitespace() {?
            if marker.len() < base_marker.len() { Ok(()) } else { Err("same-or-deeper") }
        }

        // Helper rule to check if we're at a shallower ordered marker
        // Used to terminate nested lists when a blank line precedes a shallower item
        // Same-level markers continue the list as siblings; only shallower markers end it
        rule at_shallower_ordered_marker(base_marker: &str)
        = whitespace()* marker:ordered_list_marker() whitespace() {?
            if marker.len() < base_marker.len() { Ok(()) } else { Err("same-or-deeper") }
        }

        // Helper rule to check if we're at a list separator (forces list termination)
        // Matches either a line comment (//) or empty block attributes ([]) on their own line
        // Note: Separator must be preceded by at least one blank line (2+ newlines)
        // Without a blank line before it, a comment is just skipped, not a separator
        rule at_list_separator()
        = eol()*<2,> at_list_separator_content()

        // Helper rule to check for separator content at current position (no leading newlines)
        // Used by continuation_lines to stop at separators
        rule at_list_separator_content()
        = "//" [^'\n']* (&eol() / ![_])  // Line comment separator
        / whitespace()* "[" whitespace()* "]" whitespace()* (&eol() / ![_])  // Empty block attributes

        rule unordered_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata, parent_is_ordered: bool, allow_continuation: bool) -> Result<Block, Error>
        // Parse whitespace + marker first to capture base_marker for rest items
        // marker_start captures position before marker for correct first item location
        = whitespace()* marker_start:position!() base_marker:$(unordered_list_marker()) &whitespace()
        first:unordered_list_item_after_marker(offset, block_metadata, allow_continuation, base_marker, marker_start)
        rest:(unordered_list_rest_item(offset, block_metadata, parent_is_ordered, allow_continuation, base_marker))*
        end:position!()
        {
            tracing::info!("Found unordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(end, |(_, item_end)| *item_end);
            let items: Vec<ListItem> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or(String::new(), |item| item.marker.clone());

            Ok(Block::UnorderedList(UnorderedList {
                title: block_metadata.title.clone(),
                metadata: block_metadata.metadata.clone(),
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        // Parse first item content after marker has been consumed by unordered_list
        // marker_start is the position where the marker began, for correct location tracking
        rule unordered_list_item_after_marker(offset: usize, block_metadata: &BlockParsingMetadata, allow_continuation: bool, marker: &'input str, marker_start: usize) -> Result<(ListItem, usize), Error>
        = item:unordered_list_item_with_continuation_after_marker(offset, block_metadata, marker, marker_start) {? if allow_continuation { Ok(item) } else { Err("skip") } }
        / item:unordered_list_item_no_continuation_after_marker(offset, block_metadata, marker, marker_start) { item }

        rule unordered_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata, parent_is_ordered: bool, allow_continuation: bool, base_marker: &str) -> Result<(ListItem, usize), Error>
        // Case 1: No blank lines (directly at content) - accept any depth for consecutive items
        //
        // OPTIMIZATION: `!at_ordered_marker_ahead()` (only in parent_is_ordered=true branch)
        // This lookahead is purely for performance - it fails fast when an ordered marker (`.`)
        // appears after nested unordered items. Without it, `unordered_list_item` would attempt
        // to parse the ordered marker, fail, and backtrack - same result, just slower.
        // See fixtures: nested_unordered_in_ordered.adoc, nested_ordered_in_unordered.adoc
        = !at_list_separator() !eol() comment_line()* !at_ordered_marker_ahead() item:unordered_list_item(offset, block_metadata, allow_continuation)
        {?
            if parent_is_ordered {
                Ok(item)
            } else {
                Err("skip")
            }
        }
        / !at_list_separator() !eol() comment_line()* item:unordered_list_item(offset, block_metadata, allow_continuation)
        {?
            if parent_is_ordered {
                Err("skip")
            } else {
                Ok(item)
            }
        }
        // Case 2: Blank lines present (at newline(s)) - reject shallower markers (they belong to parent list)
        / !at_list_separator() eol()+ comment_line()* !at_shallower_unordered_marker(base_marker) !at_ordered_marker_ahead() item:unordered_list_item(offset, block_metadata, allow_continuation)
        {?
            if parent_is_ordered {
                Ok(item)
            } else {
                Err("skip")
            }
        }
        / !at_list_separator() eol()+ comment_line()* !at_shallower_unordered_marker(base_marker) item:unordered_list_item(offset, block_metadata, allow_continuation)
        {?
            if parent_is_ordered {
                Err("skip")
            } else {
                Ok(item)
            }
        }

        rule ordered_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata, parent_is_ordered: bool, allow_continuation: bool) -> Result<Block, Error>
        // Parse whitespace + marker first to capture base_marker for rest items
        // marker_start captures position before marker for correct first item location
        = whitespace()* marker_start:position!() base_marker:$(ordered_list_marker()) &whitespace()
        first:ordered_list_item_after_marker(offset, block_metadata, allow_continuation, base_marker, marker_start)
        rest:(ordered_list_rest_item(offset, block_metadata, parent_is_ordered, allow_continuation, base_marker))*
        end:position!()
        {
            tracing::info!("Found ordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
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

        // Parse first item content after marker has been consumed by ordered_list
        // marker_start is the position where the marker began, for correct location tracking
        rule ordered_list_item_after_marker(offset: usize, block_metadata: &BlockParsingMetadata, allow_continuation: bool, marker: &'input str, marker_start: usize) -> Result<(ListItem, usize), Error>
        = item:ordered_list_item_with_continuation_after_marker(offset, block_metadata, marker, marker_start) {? if allow_continuation { Ok(item) } else { Err("skip") } }
        / item:ordered_list_item_no_continuation_after_marker(offset, block_metadata, marker, marker_start) { item }

        rule ordered_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata, parent_is_ordered: bool, allow_continuation: bool, base_marker: &str) -> Result<(ListItem, usize), Error>
        // Case 1: No blank lines (directly at content) - accept any depth for consecutive items
        //
        // OPTIMIZATION: `!at_unordered_marker_ahead()` (only in parent_is_ordered=false branch)
        // This lookahead is purely for performance - it fails fast when an unordered marker (`*`)
        // appears after nested ordered items. Without it, `ordered_list_item` would attempt
        // to parse the unordered marker, fail, and backtrack - same result, just slower.
        // See fixtures: nested_unordered_in_ordered.adoc, nested_ordered_in_unordered.adoc
        = !at_list_separator() !eol() comment_line()* !at_unordered_marker_ahead() item:ordered_list_item(offset, block_metadata, allow_continuation)
        {?
            if parent_is_ordered {
                Ok(item)
            } else {
                Err("skip")
            }
        }
        / !at_list_separator() !eol() comment_line()* item:ordered_list_item(offset, block_metadata, allow_continuation)
        {?
            if parent_is_ordered {
                Err("skip")
            } else {
                Ok(item)
            }
        }
        // Case 2: Blank lines present (at newline(s)) - reject shallower markers (they belong to parent list)
        / !at_list_separator() eol()+ comment_line()* !at_shallower_ordered_marker(base_marker) !at_unordered_marker_ahead() item:ordered_list_item(offset, block_metadata, allow_continuation)
        {?
            if parent_is_ordered {
                Ok(item)
            } else {
                Err("skip")
            }
        }
        / !at_list_separator() eol()+ comment_line()* !at_shallower_ordered_marker(base_marker) item:ordered_list_item(offset, block_metadata, allow_continuation)
        {?
            if parent_is_ordered {
                Err("skip")
            } else {
                Ok(item)
            }
        }

        // Note: The `*_with_continuation` and `*_no_continuation` variants exist because
        // PEG parsers are greedy - nested items must NOT consume explicit continuations
        // that belong to their parent. Attempting to handle this in semantic actions
        // (by always parsing continuations then discarding them) would consume input
        // needed by the parent rule. This structural duplication is intentional.
        rule unordered_list_item(offset: usize, block_metadata: &BlockParsingMetadata, allow_continuation: bool) -> Result<(ListItem, usize), Error>
        = item:unordered_list_item_with_continuation(offset, block_metadata) {? if allow_continuation { Ok(item) } else { Err("skip") } }
        / item:unordered_list_item_no_continuation(offset, block_metadata) { item }

        rule unordered_list_item_with_continuation(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<(ListItem, usize), Error>
        = start:position!()
        whitespace()*
        marker:unordered_list_marker()
        whitespace()
        checked:checklist_item()?
        first_line_start:position()
        // Parse first line (principal text)
        first_line:$((!(eol()) [_])*)
        // Parse continuation lines that are part of the same paragraph
        // Stop at: blank line, list item start, explicit continuation marker, section heading, or list separator
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        // Try to parse nested ordered list (only if followed by newline)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Nested items cannot consume parent-level continuations (allow_continuation: false)
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for explicit_continuation
        nested:(!at_list_separator() eol()+ nested_content:unordered_list_item_nested_content(offset, block_metadata) { nested_content })?
        // Try to parse explicit continuations (+ marker)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Parent items accept both:
        // - Immediate continuations (0 empty lines) for content directly after principal text
        // - Ancestor continuations (1+ empty lines) for content that bubbles up from nested items
        // Use * to match a mixed sequence of immediate and ancestor continuations
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        end:position!()
        {
            tracing::info!(%first_line, ?continuation_lines, %marker, ?checked, "found unordered list item");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text = assemble_principal_text(first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            // Process principal text as inline nodes
            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, &first_line_start, first_line_end, offset, &principal_text)?
            };

            let mut blocks = Vec::new();
            // nested_content is no longer optional in the grammar, so one less Some level
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            // Collect all continuation blocks (each is a Result<Block, Error>)
            blocks.extend(explicit_continuations.into_iter().flatten());

            // Use end position after all blocks if we have any, otherwise use item_end
            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker: marker.to_string(),
                checked,
                location: state.create_location(start+offset, actual_end+offset),
            }, actual_end))
        }

        // Version with immediate continuations only (for nested items)
        // Nested items consume continuations with 0 empty lines (immediate attachment).
        // Continuations with 1+ empty lines bubble up to ancestor items.
        rule unordered_list_item_no_continuation(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<(ListItem, usize), Error>
        = start:position!()
        whitespace()*
        marker:unordered_list_marker()
        whitespace()
        checked:checklist_item()?
        first_line_start:position()
        first_line:$((!(eol()) [_])*)
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        // Nested items can still have nested lists, but those also cannot consume parent continuations
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for immediate_continuation
        nested:(!at_list_separator() eol()+ nested_content:unordered_list_item_nested_content(offset, block_metadata) { nested_content })?
        // Parse immediate continuations (0 empty lines) - these attach to this item
        // Ancestor continuations (1+ empty lines) bubble up to parent items
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        end:position!()
        {
            tracing::info!(%first_line, ?continuation_lines, %marker, ?checked, "found unordered list item (immediate continuation only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text = assemble_principal_text(first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, &first_line_start, first_line_end, offset, &principal_text)?
            };

            let mut blocks = Vec::new();
            // nested_content is no longer optional in the grammar, so one less Some level
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            // Collect all immediate continuation blocks
            blocks.extend(immediate_continuations.into_iter().flatten());

            // Use end position after all blocks if we have any, otherwise use item_end
            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker: marker.to_string(),
                checked,
                location: state.create_location(start+offset, actual_end+offset),
            }, actual_end))
        }

        // After-marker variants: used when marker has already been consumed by parent rule
        // These are identical to the regular variants except they take marker as a parameter
        // instead of parsing it, and start after the marker position
        rule unordered_list_item_with_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata, marker: &'input str, marker_start: usize) -> Result<(ListItem, usize), Error>
        = start:position!()
        whitespace()
        checked:checklist_item()?
        first_line_start:position()
        first_line:$((!(eol()) [_])*)
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        nested:(!at_list_separator() eol()+ nested_content:unordered_list_item_nested_content(offset, block_metadata) { nested_content })?
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        end:position!()
        {
            tracing::info!(%first_line, ?continuation_lines, %marker, ?checked, "found unordered list item (after marker)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text = assemble_principal_text(first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, &first_line_start, first_line_end, offset, &principal_text)?
            };

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker: marker.to_string(),
                checked,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        rule unordered_list_item_no_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata, marker: &'input str, marker_start: usize) -> Result<(ListItem, usize), Error>
        = start:position!()
        whitespace()
        checked:checklist_item()?
        first_line_start:position()
        first_line:$((!(eol()) [_])*)
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        nested:(!at_list_separator() eol()+ nested_content:unordered_list_item_nested_content(offset, block_metadata) { nested_content })?
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        end:position!()
        {
            tracing::info!(%first_line, ?continuation_lines, %marker, ?checked, "found unordered list item (after marker, immediate only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text = assemble_principal_text(first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, &first_line_start, first_line_end, offset, &principal_text)?
            };

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker: marker.to_string(),
                checked,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        /// Parse nested content within an unordered list item (e.g., nested ordered list)
        /// Note: allow_continuation is false to prevent nested items from consuming parent-level continuations
        rule unordered_list_item_nested_content(offset: usize, block_metadata: &BlockParsingMetadata) -> Option<Result<Block, Error>>
        // FUNCTIONAL: !at_root_ordered_marker() prevents root-level ordered items (no leading whitespace)
        // from being incorrectly parsed as nested. Without this, `. item` at column 1 would be nested
        // inside the parent unordered item instead of being a sibling list.
        = !at_root_ordered_marker() nested_start:position!() list:ordered_list(nested_start, offset, block_metadata, true, false) {
            Some(list)
        }

        // See comment on unordered_list_item for why *_with/without_continuation variants exist.
        rule ordered_list_item(offset: usize, block_metadata: &BlockParsingMetadata, allow_continuation: bool) -> Result<(ListItem, usize), Error>
        = item:ordered_list_item_with_continuation(offset, block_metadata) {? if allow_continuation { Ok(item) } else { Err("skip") } }
        / item:ordered_list_item_no_continuation(offset, block_metadata) { item }

        rule ordered_list_item_with_continuation(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<(ListItem, usize), Error>
        = start:position!()
        whitespace()*
        marker:ordered_list_marker()
        whitespace()
        checked:checklist_item()?
        first_line_start:position()
        // Parse first line (principal text)
        first_line:$((!(eol()) [_])*)
        // Parse continuation lines that are part of the same paragraph
        // Stop at: blank line, list item start, explicit continuation marker, section heading, or list separator
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        // Try to parse nested unordered list (only if followed by newline)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Nested items cannot consume parent-level continuations (allow_continuation: false)
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for explicit_continuation
        nested:(!at_list_separator() eol()+ nested_content:ordered_list_item_nested_content(offset, block_metadata) { nested_content })?
        // Try to parse explicit continuations (+ marker)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Parent items accept both:
        // - Immediate continuations (0 empty lines) for content directly after principal text
        // - Ancestor continuations (1+ empty lines) for content that bubbles up from nested items
        // Use * to match a mixed sequence of immediate and ancestor continuations
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        end:position!()
        {
            tracing::info!(%first_line, ?continuation_lines, %marker, ?checked, "found ordered list item");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text = assemble_principal_text(first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            // Process principal text as inline nodes
            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, &first_line_start, first_line_end, offset, &principal_text)?
            };

            let mut blocks = Vec::new();
            // nested_content is no longer optional in the grammar, so one less Some level
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            // Collect all continuation blocks (each is a Result<Block, Error>)
            blocks.extend(explicit_continuations.into_iter().flatten());

            // Use end position after all blocks if we have any, otherwise use item_end
            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker: marker.to_string(),
                checked,
                location: state.create_location(start+offset, actual_end+offset),
            }, actual_end))
        }

        // Version with immediate continuations only (for nested items)
        // Nested items consume continuations with 0 empty lines (immediate attachment).
        // Continuations with 1+ empty lines bubble up to ancestor items.
        rule ordered_list_item_no_continuation(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<(ListItem, usize), Error>
        = start:position!()
        whitespace()*
        marker:ordered_list_marker()
        whitespace()
        checked:checklist_item()?
        first_line_start:position()
        first_line:$((!(eol()) [_])*)
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        // Nested items can still have nested lists, but those also cannot consume parent continuations
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for immediate_continuation
        nested:(!at_list_separator() eol()+ nested_content:ordered_list_item_nested_content(offset, block_metadata) { nested_content })?
        // Parse immediate continuations (0 empty lines) - these attach to this item
        // Ancestor continuations (1+ empty lines) bubble up to parent items
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        end:position!()
        {
            tracing::info!(%first_line, ?continuation_lines, %marker, ?checked, "found ordered list item (immediate continuation only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text = assemble_principal_text(first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, &first_line_start, first_line_end, offset, &principal_text)?
            };

            let mut blocks = Vec::new();
            // nested_content is no longer optional in the grammar, so one less Some level
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            // Collect all immediate continuation blocks
            blocks.extend(immediate_continuations.into_iter().flatten());

            // Use end position after all blocks if we have any, otherwise use item_end
            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker: marker.to_string(),
                checked,
                location: state.create_location(start+offset, actual_end+offset),
            }, actual_end))
        }

        // After-marker variants for ordered lists: used when marker has already been consumed by parent rule
        rule ordered_list_item_with_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata, marker: &'input str, marker_start: usize) -> Result<(ListItem, usize), Error>
        = start:position!()
        whitespace()
        checked:checklist_item()?
        first_line_start:position()
        first_line:$((!(eol()) [_])*)
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        nested:(!at_list_separator() eol()+ nested_content:ordered_list_item_nested_content(offset, block_metadata) { nested_content })?
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        end:position!()
        {
            tracing::info!(%first_line, ?continuation_lines, %marker, ?checked, "found ordered list item (after marker)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text = assemble_principal_text(first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, &first_line_start, first_line_end, offset, &principal_text)?
            };

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker: marker.to_string(),
                checked,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        rule ordered_list_item_no_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata, marker: &'input str, marker_start: usize) -> Result<(ListItem, usize), Error>
        = start:position!()
        whitespace()
        checked:checklist_item()?
        first_line_start:position()
        first_line:$((!(eol()) [_])*)
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        nested:(!at_list_separator() eol()+ nested_content:ordered_list_item_nested_content(offset, block_metadata) { nested_content })?
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        end:position!()
        {
            tracing::info!(%first_line, ?continuation_lines, %marker, ?checked, "found ordered list item (after marker, immediate only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text = assemble_principal_text(first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, &first_line_start, first_line_end, offset, &principal_text)?
            };

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker: marker.to_string(),
                checked,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        /// Parse nested content within an ordered list item (e.g., nested unordered list)
        /// Note: allow_continuation is false to prevent nested items from consuming parent-level continuations
        rule ordered_list_item_nested_content(offset: usize, block_metadata: &BlockParsingMetadata) -> Option<Result<Block, Error>>
        // FUNCTIONAL: !at_root_unordered_marker() prevents root-level unordered items (no leading whitespace)
        // from being incorrectly parsed as nested. Without this, `* item` at column 1 would be nested
        // inside the parent ordered item instead of being a sibling list.
        = !at_root_unordered_marker() nested_start:position!() list:unordered_list(nested_start, offset, block_metadata, true, false) {
            Some(list)
        }

        /// Predicate rule that succeeds when we're NOT after a verbatim block
        /// Used with negative lookahead to ensure callout lists only match after verbatim blocks
        rule not_after_verbatim_block() -> ()
        = {?
            if state.last_block_was_verbatim {
                Err("is_after_verbatim")
            } else {
                Ok(())
            }
        }

        rule callout_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        // !not_after_verbatim_block(): FUNCTIONAL - callout lists only make sense after source/listing blocks
        // The double negative succeeds only when last_block_was_verbatim is true
        = !not_after_verbatim_block()
        // OPTIMIZATION: This positive lookahead fails fast when not at a callout marker (<1>, <.>, etc.)
        // Without it, callout_list_item would be called and fail - same result, just slower
        &(whitespace()* callout_list_marker() whitespace())
        first:callout_list_item(offset, block_metadata)
        rest:(callout_list_rest_item(offset, block_metadata))*
        end:position!()
        {
            tracing::info!("Found callout list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(end, |(_, item_end)| *item_end);
            let mut items: Vec<ListItem> = content.into_iter().map(|(item, _)| item).collect();

            // Resolve auto-numbered callout markers (<.>)
            let mut auto_number = 1;
            for item in &mut items {
                if item.marker == "<.>" {
                    item.marker = format!("<{auto_number}>");
                    auto_number += 1;
                }
            }

            // Validate callout list items
            let mut expected_number = 1;
            for item in &items {
                // Extract the number from the marker (e.g., "<5>" -> 5)
                if let Some(actual_number) = extract_callout_number(&item.marker) {
                    let file_name = state.current_file.as_ref()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    let line = item.location.start.line;

                    // Check sequential order
                    if actual_number != expected_number {
                        tracing::warn!(
                            "{file_name}: line {line}: callout list item index: expected {expected_number}, got {actual_number}"
                        );
                    }

                    // Check if the EXPECTED callout exists in the verbatim block
                    // (This warns when sequence is broken and the expected number is missing)
                    if !state.last_verbatim_callouts.contains(&expected_number) {
                        tracing::warn!(
                            "{file_name}: line {line}: no callout found for <{expected_number}>"
                        );
                    }

                    expected_number += 1;
                }
            }

            // Reset the flag after successfully parsing the callout list
            state.last_block_was_verbatim = false;
            state.last_verbatim_callouts.clear();

            Ok(Block::CalloutList(CalloutList {
                title: block_metadata.title.clone(),
                metadata: block_metadata.metadata.clone(),
                items,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        rule callout_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<(ListItem, usize), Error>
        = eol()+ item:callout_list_item(offset, block_metadata)
        {?
            Ok(item)
        }

        rule callout_list_item(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<(ListItem, usize), Error>
        = start:position!()
        whitespace()*
        marker:callout_list_marker()
        whitespace()
        first_line_start:position()
        // Parse first line (principal text)
        first_line:$((!(eol()) [_])*)
        // Parse continuation lines that are part of the same paragraph
        continuation_lines:(
            eol()
            !(whitespace()* (callout_list_marker() / unordered_list_marker() / ordered_list_marker() / eol()))
            line:$((!(eol()) [_])*)
            { line }
        )*
        first_line_end:position!()
        {
            // Callout lists are always at level 1 (they don't nest)
            let level: ListLevel = 1;

            // Combine first line and continuation lines
            let principal_text = if continuation_lines.is_empty() {
                first_line.to_string()
            } else {
                let mut text = first_line.to_string();
                for cont_line in continuation_lines {
                    text.push('\n');
                    text.push_str(cont_line);
                }
                text
            };

            // Calculate the actual end position for the principal text
            let content_end = if principal_text.is_empty() {
                first_line_end
            } else {
                first_line_end.saturating_sub(1)
            };

            // The end position for the list item should be at the last character of content
            let item_end = if principal_text.is_empty() {
                start
            } else {
                first_line_end.saturating_sub(1)
            };

            // Process principal text as inline nodes
            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, &first_line_start, first_line_end, offset, &principal_text)?
            };

            // For callout lists, we don't support nested content or attached blocks
            let blocks = vec![];

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker: marker.to_string(),
                checked: None,
                location: state.create_location(start+offset, item_end+offset),
            }, item_end))
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

            // Calculate actual end from last attached block, or fall back to end of principal/term
            // Note: end:position!() captures position after consuming blank lines looking for more
            // continuations, which ends up at the start of the next item. We need the actual content end.
            let actual_end = description.last().map_or_else(
                || {
                    // No attached content: use end of principal text line
                    if principal_content.is_empty() {
                        // Just term + delimiter
                        principal_start
                    } else {
                        principal_start + principal_content.len()
                    }
                },
                |b| {
                    let loc = b.location();
                    loc.absolute_end - offset
                },
            );

            Ok(DescriptionListItem {
                anchors: vec![],
                term,
                delimiter: delimiter.to_string(),
                principal_text,
                description,
                location: state.create_location(start+offset, actual_end+offset),
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
        &(whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace())
        list_start:position!()
        list:(unordered_list(list_start, offset, block_metadata, false, true) / ordered_list(list_start, offset, block_metadata, false, true))
        {
            tracing::info!("Auto-attaching list to description list item");
            Ok(vec![list?])
        }

        // Parse one or more explicit continuations for description lists
        // Same pattern as list_explicit_continuation: + marker followed by a single block
        // Uses block_in_continuation to prevent lists inside continuations from consuming
        // further continuations that belong to the parent item
        rule description_list_explicit_continuation(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Vec<Block>, Error>
        = continuations:(
            eol()* "+" eol()
            block:block_in_continuation(offset, block_metadata.parent_section_level)
            { block }
          )+
        {
            tracing::info!(count = continuations.len(), "Description list explicit continuation blocks");
            Ok(continuations.into_iter().filter_map(Result::ok).collect())
        }

        // Parse a single immediate continuation (0 empty lines before +)
        // These attach to the current (most recent) list item per AsciiDoc spec.
        // Uses block_in_continuation to prevent lists inside continuations from consuming
        // further continuations that belong to the parent item.
        // Pattern: exactly one newline before + (content\n+\nblock)
        rule list_explicit_continuation_immediate(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = eol() !eol() "+" eol()
          block:block_in_continuation(offset, block_metadata.parent_section_level)
        {
            tracing::info!("List immediate continuation block (0 empty lines)");
            block
        }

        // Parse a single ancestor continuation (1+ empty lines before +)
        // Per AsciiDoc spec: each empty line before + moves attachment up one nesting level.
        // 1 empty line = parent, 2 empty lines = grandparent, etc.
        // Uses block_in_continuation to prevent lists inside continuations from consuming
        // further continuations that belong to the parent item.
        // Pattern: two or more newlines before + (content\n\n+\nblock)
        rule list_explicit_continuation_ancestor(offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = eol() eol()+ "+" eol()
          block:block_in_continuation(offset, block_metadata.parent_section_level)
        {
            tracing::info!("List ancestor continuation block (1+ empty lines)");
            block
        }

        pub(crate) rule inlines(offset: usize, block_metadata: &BlockParsingMetadata) -> Vec<InlineNode>
        = (non_plain_text(offset, block_metadata) / plain_text(offset, block_metadata))+

        rule non_plain_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = inline:(
            inline_anchor:inline_anchor(offset) { inline_anchor }
            / cross_reference_shorthand:cross_reference_shorthand(offset) { cross_reference_shorthand }
            / cross_reference_macro:cross_reference_macro(offset) { cross_reference_macro }
            / hard_wrap:hard_wrap(offset) { hard_wrap }
            / &"footnote:" footnote:footnote(offset, block_metadata) { footnote }
            / stem:inline_stem(offset) { stem }
            / image:inline_image(offset, block_metadata) { image }
            / icon:inline_icon(offset, block_metadata) { icon }
            / keyboard:inline_keyboard(offset) { keyboard }
            / button:inline_button(offset) { button }
            / menu:inline_menu(offset) { menu }
            // mailto has to come before the url_macro because url_macro calls url() which
            // also matches against mailto:
            / mailto_macro:mailto_macro(offset, block_metadata) { mailto_macro }
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
                process_inlines_or_err!(
                    process_inlines(state, block_metadata, &content_start, end, offset, &content_str),
                    "could not process footnote content"
                )?
            };

            let mut footnote = Footnote {
                id: id.clone(),
                content,
                number: 0, // Will be set by register_footnote
                location: state.create_block_location(start, end, offset),
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
            (start, id, content_start, content.clone(), end)

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
                location: state.create_block_location(start, end, offset),
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
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule inline_button(offset: usize) -> InlineNode
        = start:position!()
        "btn:[" label:$balanced_bracket_content() "]" end:position!()
        {
            tracing::info!(?label, "Found button inline");
            InlineNode::Macro(InlineMacro::Button(Button {
                label: label.trim().to_string(),
                location: state.create_block_location(start, end, offset),
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
                location: state.create_block_location(start, end, offset),
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
        {?
            tracing::info!(?target, "Found url macro");
            let (text, attributes) = content;
            let mut metadata = BlockMetadata::default();
            for (k, v, _pos) in attributes {
                if let AttributeValue::String(v) = v {
                    metadata.attributes.insert(k, AttributeValue::String(v));
                }
            }
            let text = if let Some(text) = text {
                process_inlines(state, block_metadata, &start, end, offset, &text)
                    .map_err(|e| {
                        tracing::error!(?e, url_text = text, "could not process URL macro text");
                        "could not process URL macro text"
                    })?
            } else {
                vec![]
            };
            let target_source = Source::from_str(&target).map_err(|_| "failed to parse URL target")?;
            Ok(InlineNode::Macro(InlineMacro::Url(Url {
                text,
                target: target_source,
                attributes: metadata.attributes.clone(),
                location: state.create_block_location(start.offset, end, offset),
            })))
        }

        /// Parse `mailto:` macros with attribute handling.
        ///
        /// `mailto:` macros have the format: `mailto:joe@example.com[text,attr1=value1,attr2=value2]`
        ///
        /// This is similar to link macros but the `mailto:` is directly specified rather
        /// than using the `link:` prefix.
        rule mailto_macro(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position()
        &"mailto:"
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
        {?
            tracing::info!(?target, "Found mailto macro");
            let (text, attributes) = content;
            let mut metadata = BlockMetadata::default();
            for (k, v, _pos) in attributes {
                if let AttributeValue::String(v) = v {
                    metadata.attributes.insert(k, AttributeValue::String(v));
                }
            }
            let text = if let Some(text) = text {
                process_inlines(state, block_metadata, &start, end, offset, &text)
                    .map_err(|e| {
                        tracing::error!(?e, url_text = text, "could not process mailto macro text");
                        "could not process mailto macro text"
                    })?
            } else {
                vec![]
            };
            let target_source = Source::from_str(&target).map_err(|_| "failed to parse mailto target")?;
            Ok(InlineNode::Macro(InlineMacro::Mailto(Mailto {
                text,
                target: target_source,
                attributes: metadata.attributes.clone(),
                location: state.create_block_location(start.offset, end, offset),
            })))
        }

        rule inline_autolink(offset: usize) -> InlineNode
        = start:position!()
        url_info:(
            "<" url:url() ">" { (url, true) }
            / "<" url:email_address() ">" { (format!("mailto:{url}"), true) }
            / url:url() { (url, false) }
            / url:email_address() { (format!("mailto:{url}"), false) }
        )
        end:position!()
        {?
            let (url, bracketed) = url_info;
            tracing::info!(?url, bracketed, "Found autolink inline");
            let url_source = Source::from_str(&url).map_err(|_| "failed to parse autolink URL")?;
            Ok(InlineNode::Macro(InlineMacro::Autolink(Autolink {
                url: url_source,
                bracketed,
                location: state.create_block_location(start, end, offset),
            })))
        }

        rule inline_line_break(offset: usize) -> InlineNode
        = start:position!() " +" end:position!() eol()
        {?
            // Hard line break requires `text +` where text is actual content (non-whitespace)
            // When `+` appears indented at the start of a line (after newline + whitespace),
            // it should be treated as literal text, not a hard break.
            // See: https://github.com/nlopes/acdc/issues/234
            let absolute_pos = start + offset;
            let valid = absolute_pos > 0 && {
                let prev_byte_pos = absolute_pos.saturating_sub(1);
                state.input.as_bytes().get(prev_byte_pos).is_some_and(|&b| {
                    !b.is_ascii_whitespace()
                })
            };

            if !valid {
                return Err("hard line break requires preceding non-whitespace");
            }

            tracing::info!("Found inline line break");
            Ok(InlineNode::LineBreak(LineBreak {
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule hard_wrap(offset: usize) -> InlineNode
            = start:position!() " + \\" end:position!() &eol()
        {
            tracing::info!("Found hard wrap inline");
            InlineNode::LineBreak(LineBreak {
                location: state.create_block_location(start, end, offset),
            })
        }

        rule inline_icon(offset: usize, _block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position() "icon:" source:source() attributes:attributes() end:position!()
        {
            let (_discrete, metadata, _title_position) = attributes;
            let mut metadata = metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            // For font mode, the first positional (style) can be a size value (1x, 2x,
            // lg, fw) -> stored as "size" attribute;
            //
            // For image mode, the first positional (style) can be alt text.
            if let Some(ref style) = metadata.style {
                // Strip surrounding quotes if present (quoted positional attributes)
                let style_value = style.trim_matches('"');
                if ICON_SIZES.contains(&style_value) {
                    // Named size= attribute takes precedence over positional size so we
                    // insert rather than set (set overrides).
                    metadata.attributes.insert(
                        "size".to_string(),
                        AttributeValue::String(style_value.to_string()),
                    );
                } else {
                    // Other value become alt (fa-{value} in image mode)
                    metadata.attributes.set(
                        "alt".to_string(),
                        AttributeValue::String(style_value.to_string()),
                    );
                }
            }
            // Copy roles to attributes so they're accessible in the converter
            if !metadata.roles.is_empty() {
                metadata.attributes.set(
                    "role".to_string(),
                    AttributeValue::String(metadata.roles.join(" ")),
                );
            }
            InlineNode::Macro(InlineMacro::Icon(Icon {
                target: source,
                attributes: metadata.attributes.clone(),
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        rule inline_stem(offset: usize) -> InlineNode
        = start:position!() "stem:[" content:balanced_bracket_content() "]" end:position!()
        {
            // Get notation from :stem: document attribute
            let notation = match state.document_attributes.get_string("stem") {
                Some(s) => {
                    StemNotation::from_str(&s).unwrap_or(StemNotation::Asciimath)
                }
                _ => StemNotation::Asciimath,
            };

            InlineNode::Macro(InlineMacro::Stem(Stem {
                content,
                notation,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule inline_image(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position() "image:" source:source() attributes:attributes() end:position!()
        {?
            let (_discrete, metadata, title_position) = attributes;
            let mut metadata = metadata.clone();
            let mut title = Title::default();
            if let Some(style) = metadata.style {
                metadata.style = None; // Clear style to avoid confusion
                // For inline images, the first positional attribute is the alt text (title)
                title = Title::new(vec![InlineNode::PlainText(Plain {
                    content: style,
                    location: state.create_block_location(start.offset, end, offset),
                })]);
            }
            if metadata.positional_attributes.len() >= 2 {
                metadata.attributes.insert("height".into(), AttributeValue::String(metadata.positional_attributes.remove(1)));
            }
            if !metadata.positional_attributes.is_empty() {
                metadata.attributes.insert("width".into(), AttributeValue::String(metadata.positional_attributes.remove(0)));
            }
            metadata.move_positional_attributes_to_attributes();
            // For inline images, if there's no first positional (no alt text in title field),
            // check if there's a named title attribute. Only then should we use it to populate
            // the title field for rendering purposes, but we keep it in attributes for the
            // HTML title attribute (hover text).
            if title.is_empty()
                && let Some(AttributeValue::String(content)) = metadata.attributes.get("title")
                && let Some((title_start, title_end)) = title_position
            {
                // Use the captured position from the named_attribute rule
                let title_start_pos = PositionWithOffset {
                    offset: title_start,
                    position: state.line_map.offset_to_position(title_start, &state.input),
                };
                title = Title::new(process_inlines_or_err!(
                    process_inlines(state, block_metadata, &title_start_pos, title_end, offset, content),
                    "could not process title in inline image macro"
                )?);
            }
            // Note: We do NOT remove the title attribute - it's needed for the HTML title attribute

            Ok(InlineNode::Macro(InlineMacro::Image(Box::new(Image {
                title,
                source,
                metadata: metadata.clone(),
                location: state.create_block_location(start.offset, end, offset),

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
                location: state.create_block_location(start, end, offset),
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
                location: state.create_block_location(start, end, offset),
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
                location: state.create_block_location(start, end, offset),
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
            = attrs:inline_attributes()? start:position() "**" content_start:position() content:$((!(eol() / ![_] / "**") [_])+) "**" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found unconstrained bold text inline");
            let content = process_inlines_or_err!(
                process_inlines(state, block_metadata, &content_start, end - 2, offset, content),
                "could not process unconstrained bold text content"
            )?;
            Ok(InlineNode::BoldText(Bold {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        rule bold_text_constrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "*"
        content:$([^(' ' | '\t' | '\n')] [^'*']* ("*" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_]) [^'*']*)*)
        "*"
        end:position!() &([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_])
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + offset;
            let valid_boundary = absolute_pos == 0 || {
                let prev_byte_pos = absolute_pos.saturating_sub(1);
                state.input.as_bytes().get(prev_byte_pos).is_none_or(|&b| {
                    match_constrained_boundary(b)
                })
            };

            if !valid_boundary {
                tracing::debug!(absolute_pos, prev_byte = ?state.input.as_bytes().get(absolute_pos.saturating_sub(1)), "Invalid word boundary for constrained bold");
                return Err("invalid word boundary for constrained bold");
            }

            tracing::info!(?offset, ?content, ?role, "Found constrained bold text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let content = process_inlines_or_err!(
                process_inlines(state, block_metadata, &adjusted_content_start, end - 1, offset, content),
                "could not process constrained bold text content"
            )?;

            Ok(InlineNode::BoldText(Bold {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule italic_text_constrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "_"
        content:$([^(' ' | '\t' | '\n')] [^'_']* ("_" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_]) [^'_']*)*)
        "_"
        end:position!() &([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_])
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + offset;
            let valid_boundary = absolute_pos == 0 || {
                let prev_byte_pos = absolute_pos.saturating_sub(1);
                state.input.as_bytes().get(prev_byte_pos).is_none_or(|&b| {
                    match_constrained_boundary(b)
                })
            };

            if !valid_boundary {
                return Err("invalid word boundary for constrained italic");
            }

            tracing::info!(?offset, ?content, ?role, "Found constrained italic text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let content = process_inlines_or_err!(
                process_inlines(state, block_metadata, &adjusted_content_start, end - 1, offset, content),
                "could not process constrained italic text content"
            )?;
            Ok(InlineNode::ItalicText(Italic {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule bold_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        "*"
        [^(' ' | '\t' | '\n')]
        [^'*']*
        ("*" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_]) [^'*']*)*
        "*"
        ([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_])
        {?
            // Check if we're at start OR preceded by word boundary (no asterisk)
            let valid_boundary = boundary_pos == 0 || {
                let prev_byte_pos = boundary_pos.saturating_sub(1);
                state.input.as_bytes().get(prev_byte_pos).is_none_or(|&b| {
                    match_constrained_boundary(b)
                })
            };

            if valid_boundary { Ok(()) } else { Err("invalid word boundary") }
        }

        rule italic_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        "_"
        [^(' ' | '\t' | '\n')]
        [^'_']*
        ("_" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_]) [^'_']*)*
        "_"
        ([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_])
        {?
            // Check if we're at start OR preceded by word boundary (no underscore)
            let valid_boundary = boundary_pos == 0 || {
                let prev_byte_pos = boundary_pos.saturating_sub(1);
                state.input.as_bytes().get(prev_byte_pos).is_none_or(|&b| {
                    match_constrained_boundary(b)
                })
            };

            if valid_boundary { Ok(()) } else { Err("invalid word boundary") }
        }

        rule italic_text_unconstrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "__" content_start:position() content:$((!(eol() / ![_] / "__") [_])+) "__" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found unconstrained italic text inline");
            let content = process_inlines_or_err!(
                process_inlines(state, block_metadata, &content_start, end - 2, offset, content),
                "could not process unconstrained italic text content"
            )?;
            Ok(InlineNode::ItalicText(Italic {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        rule monospace_text_unconstrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "``" content_start:position() content:$((!(eol() / ![_] / "``") [_])+) "``" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found unconstrained monospace text inline");
            let content = process_inlines_or_err!(
                process_inlines(state, block_metadata, &content_start, end - 2, offset, content),
                "could not process unconstrained monospace text content"
            )?;
            Ok(InlineNode::MonospaceText(Monospace {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        rule monospace_text_constrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "`"
        content:$([^(' ' | '\t' | '\n')] [^'`']* ("`" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_]) [^'`']*)*)
        "`"
        end:position!()
        &([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_])
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + offset;
            let valid_boundary = absolute_pos == 0 || {
                let prev_byte_pos = absolute_pos.saturating_sub(1);
                state.input.as_bytes().get(prev_byte_pos).is_none_or(|&b| {
                    match_constrained_boundary(b)
                })
            };
            if !valid_boundary {
                return Err("monospace must be at word boundary");
            }
            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found constrained monospace text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let content = process_inlines_or_err!(
                process_inlines(state, block_metadata, &adjusted_content_start, end - 1, offset, content),
                "could not process constrained monospace text content"
            )?;
            Ok(InlineNode::MonospaceText(Monospace {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule monospace_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        "`"
        [^(' ' | '\t' | '\n')]
        [^'`']*
        ("`" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_]) [^'`']*)*
        "`"
        ([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_])
        {?
            // Check if we're at start OR preceded by word boundary (no backtick)
            let valid_boundary = boundary_pos == 0 || {
                let prev_byte_pos = boundary_pos.saturating_sub(1);
                state.input.as_bytes().get(prev_byte_pos).is_none_or(|&b| {
                    match_constrained_boundary(b)
                })
            };

            if !valid_boundary {
                return Err("monospace must be at word boundary");
            }
            Ok(())
        }

        rule highlight_text_unconstrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "##" content_start:position() content:$((!(eol() / ![_] / "##") [_])+) "##" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found unconstrained highlight text inline");
            let content = process_inlines_or_err!(
                process_inlines(state, block_metadata, &content_start, end - 2, offset, content),
                "could not process unconstrained highlight text content"
            )?;
            Ok(InlineNode::HighlightText(Highlight {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        rule highlight_text_constrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "#"
        content:$([^(' ' | '\t' | '\n')] [^'#']* ("#" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_]) [^'#']*)*)
        "#"
        end:position!()
        &([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_])
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + offset;
            let valid_boundary = absolute_pos == 0 || {
                let prev_byte_pos = absolute_pos.saturating_sub(1);
                state.input.as_bytes().get(prev_byte_pos).is_none_or(|&b| {
                    match_constrained_boundary(b)
                })
            };

            if !valid_boundary {
                tracing::debug!(absolute_pos, prev_byte = ?state.input.as_bytes().get(absolute_pos.saturating_sub(1)), "Invalid word boundary for constrained highlight");
                return Err("invalid word boundary for constrained highlight");
            }
            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found constrained highlight text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let content = process_inlines_or_err!(
                process_inlines(state, block_metadata, &adjusted_content_start, end - 1, offset, content),
                "could not process constrained highlight text content"
            )?;
            Ok(InlineNode::HighlightText(Highlight {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule highlight_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        "#"
        [^(' ' | '\t' | '\n')]
        [^'#']*
        ("#" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_]) [^'#']*)*
        "#"
        ([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | ')' | ']' | '}' | '/' | '-' | '<' | '>'] / ![_])
        {?
            // Check if we're at start OR preceded by word boundary (no hash)
            let valid_boundary = boundary_pos == 0 || {
                let prev_byte_pos = boundary_pos.saturating_sub(1);
                state.input.as_bytes().get(prev_byte_pos).is_none_or(|&b| {
                    match_constrained_boundary(b)
                })
            };

            if valid_boundary { Ok(()) } else { Err("invalid word boundary") }
        }

        /// Parse superscript text (^text^)
        rule superscript_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "^" content_start:position() content:$([^('^' | ' ' | '\t' | '\n')]+) "^" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found superscript text inline");
            let content = process_inlines_or_err!(
                process_inlines(state, block_metadata, &content_start, end - 1, offset, content),
                "could not process superscript text content"
            )?;
            Ok(InlineNode::SuperscriptText(Superscript {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        /// Parse subscript text (~text~)
        rule subscript_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "~" content_start:position() content:$([^('~' | ' ' | '\t' | '\n')]+) "~" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found subscript text inline");
            let content = process_inlines_or_err!(
                process_inlines(state, block_metadata, &content_start, end - 1, offset, content),
                "could not process subscript text content"
            )?;
            Ok(InlineNode::SubscriptText(Subscript {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        /// Parse curved quotation text (`"text"`)
        rule curved_quotation_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "\"`" content_start:position() content:$((!("`\"") [_])+) "`\"" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found curved quotation text inline");
            let content = process_inlines_or_err!(
                process_inlines(state, block_metadata, &content_start, end - 2, offset, content),
                "could not process curved quotation text content"
            )?;
            Ok(InlineNode::CurvedQuotationText(CurvedQuotation {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        /// Parse curved apostrophe text (`'text'`)
        rule curved_apostrophe_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "'`" content_start:position() content:$((!("`'") [_])+) "`'" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found curved apostrophe text inline");
            let content = process_inlines_or_err!(
                process_inlines(state, block_metadata, &content_start, end - 2, offset, content),
                "could not process curved apostrophe text content"
            )?;
            Ok(InlineNode::CurvedApostropheText(CurvedApostrophe {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        /// Parse standalone curved apostrophe (`')
        rule standalone_curved_apostrophe(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = start:position() "`'" end:position!()
        {?
            tracing::info!(?start, ?end, ?offset, "Found standalone curved apostrophe inline");
            Ok(InlineNode::StandaloneCurvedApostrophe(StandaloneCurvedApostrophe {
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        rule plain_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start_pos:position!()
        content:$((
            "\\" ['^' | '~']  // Escape sequences for superscript/subscript markers
            / (!(eol()*<2,> / ![_] / inline_anchor_match() / cross_reference_shorthand_match() / cross_reference_macro_match() / hard_wrap(offset) / footnote_match(offset, block_metadata) / inline_image(start_pos, block_metadata) / inline_icon(start_pos, block_metadata) / inline_stem(start_pos) / inline_keyboard(start_pos) / inline_button(start_pos) / inline_menu(start_pos) / mailto_macro(start_pos, block_metadata) / url_macro(start_pos, block_metadata) / inline_pass(start_pos) / link_macro(start_pos) / inline_autolink(start_pos) / inline_line_break(start_pos) / bold_text_unconstrained(start_pos, block_metadata) / bold_text_constrained_match() / italic_text_unconstrained(start_pos, block_metadata) / italic_text_constrained_match() / monospace_text_unconstrained(start_pos, block_metadata) / monospace_text_constrained_match() / highlight_text_unconstrained(start_pos, block_metadata) / highlight_text_constrained_match() / superscript_text(start_pos, block_metadata) / subscript_text(start_pos, block_metadata) / curved_quotation_text(start_pos, block_metadata) / curved_apostrophe_text(start_pos, block_metadata) / standalone_curved_apostrophe(start_pos, block_metadata)) [_])
        )+)
        end:position!()
        {
            tracing::info!(?content, "Found plain text inline");
            // Note: Backslash escape stripping (e.g., \^ -> ^) is handled by the converter,
            // not here, so that verbatim contexts (like monospace) preserve backslashes.
            InlineNode::PlainText(Plain {
                content: content.to_string(),
                location: state.create_block_location(start_pos, end, offset),
            })
        }

        /// Parse a quoted paragraph: "content" followed by `-- attribution[, citation]`
        ///
        /// This matches the AsciiDoc shorthand syntax for blockquotes:
        /// ```
        /// "I hold it that a little rebellion now and then is a good thing."
        /// -- Thomas Jefferson, Papers of Thomas Jefferson
        /// ```
        rule quoted_paragraph(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = content_start:position()
          "\"" quoted_content:$((!"\"" [_])+) "\""
          eol()
          "-- " attribution_line:$([^'\n']+)
          end:position!()
        {
            tracing::info!(?quoted_content, ?attribution_line, "found quoted paragraph");

            // Parse attribution line: "Author Name, Source Title" or just "Author Name"
            let (attribution, citation) = match attribution_line.split(',').collect::<Vec<_>>()[..] {
                [attr, cite] => (attr.trim().to_string(), Some(cite.trim().to_string())),
                [attr] => (attr.trim().to_string(), None),
                _ => {
                    tracing::warn!(?attribution_line, "attribution line has unexpected format, using full line as attribution");
                    (attribution_line.trim().to_string(), None)
                }
            };

            // Process the quoted content as inlines
            let (location, processed) = preprocess_inline_content(
                state,
                &content_start,
                end,
                offset,
                quoted_content,
            )?;
            let content = parse_inlines(&processed, state, block_metadata, &location)?;
            let content = map_inline_locations(state, &processed, &content, &location)?;

            // Build metadata with quote style and attribution
            let mut metadata = block_metadata.metadata.clone();
            metadata.style = Some("quote".to_string());
            metadata.attributes.insert(
                "attribution".into(),
                AttributeValue::String(attribution),
            );
            if let Some(cite) = citation {
                metadata.attributes.insert(
                    "citation".into(),
                    AttributeValue::String(cite),
                );
            }

            Ok(Block::Paragraph(Paragraph {
                content,
                metadata,
                title: block_metadata.title.clone(),
                location: state.create_block_location(start, end, offset),
            }))
        }

        /// Parse a markdown-style blockquote: lines starting with `> `
        ///
        /// This matches the Markdown-compatible syntax for blockquotes:
        /// ```
        /// > I hold it that a little rebellion now and then is a good thing,
        /// > and as necessary in the political world as storms in the physical.
        /// > -- Thomas Jefferson, Papers of Thomas Jefferson: Volume 11
        /// ```
        ///
        /// The content after `> ` on each line is joined and parsed as blocks.
        /// Attribution is extracted from a line matching `> -- Author[, Citation]`.
        rule markdown_blockquote(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = lines:markdown_blockquote_content_line()+ attribution:markdown_blockquote_attribution()? end:position!()
        {
            tracing::info!(?lines, ?attribution, "found markdown blockquote");

            let content = lines.join("\n");
            let content_start = start;

            // Build metadata with quote style and attribution
            let mut metadata = block_metadata.metadata.clone();
            metadata.style = Some("quote".to_string());
            if let Some((author, citation)) = attribution {
                metadata.attributes.insert(
                    "attribution".into(),
                    AttributeValue::String(author),
                );
                if let Some(cite) = citation {
                    metadata.attributes.insert(
                        "citation".into(),
                        AttributeValue::String(cite),
                    );
                }
            }

            let location = state.create_block_location(start, end, offset);

            // Parse the content as blocks
            let blocks = if content.trim().is_empty() {
                Vec::new()
            } else {
                document_parser::blocks(&content, state, content_start + offset, block_metadata.parent_section_level).unwrap_or_else(|e| {
                    adjust_and_log_parse_error(&e, &content, content_start + offset, state, "Error parsing content as blocks in markdown blockquote");
                    Ok(Vec::new())
                })?
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata,
                delimiter: ">".to_string(),
                inner: DelimitedBlockType::DelimitedQuote(blocks),
                title: block_metadata.title.clone(),
                location,
            }))
        }

        /// Match a content line of a markdown-style blockquote
        /// A line is content if:
        /// 1. It's followed by another `>` line (so `> -- ...` mid-blockquote is content)
        /// 2. OR it doesn't start with `-- ` (so it can't be attribution)
        rule markdown_blockquote_content_line() -> &'input str
        = "> " content:$([^'\n']*) eol() &">" { content }
        / "> " !("-- ") content:$([^'\n']*) (eol() / ![_]) { content }
        / ">" eol() &">" { "" }
        / ">" eol() { "" }
        / ">" ![_] { "" }

        /// Match an attribution line: `> -- Author[, Citation]`
        /// Only matches at the END of a blockquote (not followed by more `>` lines)
        /// Returns (author, Option<citation>)
        rule markdown_blockquote_attribution() -> (String, Option<String>)
        = "> -- " author:$([^(',' | '\n')]+) ", " citation:$([^'\n']+) ((eol() !">") / ![_]) {
            (author.trim().to_string(), Some(citation.trim().to_string()))
        }
        / "> -- " author:$([^'\n']+) ((eol() !">") / ![_]) {
            (author.trim().to_string(), None)
        }

        rule paragraph(start: usize, offset: usize, block_metadata: &BlockParsingMetadata) -> Result<Block, Error>
        = admonition:admonition()?
        content_start:position()
        content:$((!(
            eol()*<2,>
            / eol()* ![_]
            / eol() &attributes_line()
            / eol() example_delimiter()
            / eol() listing_delimiter()
            / eol() literal_delimiter()
            / eol() sidebar_delimiter()
            / eol() quote_delimiter()
            / eol() pass_delimiter()
            / eol() table_delimiter()
            / eol() markdown_code_delimiter()
            / eol() comment_delimiter()
            / eol() open_delimiter() &(whitespace()* eol())
            / eol() list(start, offset, block_metadata)
            / eol() &"+"  // Stop at list continuation marker
            / eol()* &(section_level_at_line_start(offset, None) (whitespace() / eol() / ![_]))
        ) [_])+)
        end:position!()
        {
            // Reset the verbatim flag since paragraph is not a verbatim block
            state.last_block_was_verbatim = false;

            // Check if this is a literal paragraph BEFORE preprocessing
            //
            // Literal paragraphs start with a space and should not have inline
            // preprocessing applied
            if content.starts_with(' ') {
                return Ok(get_literal_paragraph(state, content, start, end, offset, block_metadata));
            }

            let (location, processed) = preprocess_inline_content(state, &content_start, end, offset, content)?;
            let content = parse_inlines(&processed, state, block_metadata, &location)?;
            let content = map_inline_locations(state, &processed, &content, &location)?;

            // Title should either be an attribute named title, or the title parsed from the block metadata
            let title: Title = if let Some(AttributeValue::String(title)) = block_metadata.metadata.attributes.get("title") {
                vec![InlineNode::PlainText(Plain {
                    content: title.clone(),
                    location: state.create_location(start+offset, (start+offset).saturating_add(title.len()).saturating_sub(1)),
                })].into()
            } else {
                block_metadata.title.clone()
            };

            if let Some((variant, admonition_start, admonition_end)) = admonition {
                let Ok(parsed_variant) = AdmonitionVariant::from_str(&variant) else {
                    tracing::error!(%variant, "invalid admonition variant");
                    return Err(Error::InvalidAdmonitionVariant(
                        Box::new(create_source_location(state.create_location(admonition_start + offset, admonition_end + offset - 1), state.current_file.clone())),
                        variant
                    ));
                };
                tracing::info!(%variant, "found admonition block with variant");
                Ok(Block::Admonition(Admonition{
                    metadata: block_metadata.metadata.clone(),
                    title,
                    blocks: vec![Block::Paragraph(Paragraph {
                        content,
                        metadata: block_metadata.metadata.clone(),
                        title: Title::default(),
                        location: state.create_block_location(content_start.offset, end, offset),
                    })],
                    location: state.create_block_location(start, end, offset),
                    variant: parsed_variant,

                }))
            } else {
                // For quote/verse styled paragraphs, extract attribution/citation
                let mut metadata = block_metadata.metadata.clone();
                if let Some(ref style) = metadata.style
                    && (style == "quote" || style == "verse")
                {
                    // Extract attribution/citation from positional attributes
                    // [quote, attribution, citation] or [verse, attribution, citation]
                    if metadata.positional_attributes.len() >= 2 {
                        metadata.attributes.insert(
                            "citation".into(),
                            AttributeValue::String(metadata.positional_attributes.remove(1).trim().to_string()),
                        );
                    }
                    if !metadata.positional_attributes.is_empty() {
                        metadata.attributes.insert(
                            "attribution".into(),
                            AttributeValue::String(metadata.positional_attributes.remove(0).trim().to_string()),
                        );
                    }
                }
                metadata.move_positional_attributes_to_attributes();

                tracing::info!(?content, ?location, "found paragraph block");
                Ok(Block::Paragraph(Paragraph {
                    content,
                    metadata,
                    title,
                    location: state.create_block_location(start, end, offset),
                }))
            }
        }

        rule admonition() -> (String, usize, usize)
            = start:position!() variant:$("NOTE" / "WARNING" / "TIP" / "IMPORTANT" / "CAUTION") ": " end:position!()
        {
            (variant.to_string(), start, end)
        }

        // Lookahead rule that warns about anchor ID-like patterns containing whitespace.
        //
        // This uses negative lookahead and emits a warning if it detects whitespace. It
        // does not consume the input.
        rule warn_anchor_id_with_whitespace() -> ()
        = start:position!()
        &(
            id:$([^'\'' | ',' | ']' | '.' | '#']+)
            end:position!()
            {?
                if id.chars().any(char::is_whitespace) {
                    tracing::warn!(anchor_id = %id, location = ?state.create_location(start, end), "anchor id contains whitespace which is not allowed, treating as literal text");
                }
                // Always fail so the lookahead doesn't match - we just want the side
                // effect
                Err::<(), &'static str>("")
            }
        )

        rule anchor() -> Anchor
        = start:position!()
        result:(
            // Double-bracket [[id]] syntax - allows dots in ID since no role shorthand
            // possible.
            //
            // Whitespace is excluded per AsciiDoc documentation at
            // https://docs.asciidoctor.org/asciidoc/latest/attributes/id/#valid-id-characters
            double_open_square_bracket() warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | ' ' | '\t' | '\n' | '\r']+) comma() reftext:$([^']']+) double_close_square_bracket() {
                (id, Some(reftext))
            } /
            start:position!() double_open_square_bracket() warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | ' ' | '\t' | '\n' | '\r']+) double_close_square_bracket() {
                (id, None)
            } /
            // Single-bracket [#id] shorthand - exclude '.', '%' as they start role/option
            // shorthands
            //
            // Whitespace is excluded per AsciiDoc documentation at
            // https://docs.asciidoctor.org/asciidoc/latest/attributes/id/#valid-id-characters
            start:position!() open_square_bracket() "#" warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | '.' | '%' | ' ' | '\t' | '\n' | '\r']+) comma() reftext:$([^']']+) close_square_bracket() {
                (id, Some(reftext))
            } /
            start:position!() open_square_bracket() "#" warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | '.' | '%' | ' ' | '\t' | '\n' | '\r']+) close_square_bracket() {
                (id, None)
            }
        )
        end:position!()
        eol()
        {
            let (id, reftext) = result;
            Anchor {
                id: id.to_string(),
                xreflabel: reftext.map(ToString::to_string),
                location: state.create_location(start, end)
            }
        }

        rule inline_anchor(offset: usize) -> InlineNode
        = start:position!()
        double_open_square_bracket()
        // Whitespace is excluded - IDs must not contain spaces
        warn_anchor_id_with_whitespace()?
        id:$([^'\'' | ',' | ']' | '.' | ' ' | '\t' | '\n' | '\r']+)
        reftext:(
            comma() reftext:$([^']']+) {
                Some(reftext)
            } /
            {
                None
            }
        )
        double_close_square_bracket()
        end:position!()
        {
                InlineNode::InlineAnchor(Anchor {
                    id: id.to_string(),
                    xreflabel: reftext.map(ToString::to_string),
                    location: state.create_block_location(start, end, offset)
                })
            }

        rule inline_anchor_match() -> ()
        = double_open_square_bracket() [^'\'' | ',' | ']' | '.' | ' ' | '\t' | '\n' | '\r']+ (comma() [^']']+)? double_close_square_bracket()

        pub(crate) rule attributes_line() -> (bool, BlockMetadata)
            // FUNCTIONAL: Don't match empty [] followed by blank line - that's a list separator, not block attributes
            // Without this, `[]\n\n` would be parsed as an empty attributes line, breaking list separation
            = !empty_list_separator() attributes:attributes() eol() {
                let (discrete, metadata, _title_position) = attributes;
                (discrete, metadata)
            }

        // Empty brackets followed by a blank line is a list separator
        rule empty_list_separator()
            = whitespace()* "[" whitespace()* "]" whitespace()* eol() eol()

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
                        let values: Vec<String> = if v.starts_with('"') && v.ends_with('"') {
                            // Remove the quotes from the value, split by commas, and trim whitespace
                            v[1..v.len()-1].split(',').map(|s| s.trim().to_string()).collect()
                        } else {
                            vec![v.clone()]
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
                                metadata.attributes.insert(k.clone(), AttributeValue::String(v));
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
                        metadata.attributes.insert(k.clone(), AttributeValue::String(v));
                    } else if v == AttributeValue::None && pos.is_none() {
                        metadata.positional_attributes.push(k);
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

        /// Role pattern for inline contexts - allows % as literal character
        rule inline_role() -> &'input str = $([^(',' | ']' | '#' | '.')]+)

        /// Bare role pattern for inline contexts (no prefix) - matches CSS-like identifiers
        /// Starts with letter, followed by letters, numbers, or hyphens
        /// Used for syntax like [line-through]#text# (asciidoctor compatibility)
        rule bare_inline_role() -> &'input str = $(['a'..='z' | 'A'..='Z'] ['a'..='z' | 'A'..='Z' | '0'..='9' | '-']*)

        /// ID pattern for inline contexts - allows % as literal character
        rule inline_id() -> &'input str = $(id_start_char() inline_id_subsequent_char()*)
        rule inline_id_subsequent_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-' | '%']

        /// Parse a single attribute shorthand: .role, #id, or %option
        /// Used by block_style() for block-level attributes
        rule shorthand() -> Shorthand
        = "#" id:block_style_id() { Shorthand::Id(id.to_string()) }
        / "." role:role() { Shorthand::Role(role.to_string()) }
        / "%" option:option() { Shorthand::Option(option.to_string()) }

        /// Parse inline attribute shorthand: .role, #id, %role, or bare role
        /// In inline context, % is not an option separator - it's a literal character
        /// Leading % is treated as part of the role name
        /// Bare roles (no prefix) are supported for asciidoctor compatibility
        rule inline_shorthand() -> Shorthand
        = "#" id:inline_id() { Shorthand::Id(id.to_string()) }
        / "." role:inline_role() { Shorthand::Role(role.to_string()) }
        / "%" role:inline_role() { Shorthand::Role(format!("%{role}")) }
        / role:bare_inline_role() { Shorthand::Role(role.to_string()) }

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
            = whitespace()* att:named_attribute() { att }
              / att:positional_attribute_value() {
                  let substituted = String::substitute_attributes(&att, &state.document_attributes);
                  Some((substituted, AttributeValue::None, None))
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
                {
                    let substituted_value = String::substitute_attributes(&value, &state.document_attributes);
                    Some((name.to_string(), AttributeValue::String(substituted_value), Some((start, end))))
                }

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
                    "#" id_start:position!() id:block_style_id() id_end:position!() {
                        (Shorthand::Id(id.to_string()), Some((id_start, id_end)))
                    }
                    / s:shorthand() { (s, None) }
                )+ {
                    (Some(style), shorthands)
                } /
                style:positional_attribute_value() !"=" {
                    tracing::info!(%style, "Found block style without shorthands");
                    (Some(style), Vec::new())
                } /
                shorthands:(
                    "#" id_start:position!() id:block_style_id() id_end:position!() {
                        (Shorthand::Id(id.to_string()), Some((id_start, id_end)))
                    }
                    / s:shorthand() { (s, None) }
                )+ {
                    (None, shorthands)
               }
            )
            end:position!() {
                let (style, shorthands) = content;
                let mut maybe_anchor = None;
                let mut roles = Vec::new();
                let mut options = Vec::new();
                for (shorthand, pos) in shorthands {
                    match shorthand {
                        Shorthand::Id(id) => {
                            let (id_start, id_end) = pos.unwrap_or((start, end));
                            maybe_anchor = Some(Anchor {
                                id,
                                xreflabel: None,
                                location: state.create_location(id_start, id_end)
                            });
                        },
                        Shorthand::Role(role) => roles.push(role),
                        Shorthand::Option(option) => options.push(option),
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
            // Strip surrounding quotes from quoted values
            let trimmed = inner.trim_matches('"');
            tracing::debug!(%inner, %trimmed, "Found named attribute value (inner)");
            trimmed.to_string()
        }
        / s:$([^(',' | '"' | ']')]+)
        {
            tracing::debug!(%s, "Found named attribute value");
            s.to_string()
        }

        rule positional_attribute_value() -> String
        = quoted:inner_attribute_value() {
            let trimmed = quoted.trim_matches('"');
            tracing::debug!(quoted, trimmed, "Found quoted positional attribute value");
            trimmed.to_string()
        }
        / s:$([^('"' | ',' | ']' | '#' | '.' | '%')] [^(',' | ']' | '#' | '.' | '%' | '=')]*)
        {
            let trimmed = s.trim();
            tracing::debug!(%s, %trimmed, "Found unquoted positional attribute value");
            trimmed.to_string()
        }

        rule inner_attribute_value() -> String
        = s:$("\"" [^('"' | ']')]* "\"") { s.to_string() }

        /// URL rule matches both web URLs (proto://) and mailto: URLs
        pub rule url() -> String =
        proto:$("https" / "http" / "ftp" / "irc") "://" path:url_path() { format!("{proto}://{path}") }
        / "mailto:" email:email_address() { format!("mailto:{email}") }

        /// Email address pattern (RFC 822 simplified)
        ///
        /// Local part: alphanumeric plus . _ % + -
        /// Domain: alphanumeric plus . - (must contain TLD, must end with alphanumeric)
        ///
        /// - Domain must contain at least one dot (e.g., `foo@bar` is not valid,
        ///   `foo@bar.com` is)
        ///
        /// - Domain must end with alphanumeric (prevents capturing trailing punctuation
        ///   like `user@example.com.` - the dot stays outside the email for sentence
        ///   endings)
        rule email_address() -> String
        = local:$(
            // Quoted local part: "Jane Doe"@example.com
            // Quotes allow spaces and special chars in the local part (RFC 5321).
            "\"" [^'"']+ "\""
            // Unquoted local part (no spaces allowed)
            / ['a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '%' | '+' | '-']+
        )
        "@"
        // Format: alphanumeric+ (separator alphanumeric+)*
        // This ensures domain ends with alphanumeric (not . or -) and has proper structure.
        // e.g., `example.com.` -> matches `example.com`, trailing dot stays outside
        domain:$(
            ['a'..='z' | 'A'..='Z' | '0'..='9']+
            (['.' | '-'] ['a'..='z' | 'A'..='Z' | '0'..='9']+)*
        )
        {?
            // Require TLD - domain must contain at least one dot. This prevents `foo@bar`
            // from becoming a mailto link.
            if !domain.contains('.') {
                return Err("email domain must have TLD (contain a dot)");
            }

            Ok(format!("{local}@{domain}"))
        }

        /// URL path component - supports query params, fragments, encoding, etc.
        /// Excludes '[' and ']' to respect AsciiDoc macro/attribute boundaries
        rule url_path() -> String = path:$(['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '.' | '_' | '~' | ':' | '/' | '?' | '#' | '@' | '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' | '%' ]+)
        {?
            let mut inline_state = InlinePreprocessorParserState::new(
                path,
                state.line_map.clone(),
                &state.input,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|e| {
                tracing::error!(?e, "could not preprocess url path");
                "could not preprocess url path"
            })?;
            Ok(processed.text)
        }

        /// Filesystem path - conservative character set for cross-platform compatibility
        /// Includes '{' and '}' for `AsciiDoc` attribute substitution
        pub rule path() -> String = path:$(['A'..='Z' | 'a'..='z' | '0'..='9' | '{' | '}' | '_' | '-' | '.' | '/' | '\\' ]+)
        {?
            let mut inline_state = InlinePreprocessorParserState::new(
                path,
                state.line_map.clone(),
                &state.input,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|e| {
                tracing::error!(?e, "could not preprocess path");
                "could not preprocess path"
            })?;
            Ok(processed.text)
        }

        /// Parse optional attribute list for inline elements
        /// Returns (roles, id) extracted from attributes like [.role1.role2] or [#id.role]
        /// This is a simplified version of block attributes, used for inline formatting
        /// In inline context, % is treated as a literal character, not an option separator
        /// Stops parsing shorthands at invalid characters (comma, space, etc.)
        rule inline_attributes() -> (Vec<String>, Option<String>)
        = open_square_bracket() shorthands:inline_shorthand()+ [^']']* close_square_bracket()
        {
            let mut roles = Vec::new();
            let mut id = None;

            for s in shorthands {
                match s {
                    Shorthand::Role(r) => roles.push(r),
                    Shorthand::Id(i) => {
                        // If multiple IDs are specified, last one wins
                        id = Some(i);
                    }
                    Shorthand::Option(o) => {
                        // Options are not parsed by inline_shorthand, this branch should not occur
                        // Defensive: log and continue rather than panic
                        tracing::error!(option=?o, "inline_shorthand() unexpectedly produced Option variant");
                    }
                }
            }

            (roles, id)
        }

        pub rule source() -> Source
            = source:
        (
            u:url() {?
                Source::from_str(&u).map_err(|_| "failed to parse URL")
            }
            / p:path() {?
                Source::from_str(&p).map_err(|_| "failed to parse path")
            }
        )
        { source }

        rule digits() = ['0'..='9']+

        rule whitespace() = quiet!{ " " / "\t" }
        rule eol() = quiet!{ "\n" }

        rule comment_line() = quiet!{ comment() (eol() / ![_]) }
        rule comment() = quiet!{ "//" [^'\n']+ (&eol() / ![_]) }

        // Value parsing for document attributes
        // Handles both single-line values and values with continuation markers (" \" or " + \")
        // The preprocessor preserves these markers for the parser to handle
        rule document_attribute_value() -> String
        = " " lines:document_attribute_value_lines()
        {
            lines.join("\n")
        }

        // Parse value lines, continuing while lines end with backslash
        rule document_attribute_value_lines() -> Vec<&'input str>
        = backslash_continuation_lines() / single_line:$([^'\n']+) { vec![single_line] }

        // Lines ending with backslash continuation - keeps consuming lines until one doesn't end with backslash
        rule backslash_continuation_lines() -> Vec<&'input str>
        = lines:(line:$((!(" \\" eol()) [^'\n'])+ " \\") eol() { line })+
          last:$([^'\n']+)?
        {
            let mut result = lines;
            if let Some(l) = last {
                result.push(l);
            }
            result
        }

        // Document attribute parsing
        // Works identically in both header and block metadata contexts
        rule document_attribute_match() -> AttributeEntry<'input>
        = ":"
        key_entry:(
            "!" key:$([^':']+) { (false, key) }
            / key:$([^('!' | ':')]+) "!" { (false, key) }
            / key:$([^':']+) { (true, key) }
        )
        ":" &" "?
        value:document_attribute_value()?
        {
            let (set, key) = key_entry;
            AttributeEntry::new(key, set, value.as_deref())
        }
        / expected!("document attribute key starting with ':'")

        rule position() -> PositionWithOffset = offset:position!() {
            PositionWithOffset {
                offset,
                position: state.line_map.offset_to_position(offset, &state.input)
            }
        }

    }
}

/// Resolve auto-numbered callout markers (`<.>`) in verbatim text.
///
/// Scans for `<.>` markers at the end of lines and replaces them with
/// sequential numbers `<1>`, `<2>`, etc. Only processes markers at line endings
/// per `AsciiDoc` spec.
///
/// # Arguments
/// * `text` - The raw verbatim text that may contain `<.>` markers
///
/// # Returns
/// A tuple of (resolved text, list of callout numbers found)
fn resolve_verbatim_callouts(text: &str) -> (String, Vec<usize>) {
    use std::fmt::Write;

    let mut auto_number = 1;
    let mut callout_numbers = Vec::new();

    let resolved_text = text
        .lines()
        .map(|line| {
            // Check if line ends with <.> (possibly with trailing whitespace)
            let trimmed_end = line.trim_end();
            //if trimmed_end.ends_with("<.>") {
            // Find the position of <.> from the end
            if let Some(pos) = trimmed_end.rfind("<.>") {
                let mut result = line[..pos].to_string();
                let _ = write!(result, "<{auto_number}>");
                result.push_str(&line[pos + 3..]);
                callout_numbers.push(auto_number);
                auto_number += 1;
                return result;
                //}
            } else if let Some(number) = extract_callout_number(trimmed_end) {
                // Found an explicit callout number like <5>
                callout_numbers.push(number);
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");

    (resolved_text, callout_numbers)
}

/// Extract callout number from a line ending with <N>
fn extract_callout_number(line: &str) -> Option<usize> {
    if line.ends_with('>')
        && let Some(start) = line.rfind('<')
    {
        let number_str = &line[start + 1..line.len() - 1];
        number_str.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreachable
)]
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
                        column: 16,
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
            Some(&AttributeValue::String("v2.9".into()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("01-09-2024".into()))
        );
        assert_eq!(
            state.document_attributes.get("revremark"),
            Some(&AttributeValue::String("Fall incarnation".into()))
        );
        assert_eq!(
            state.document_attributes.get("description"),
            Some(&AttributeValue::String(
                "The document's description.".into()
            ))
        );
        assert_eq!(
            state.document_attributes.get("sectanchors"),
            Some(&AttributeValue::Bool(true))
        );
        assert_eq!(
            state.document_attributes.get("url-repo"),
            Some(&AttributeValue::String("https://my-git-repo.com".into()))
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
            Some(&AttributeValue::String("v2.9".into()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("01-09-2024".into()))
        );
        assert_eq!(
            state.document_attributes.get("revremark"),
            Some(&AttributeValue::String("Fall incarnation".into()))
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
            Some(&AttributeValue::String("v2.9".into()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("01-09-2024".into()))
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
            Some(&AttributeValue::String("v2.9".into()))
        );
        assert_eq!(state.document_attributes.get("revdate"), None);
        assert_eq!(
            state.document_attributes.get("revremark"),
            Some(&AttributeValue::String("Fall incarnation".into()))
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
            Some(&AttributeValue::String("v2.9".into()))
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
                        column: 16,
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
                Title::new(vec![InlineNode::PlainText(Plain {
                    content: "Document Title".to_string(),
                    location: Location {
                        absolute_start: 2,
                        absolute_end: 15,
                        start: crate::Position { line: 1, column: 3 },
                        end: crate::Position {
                            line: 1,
                            column: 16,
                        },
                    }
                })]),
                Some(Subtitle::new(vec![InlineNode::PlainText(Plain {
                    content: "And a subtitle".to_string(),
                    location: Location {
                        absolute_start: 17,
                        absolute_end: 31,
                        start: crate::Position {
                            line: 1,
                            column: 18,
                        },
                        end: crate::Position {
                            line: 1,
                            column: 32,
                        },
                    }
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
        let mut state = ParserState::new(input);
        let result =
            document_parser::header(input, &mut state)??.expect("header should be present");
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
                        column: 16,
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
                        column: 10,
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
                        column: 13,
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
                        column: 13,
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
    fn test_shorthand_id_role_combined() -> Result<(), Error> {
        // Test [#id.role] syntax - ID with role, no style
        let input = "[#bracket-id.some-role]";
        let mut state = ParserState::new(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "bracket-id".to_string(),
                xreflabel: None,
                location: Location {
                    absolute_start: 2,
                    absolute_end: 12,
                    start: crate::Position { line: 1, column: 3 },
                    end: crate::Position {
                        line: 1,
                        column: 13,
                    }
                }
            })
        );
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.contains(&"some-role".to_string()));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_id_role_option_combined() -> Result<(), Error> {
        // Test [#id.role%option] syntax - ID with role and option
        let input = "[#my-id.my-role%my-option]";
        let mut state = ParserState::new(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "my-id".to_string(),
                xreflabel: None,
                location: Location {
                    absolute_start: 2,
                    absolute_end: 7,
                    start: crate::Position { line: 1, column: 3 },
                    end: crate::Position { line: 1, column: 8 }
                }
            })
        );
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.contains(&"my-role".to_string()));
        assert!(metadata.options.contains(&"my-option".to_string()));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_multiple_roles() -> Result<(), Error> {
        // Test [#id.role1.role2] syntax - ID with multiple roles
        let input = "[#my-id.role-one.role-two]";
        let mut state = ParserState::new(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(metadata.id.as_ref().map(|a| a.id.as_str()), Some("my-id"));
        assert!(metadata.roles.contains(&"role-one".to_string()));
        assert!(metadata.roles.contains(&"role-two".to_string()));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_style_id_role() -> Result<(), Error> {
        // Test [style#id.role] syntax - already tested in test_document_attribute_with_id_mixed
        // but let's verify it still works
        let input = "[quote#my-id.my-role]";
        let mut state = ParserState::new(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(metadata.id.as_ref().map(|a| a.id.as_str()), Some("my-id"));
        assert_eq!(metadata.style, Some("quote".to_string()));
        assert!(metadata.roles.contains(&"my-role".to_string()));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_just_roles() -> Result<(), Error> {
        // Test [.role1.role2] syntax - just roles, no ID
        let input = "[.role-one.role-two]";
        let mut state = ParserState::new(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(metadata.id, None);
        assert!(metadata.roles.contains(&"role-one".to_string()));
        assert!(metadata.roles.contains(&"role-two".to_string()));
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

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_document_title() -> Result<(), Error> {
        let input = "Document Title
==============

Some content.
";
        let mut state = ParserState::new(input);
        state.options.setext = true;
        let result = document_parser::document(input, &mut state)??;
        let header = result.header.expect("document has a header");
        assert_eq!(header.title.len(), 1);
        assert!(
            matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if content == "Document Title")
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
        let mut state = ParserState::new(input);
        state.options.setext = true;
        let result = document_parser::document(input, &mut state)??;

        // Find the section
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
            matches!(&section.title[0], InlineNode::PlainText(Plain { content, .. }) if content == "Section One")
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
        let mut state = ParserState::new(input);
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
        // Test a single setext section with document title
        // Note: Multiple same-level setext sections currently nest incorrectly
        // (tracked as known limitation). This test verifies basic functionality.
        let input = "Document Title
==============

Section One
-----------

Content here.
";
        let mut state = ParserState::new(input);
        state.options.setext = true;
        let result = document_parser::document(input, &mut state)??;

        // Check document title (level 0)
        let header = result.header.expect("document has a header");
        assert!(
            matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if content == "Document Title")
        );

        // Find the section
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
            matches!(&section.title[0], InlineNode::PlainText(Plain { content, .. }) if content == "Section One")
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
        let mut state = ParserState::new(input);
        state.options.setext = true;
        let result = document_parser::document(input, &mut state)??;

        // Check document title
        let header = result.header.expect("document has a header");
        assert!(
            matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if content == "Document Title")
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

        // Verify all are level 1
        for (i, section) in sections.iter().enumerate() {
            assert_eq!(section.level, 1, "section {i} should be level 1");
        }

        // Verify titles
        assert!(
            matches!(&sections[0].title[0], InlineNode::PlainText(Plain { content, .. }) if content == "Section A")
        );
        assert!(
            matches!(&sections[1].title[0], InlineNode::PlainText(Plain { content, .. }) if content == "Section B")
        );
        assert!(
            matches!(&sections[2].title[0], InlineNode::PlainText(Plain { content, .. }) if content == "Section C")
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
        let mut state = ParserState::new(input);
        state.options.setext = true;
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
        let mut state = ParserState::new(input);
        state.options.setext = true;
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
        let mut state = ParserState::new(input);
        state.options.setext = true;
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
        let mut state = ParserState::new(input);
        state.options.setext = true;
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
}
