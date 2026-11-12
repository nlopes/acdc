// The `peg` macro adds 5 hidden parameters to every rule function, so even
// rules with just 3 explicit params exceed clippy's 7-argument threshold.
#![allow(clippy::too_many_arguments)]

use crate::{
    Anchor, Audio, Author, Block, BlockMetadata, CalloutListItem, Comment, CommentKind,
    CompatibilityMode, DescriptionListItem, DiscreteHeading, DocumentAttribute, DocumentAttributes,
    Error, Header, Image, InlineNode, ListItem, ListItemCheckedStatus, Location, PageBreak,
    Paragraph, Plain, Section, Source, Subtitle, TableOfContents, ThematicBreak, Title, Video,
    Warning, WarningKind,
    document_attribute::{AttributeDeclaration, RawAttributeValue},
    grammar::{
        ParserState,
        attributes::{
            BlockAttributeMode, block_attribute_list_allowed, resolve_block_anchor_reftext,
            style_overrides_block_form,
        },
        document::{
            author::{derive_author_attrs, register_author_attrs},
            construction::{
                ConstructionMode, DelimitedConstruction, QuoteConstruction, commit_content,
            },
            coordinator::{BlockParent, BlockStep, BlockStepKind, DocumentPrefix, NestedPrefix},
            delimited::{DelimitedKind, DelimitedParams, build_delimited_block},
            doctype::{is_book_doctype, is_manpage_doctype},
            header::{ParsedHeader, header_span, merge_header_metadata},
            lists::{
                CalloutItem, CalloutItemHead, CalloutScope, DescriptionItemHead,
                DescriptionItemSource, DescriptionListScope, DescriptionStep,
                DescriptionTermSource, ListEntry, ListItemHead, ListKind, ListMarker, ListPhase,
                ListStep, ListStepKind, OrdinaryListKind, OrdinaryListLayout, OrdinaryListScope,
                assemble_principal_text, build_callout_list, build_description_list,
                build_ordinary_list, calculate_item_end, find_dlist_marker,
            },
            manpage::{
                ManpageNameSection, derive_manpage_header_attrs, derive_name_section_attrs,
                prepare_manpage_name_attributes,
            },
            metadata::{
                AttributeBlockForm, AttributeOrAnchorLine, BlockMetadataLine, MetadataComment,
                ParsedAttributeList, RawAttributeList, TitleSource, assign_block_caption,
                finish_block_parsing_metadata, media_attributes_ahead, merge_macro_metadata,
                metadata_comment, metadata_marks_discrete_heading, order_block_prefix_events,
                parse_block_attribute_list, push_metadata_anchor, resolve_block_content_attributes,
                resolve_title, route_block_positions, route_media_attributes, select_heading_style,
                thematic_break_anchors,
            },
            paragraph::{ParagraphParams, build_paragraph, paragraph_text_end},
            quotes::{QuoteAttribution, QuoteParams, QuoteSource, build_quote},
            references::{ParsedSectionHeader, register_section_header},
            revision::{IgnoredRevisionFields, RevisionInfo, process_revision_info},
            sections::{apply_leveloffset, expected_child_level},
            source::{SourceLine, ordinary_kind},
            table::{
                InitialAttributes, PreparedTableCellParagraph, TableClosing, TableParseParams,
                construction::TableConstruction, parse_table_block_impl,
            },
        },
        helpers::{
            BlockParsingMetadata, PositionWithOffset, is_valid_bibliography_id, restore_url_path,
            title_looks_like_description_list,
        },
        inline_preprocessing,
        inline_preprocessor::InlinePreprocessorParserState,
        inline_processing::process_inlines,
        setext,
        state::BlockContext,
    },
    model::{ListLevel, SectionKind, SectionLevel, Substitution, substitution::HEADER},
};
use std::{borrow::Cow, rc::Rc};

peg::parser! {
    pub(crate) grammar document_parser(state: &mut ParserState<'input>) for str {
        use crate::model::apply_attribute_substitutions;

        // Each action gets the byte range of its preceding sequence.
        // Resolve line and column positions only when a node needs a location.
        inject span_start(_input, l, _r) -> usize { l }
        inject span_end(_input, _l, r) -> usize { r }

        #[no_eof]
        pub(in crate::grammar::document) rule document_prefix() -> Result<DocumentPrefix<'input>, Error>
        = eol()* start:position()
          comments:(comment:leading_comment() eol()* { comment })*
          header:header() prepare_manpage_front_matter()
          attributes:header_attribute_snapshot() body_offset:position!()
        {
            Ok(DocumentPrefix {
                start,
                header: header?,
                attributes,
                comments: comments.into_iter().collect::<Result<_, _>>()?,
                body_offset,
            })
        }

        // Jump to an accepted boundary without rescanning or copying its prefix.
        rule resume_at(offset: usize)
        = #{|input, _| {
            if input.is_char_boundary(offset) {
                peg::RuleResult::Matched(offset, ())
            } else {
                peg::RuleResult::Failed
            }
        }}

        // Inactive source handling can collect the complete body directly.
        // Reuse ordinary construction without moving each accepted block
        // through the ordered driver's larger construction-request enum.
        pub(in crate::grammar::document) rule remaining_blocks(offset: usize) -> Result<(Vec<Block<'input>>, usize), Error>
        = resume_at(offset) blocks:blocks(0, None, None) end:position!() eol()*
        { blocks.map(|blocks| (blocks, end)) }

        #[no_eof]
        pub(in crate::grammar::document) rule next_block(offset: usize, base_offset: usize, parent: BlockParent, mode: ConstructionMode) -> Result<BlockStep<'input>, Error>
        = resume_at(offset) step:(
            !trailing_block_metadata_match()
              block:(
                ordered_construction(mode) block:recognized_block(base_offset, parent.level, parent.kind, mode) { block }
                / recursive_construction(mode) block:block(base_offset, parent.level, parent.kind) { block.map(BlockStepKind::Block) }
              )
              end:position!()
            { block.map(|kind| BlockStep { kind, end }) }
            / eol()* &same_or_higher_level_section(base_offset, parent.level)
            { Ok(BlockStep::end(Vec::new(), offset)) }
            / trailing:trailing_block_metadata(base_offset)? end:position!() eol()* ![_]
            { trailing.transpose().map(|blocks| BlockStep::end(blocks.unwrap_or_default(), end)) }
        ) { step }

        // Read-only line probes share the actual block recognizers. The source
        // driver uses them to collect one unit, then constructs that unit once.
        pub(in crate::grammar::document) rule source_line() -> SourceLine<'input>
        = eol() { SourceLine::Blank }
        / document_attribute_match() (eol() / ![_]) { SourceLine::Attribute }
        / comment_line_prefix() source_line_tail() { SourceLine::Comment }
        / &atx_heading_match() source_line_tail() { SourceLine::Heading }
        / open:block_open() (eol() / ![_]) { SourceLine::Delimited { delimiter: open.1, table: false, comment: matches!(open.0, DelimitedKind::Comment) } }
        / delimiter:table_delimiter() (eol() / ![_]) { SourceLine::Delimited { delimiter, table: true, comment: false } }
        / (attribute_or_anchor_line_match() / title_line_match()) { SourceLine::Metadata }
        / extension_macro_source() (eol() / ![_]) { SourceLine::Macro }
        / (media_block_ahead_true() / toc_block_ahead()) source_line_tail() { SourceLine::Macro }
        / whitespace()* marker:unordered_list_marker() whitespace() text:$([^'\n']*) (eol() / ![_]) { SourceLine::List { kind: ordinary_kind(false), marker, empty: text.trim().is_empty() } }
        / whitespace()* marker:ordered_list_marker() whitespace() text:$([^'\n']*) (eol() / ![_]) { SourceLine::List { kind: ordinary_kind(true), marker, empty: text.trim().is_empty() } }
        / marker:callout_list_marker() whitespace() text:$([^'\n']*) (eol() / ![_]) { SourceLine::List { kind: ListKind::Callout, marker, empty: text.trim().is_empty() } }
        / term:description_list_term_source() &(whitespace() / eol() / ![_]) text:$([^'\n']*) (eol() / ![_]) { SourceLine::List { kind: ListKind::Description, marker: term.delimiter, empty: text.trim().is_empty() } }
        / "+" whitespace()* (eol() / ![_]) { SourceLine::Continuation }
        / ">" &(whitespace() / eol() / ![_]) source_line_tail() { SourceLine::Markdown }
        / "\"" source_line_tail() { SourceLine::Quoted }
        / source_line_tail() { SourceLine::Text }

        rule source_line_tail() = [^'\n']* (eol() / ![_])

        pub(in crate::grammar::document) rule source_block_close(expected: &str, table: bool)
        = ({? table.then_some(()).ok_or("ordinary delimiter") })
          delimiter:table_delimiter() (eol() / ![_])
          {? (delimiter == expected).then_some(()).ok_or("different table delimiter") }
        / block_close_delim(expected) (eol() / ![_])

        pub(in crate::grammar::document) rule source_paragraph_boundary(offset: usize, parent: Option<ListMarker<'input>>)
        = &(eol() / ![_] / paragraph_line_boundary(offset, parent)) source_line_tail()

        #[no_eof]
        pub(in crate::grammar::document) rule source_principal_line(offset: usize, kind: ListKind<'input>, marker: &str)
        = kind:unordered_kind(kind) unordered_list_principal_line(offset, marker, kind.parent_other) (eol() / ![_])
        / kind:ordered_kind(kind) ordered_list_principal_line(offset, marker, kind.parent_other) (eol() / ![_])
        / callout_kind(kind) callout_list_principal_line(offset) (eol() / ![_])
        / description_kind(kind) description_list_principal_line(offset) [^'\n']+ (eol() / ![_])

        pub(in crate::grammar::document) rule source_setext_underline(width: usize)
        = setext_section_level(width, None) (eol() / ![_])

        pub(in crate::grammar::document) rule source_document_underline(width: usize)
        = document_title_underline(width) (eol() / ![_])

        pub(in crate::grammar::document) rule source_document_title()
        = &atx_heading_prefix() document_title_token() whitespace() source_line_tail()

        pub(in crate::grammar::document) rule source_quoted_paragraph()
        = quoted_paragraph_params(0, 0) (eol() / ![_])

        rule ordered_construction(mode: ConstructionMode)
        = #{|_, position| if matches!(mode, ConstructionMode::Ordered) {
            peg::RuleResult::Matched(position, ())
        } else {
            peg::RuleResult::Failed
        }}

        rule recursive_construction(mode: ConstructionMode)
        = #{|_, position| if matches!(mode, ConstructionMode::Recursive) {
            peg::RuleResult::Matched(position, ())
        } else {
            peg::RuleResult::Failed
        }}

        pub(in crate::grammar::document) rule block_sequence_end(offset: usize)
        = resume_at(offset) eol()*

        rule header_attribute_snapshot() -> DocumentAttributes<'input>
        = {
            crate::document_attribute::normalize_toc_attributes(Rc::make_mut(&mut state.document_attributes));
            DocumentAttributes::clone(&state.document_attributes)
        }

        rule prepare_manpage_front_matter()
        = manpage_name_section_required() section:&manpage_name_section() {
            prepare_manpage_name_attributes(state, Some(section));
        }
        / {
            prepare_manpage_name_attributes(state, None);
        }

        rule manpage_name_section_required()
        = {?
            if is_manpage_doctype(&state.document_attributes)
                && !(state.document_attributes.text("manname").is_some()
                    && state.document_attributes.text("manpurpose").is_some())
            {
                Ok(())
            } else {
                Err("manpage name section is not required")
            }
        }

        // The first manpage section supplies header attributes. Inspect it without
        // consuming it so the regular block grammar still builds the AST.
        rule manpage_name_section() -> ManpageNameSection<'input>
        = eol()*
          metadata_attributes:(attribute:manpage_name_metadata() eol()* { attribute })*
          title:manpage_level_one_title()
          eol()*
          lines:manpage_name_body_line()+
        {?
            derive_name_section_attrs(lines)
                .map(|attributes| ManpageNameSection {
                    title,
                    attributes,
                    metadata_attributes: metadata_attributes.into_iter().flatten().collect(),
                })
                .ok_or("non-conforming manpage name section body")
        }

        rule manpage_level_one_title() -> &'input str
        = level:section_level(0, None) whitespace()+ title:$([^'\n']+) (eol() / ![_]) {?
            (level.1 == 1)
                .then_some(title.trim())
                .ok_or("not a level-one manpage name section")
        }
        / title:setext_title_text() eol()
          level:setext_section_level(title.trim().chars().count(), None) {?
            (level == 1 && !title_looks_like_description_list(title))
                .then_some(title.trim())
                .ok_or("not a level-one Setext manpage name section")
        }

        rule manpage_name_metadata() -> Option<AttributeDeclaration<'input>>
        = manpage_comment_block() { None }
        / "//" !"/" [^'\n']* (eol() / ![_]) { None }
        / "[[" (!"]]" [^'\n'])+ "]]" (eol() / ![_]) { None }
        / !empty_list_separator() block_attribute_start()
          open_square_bracket() attribute_list_content(BlockAttributeMode::Block) (eol() / ![_]) { None }
        / "." ![' ' | '\t' | '\n' | '\r' | '.'] [^'\n']* (eol() / ![_]) { None }
        / attribute:document_attribute_match() (eol() / ![_]) { Some(attribute) }

        rule manpage_comment_block()
        = delimiter:$(['/']*<4,>) eol()
          (!manpage_comment_delimiter(delimiter) [^'\n']* eol())*
          manpage_comment_delimiter(delimiter)

        rule manpage_comment_delimiter(delimiter: &str)
        = candidate:$(['/']*<4,>) (eol() / ![_]) {?
            (candidate == delimiter)
                .then_some(())
                .ok_or("not the matching comment delimiter")
        }

        rule manpage_name_body_line() -> Option<&'input str>
        = "//" !"/" [^'\n']* (eol() / ![_]) { None }
        / line:$([^'\n']+) (eol() / ![_]) { Some(line) }

        // After the document title, the first empty line ends the header in
        // both modes. Before the title, Asciidoctor mode ignores empty lines
        // between attribute entries, comments and block metadata, so a document
        // without a title keeps every leading attribute entry in its header.
        // Draft mode follows the language docs: the first empty line ends the
        // header, with or without a title. Empty lines inside an atomic comment
        // block do not end the surrounding header.
        pub(in crate::grammar::document) rule header() -> Result<ParsedHeader<'input>, Error>
            = start:position!()
            leading_end:header_leading_lines()
            metadata:header_metadata()
            titled:(
                title_authors:title_authors()
                (eol() (document_attribute() / header_comment()))*
                end:position!()
                { (title_authors, end) }
            )?
            (eol()*<,2> / ![_])
        {
            if let Some(((title, subtitle, authors), end)) = titled {
                let mut metadata = metadata?;
                crate::grammar::reference_text::resolve_metadata(state, &mut metadata)?;
                let location = header_span(state, start, end);
                let mut header = Header {
                    metadata,
                    title,
                    subtitle,
                    authors,
                    location,
                };

                derive_author_attrs(
                    state.arena,
                    &mut header,
                    Rc::make_mut(&mut state.document_attributes),
                    &state.options,
                );

                // Derive manpage attributes from header if doctype=manpage
                // This must happen during parsing so {mantitle} etc. work in body
                if is_manpage_doctype(&state.document_attributes) {
                    let warning = derive_manpage_header_attrs(
                        Some(&header),
                        Rc::make_mut(&mut state.document_attributes),
                        state.options.strict,
                        state.current_file.as_deref().map(std::path::PathBuf::as_path),
                    )?;
                    if let Some(warning) = warning {
                        state.add_warning(Warning::new(
                            warning,
                            Some(state.create_error_source_location(header.location.clone())),
                        ));
                    }
                }

                Ok(ParsedHeader { header: Some(header), untitled_location: None })
            } else {
                tracing::debug!("No document title found in the document header.");
                let untitled_location = leading_end.map(|end| header_span(state, start, end));
                Ok(ParsedHeader { header: None, untitled_location })
            }
        }

        // Attribute entries and comments at the start of the header. Returns the
        // end of the last line. Trailing empty lines are left for the caller.
        rule header_leading_lines() -> Option<usize>
        = ends:(header_leading_gap() end:header_leading_line() { end })* { ends.last().copied() }

        rule header_leading_gap()
        = (asciidoctor_mode() eol()*)?

        // Once an attribute entry matches, the rest of the line always matches,
        // so backtracking cannot apply the entry twice.
        rule header_leading_line() -> usize
        = (document_attribute() / header_comment()) end:position!() (eol() / ![_]) { end }

        rule asciidoctor_mode()
        = {?
            (state.options.compatibility_mode == crate::CompatibilityMode::Asciidoctor)
                .then_some(())
                .ok_or("draft mode")
        }

        /// Parse block metadata lines (anchors and attributes) that can appear before a document title.
        /// Only consumes metadata if followed by a document title to avoid stealing attributes
        /// meant for the first block when there's no document title.
        ///
        /// Attribute entries and comments can sit between those lines in either
        /// order. Asciidoctor mode also accepts empty lines between them; in Draft
        /// mode an empty line ends the header. A side-effect-free lookahead checks
        /// for the title first, so attribute entries are applied once, in source
        /// order, and only as header attributes.
        rule header_metadata() -> Result<BlockMetadata<'input>, Error>
            = &(header_metadata_line_match()* document_title_match())
              lines:(
                  header_empty_line() { None }
                  / document_attribute() (eol() / ![_]) { None }
                  / header_comment() (eol() / ![_]) { None }
                  / line:header_metadata_line() { Some(line) }
              )*
              &document_title_match()
            {
                merge_header_metadata(state, lines.into_iter().flatten())
            }
            / { Ok(BlockMetadata::default()) }

        rule header_metadata_line() -> AttributeOrAnchorLine<'input>
            = anchor:anchor(0) { AttributeOrAnchorLine::Anchor(anchor) }
            / attr:attributes_line() { AttributeOrAnchorLine::Attributes(Box::new(attr)) }

        rule header_empty_line()
            = asciidoctor_mode() eol()

        // Match a line above the document title without applying it.
        rule header_metadata_line_match()
            = header_empty_line()
            / document_attribute_match() (eol() / ![_])
            / comment() (eol() / ![_])
            / comment_block_match() (eol() / ![_])
            / attribute_or_anchor_line_match()

        rule title_authors() -> (Title<'input>, Option<Subtitle<'input>>, Vec<Author<'input>>)
        // Attribute entries and comments keep each metadata slot open. Consume
        // them outside optional metadata parsing so backtracking cannot assign
        // an entry twice; a blank line still ends the header.
        = title_and_subtitle:document_title()
          (eol() (document_attribute() / header_comment()))*
          authors:(eol() authors:authors_and_revision() { authors })?
          &(eol() / ![_])
        {
            let (title, subtitle) = title_and_subtitle;
            tracing::debug!("Found title and authors in the document header.");
            (title, subtitle, authors.unwrap_or_default())
        }

        pub(in crate::grammar::document) rule document_title() -> (Title<'input>, Option<Subtitle<'input>>)
        = document_title_atx()
        / document_title_setext()

        // Match a document title without processing its inlines. Lookaheads use
        // this because inline processing registers footnotes and other state.
        rule document_title_match()
        = &atx_heading_prefix() document_title_token() whitespace()
        / title:setext_title_text() eol() underline:$("="+) &(eol() / ![_]) {?
            setext::is_document_title(state.options.compatibility_mode, title, underline)
                .then_some(())
                .ok_or("not a setext document title")
        }

        /// ATX-style document title: `= Title` or `# Title`
        rule document_title_atx() -> (Title<'input>, Option<Subtitle<'input>>)
        = &atx_heading_prefix() document_title_token() whitespace() start:position!() title:$([^'\n']*) end:position!()
        {?
            tracing::debug!("Processing ATX document title");
            let block_metadata = BlockParsingMetadata::new(state.options.compatibility_mode);

            let (title_inlines, subtitle) = if let Some(colon_pos) = title.rfind(": ") {
                let subtitle_raw = &title[colon_pos + 1..];
                let subtitle_text = subtitle_raw.trim();
                if subtitle_text.is_empty() {
                    // Empty subtitle after colon, treat whole text as title
                    let (inlines, _) = process_inlines(state, &block_metadata, start, end, 0, title)
                        .map_err(|_| "could not process document title")?;
                    (inlines, None)
                } else {
                    let title_raw = &title[..colon_pos];
                    let title_text = title_raw.trim_end();
                    let title_end = start + title_text.len();
                    let (inlines, _) = process_inlines(state, &block_metadata, start, title_end, 0, title_text)
                        .map_err(|_| "could not process document title")?;

                    let sub_leading = subtitle_raw.len() - subtitle_raw.trim_start().len();
                    let sub_start_offset = start + colon_pos + 1 + sub_leading;
                    let subtitle_start = PositionWithOffset {
                        offset: sub_start_offset,
                        position: state.source_position(sub_start_offset),
                    };
                    let sub_end = sub_start_offset + subtitle_text.len();
                    let (subtitle_inlines, _) = process_inlines(state, &block_metadata, subtitle_start.offset, sub_end, 0, subtitle_text)
                        .map_err(|_| "could not process document subtitle")?;

                    (inlines, Some(Subtitle::new(subtitle_inlines)))
                }
            } else {
                let (inlines, _) = process_inlines(state, &block_metadata, start, end, 0, title)
                    .map_err(|_| "could not process document title")?;
                (inlines, None)
            };

            Ok((Title::new(title_inlines), subtitle))
        }

        rule setext_title_text() -> &'input str
        = !block_metadata_line_match() title:$([^'\n']+) {?
            setext::is_title(title).then_some(title).ok_or("not a Setext title")
        }

        /// Setext-style document title: Title underlined with `=` characters
        ///
        /// ```text
        /// Document Title
        /// ==============
        /// ```
        ///
        /// The underline must be within one character of the title width.
        /// Recognized in Asciidoctor mode.
        rule document_title_underline(title_width: usize)
        = underline:$("="+) &(eol() / ![_]) {?
            if !setext::is_enabled(state.options.compatibility_mode) {
                Err("setext mode not enabled")
            } else if !setext::width_ok(title_width, underline.chars().count()) {
                Err("underline width out of tolerance")
            } else {
                Ok(())
            }
        }

        rule document_title_setext() -> (Title<'input>, Option<Subtitle<'input>>)
        = title:setext_title_text() end:position!() eol()
          document_title_underline(title.trim().chars().count())
        {?
            let title_text = title.trim();
            tracing::debug!("Processing setext document title");
            let block_metadata = BlockParsingMetadata::new(state.options.compatibility_mode);

            let (title_inlines, subtitle) = if let Some(colon_pos) = title.rfind(": ") {
                let subtitle_raw = &title[colon_pos + 1..];
                let subtitle_text = subtitle_raw.trim();
                if subtitle_text.is_empty() {
                    let (inlines, _) = process_inlines(state, &block_metadata, span_start, end, 0, title)
                        .map_err(|_| "could not process setext document title")?;
                    (inlines, None)
                } else {
                    let title_raw = &title[..colon_pos];
                    let title_text = title_raw.trim_end();
                    let title_end = span_start + title_text.len();
                    let (inlines, _) = process_inlines(state, &block_metadata, span_start, title_end, 0, title_text)
                        .map_err(|_| "could not process setext document title")?;

                    let sub_leading = subtitle_raw.len() - subtitle_raw.trim_start().len();
                    let sub_start_offset = span_start + colon_pos + 1 + sub_leading;
                    let subtitle_start = PositionWithOffset {
                        offset: sub_start_offset,
                        position: state.source_position(sub_start_offset),
                    };
                    let sub_end = sub_start_offset + subtitle_text.len();
                    let (subtitle_inlines, _) = process_inlines(state, &block_metadata, subtitle_start.offset, sub_end, 0, subtitle_text)
                        .map_err(|_| "could not process setext document subtitle")?;

                    (inlines, Some(Subtitle::new(subtitle_inlines)))
                }
            } else {
                let (inlines, _) = process_inlines(state, &block_metadata, span_start, end, 0, title)
                    .map_err(|_| "could not process setext document title")?;
                (inlines, None)
            };

            Ok((Title::new(title_inlines), subtitle))
        }

        rule document_title_token() = "=" / "#"

        rule authors_and_revision() -> Vec<Author<'input>>
        = authors:author_line()
          (eol() (document_attribute() / header_comment()))*
          (eol() revision_pre_substitution())?
          { authors }

        rule author_line() -> Vec<Author<'input>>
            = !document_attribute_match() !comment()
              start:position!() author_line:$([^'\n']+) end:position!() {?
                let substituted_cow = apply_attribute_substitutions(author_line.trim(), HEADER, &state.document_attributes);
                // Intern any owned substitution result so the downstream
                // `authors()` parse can yield `Author<'input>` that outlives
                // this action block.
                let substituted: &'input str = match substituted_cow {
                    Cow::Borrowed(s) => s,
                    Cow::Owned(s) => state.intern_str(&s),
                };
                tracing::debug!("Processing author line with substitution");

                let mut temp_state =
                    ParserState::for_inline_parsing(substituted, state, state.inline_ctx);

                // `asciidoctor` always consumes the line after the title as the author
                // line; when it doesn't parse as structured "firstname [middle] [last]
                // [<email>]" authors (e.g. it contains parentheses, commas, or an
                // "Author:" prefix), the whole line becomes a single author's full name.
                let authors = if let Ok(authors) = document_parser::authors(substituted, &mut temp_state) {
                    tracing::debug!("Parsed authors from line");
                    authors
                } else {
                    tracing::debug!("Author line did not parse structurally; using whole line as a single author");
                    let location = state.create_error_source_location(state.create_location(start, end));
                    state.add_warning(Warning::new(
                        WarningKind::NonStandardAuthorLine { line: substituted.to_string() },
                        Some(location),
                    ));
                    vec![Author::new(state.arena, substituted, None, None)]
                };
                // Following entries may reference author metadata, but cannot
                // change the substitutions already applied to this author line.
                register_author_attrs(&authors, Rc::make_mut(&mut state.document_attributes));
                Ok(authors)
            }

        pub(in crate::grammar::document) rule authors() -> Vec<Author<'input>>
            = authors:(author() ++ (";" whitespace()*)) {
                authors
            }

        /// Parse an author in various formats:
        /// - "First Middle Last <email>"
        /// - "First Last <email>"
        /// - "First <email>"
        /// - "First Last"
        pub(in crate::grammar::document) rule author() -> Author<'input>
            = name:author_name() email:author_email()? {
                let mut author = name;
                if let Some(email_addr) = email {
                    author.email = Some(email_addr);
                }
                author
            }

        /// Parse author name in format: "First [Middle] Last" or just "First"
        rule author_name() -> Author<'input>
        = first:name_part() whitespace()+ middle:name_part() whitespace()+ last:$(name_part() ++ whitespace()) {
            Author::new(state.arena, first, Some(middle), Some(last))
        }
        / first:name_part() whitespace()+ last:name_part() {
            Author::new(state.arena, first, None, Some(last))
        }
        / first:name_part() {
            Author::new(state.arena, first, None, None)
        }

        /// Parse email address in format: " <email@domain>"
        rule author_email() -> &'input str
            = whitespace()* "<" email:$([^'>']*) ">" { email }

        rule name_part() -> &'input str
            = name:$([c if c.is_alphanumeric() || c == '.' || c == '-' || c == '\'']+ ("_" [c if c.is_alphanumeric() || c == '.' || c == '-' || c == '\'']+)*) {
                name
            }

        pub(in crate::grammar::document) rule revision() -> ()
            = "v"? number:$(digits() ++ ".") date:revision_date()? remark:revision_remark()? {
                let revision_info = RevisionInfo {
                    number: Cow::Owned(number.to_string()),
                    date: date.map(|d| Cow::Owned(d.to_string())),
                    remark: remark.map(|r| Cow::Owned(r.to_string())),
                };
                if revision_info.number.is_empty() {
                    return;
                }
                let revision_location = state.create_location(span_start, span_end);
                let ignored: IgnoredRevisionFields = {
                    let document_attributes = Rc::make_mut(&mut state.document_attributes);
                    process_revision_info(revision_info, document_attributes)
                };
                if ignored.number {
                    state.add_generic_warning_at(
                        "Revision number found in revision line but ignoring due to being set through attribute entries.".to_string(),
                        revision_location.clone(),
                    );
                }
                if ignored.date {
                    state.add_generic_warning_at(
                        "Revision date found in revision line but ignoring due to being set through attribute entries.".to_string(),
                        revision_location.clone(),
                    );
                }
                if ignored.remark {
                    state.add_generic_warning_at(
                        "Revision remark found in revision line but ignoring due to being set through attribute entries.".to_string(),
                        revision_location,
                    );
                }
            }

        /// Parse revision line with attribute reference support
        rule revision_pre_substitution() -> ()
            = rev_line:$([^'\n']+) {?
                let substituted_cow = apply_attribute_substitutions(rev_line.trim(), HEADER, &state.document_attributes);
                let substituted: &'input str = match substituted_cow {
                    Cow::Borrowed(s) => s,
                    Cow::Owned(s) => state.intern_str(&s),
                };
                tracing::debug!("Processing revision line with substitution");

                let mut temp_state =
                    ParserState::for_inline_parsing(substituted, state, state.inline_ctx);

                match document_parser::revision(substituted, &mut temp_state) {
                    Ok(()) => {
                        for key in ["revnumber", "revdate", "revremark"] {
                            if let Some(value) = temp_state.document_attributes.text(key) {
                                let value = state.intern_str(value);
                                Rc::make_mut(&mut state.document_attributes)
                                    .insert_text(key.into(), value.into());
                            }
                        }
                        tracing::debug!("Parsed revision from line");
                        Ok(())
                    }
                    Err(_) => Err("line did not parse as revision")
                }
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
        = start:position!() att:document_attribute_match() end:position!() (&eol() / ![_])
        {
            tracing::debug!("Found document attribute in the document header");
            let location = state.create_block_location(start, end, 0);
            state.apply_document_attribute(&att, true, location);
        }

        pub(in crate::grammar::document) rule blocks(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>) -> Result<Vec<Block<'input>>, Error>
        = blocks:(!trailing_block_metadata_match() block:block(offset, parent_section_level, direct_parent_section_kind) { block })*
          trailing:trailing_block_metadata(offset)?
        {
            let mut blocks = blocks.into_iter().collect::<Result<Vec<_>, Error>>()?;
            if let Some(trailing) = trailing {
                blocks.extend(trailing?);
            }
            Ok(order_block_prefix_events(blocks))
        }

        pub(in crate::grammar::document) rule compound_blocks(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Vec<Block<'input>>, Error>
        = content:blocks(offset, parent_section_level, None) eol()* { content }

        rule trailing_block_metadata_match()
        = eol()* &("[" / ".") block_metadata_sequence_match() ![_]

        // Unused block metadata has no inline effects; document attributes still take effect.
        rule trailing_block_metadata(offset: usize) -> Result<Vec<Block<'input>>, Error>
        = &trailing_block_metadata_match() eol()* events:(
            event:document_attribute_block(offset) (eol() / ![_]) eol()* { Some(event) }
            / comment:metadata_comment_line(offset) eol()* { Some(metadata_comment(state, comment)) }
            / (attribute_or_anchor_line_match() / title_line_match()) eol()* { None }
        )+
        {
            events.into_iter().flatten().collect()
        }

        pub(in crate::grammar::document) rule nested_document_blocks(offset: usize, initial_attributes: &mut InitialAttributes<'input>) -> Result<Vec<Block<'input>>, Error>
        = prefix:nested_document_prefix(offset, initial_attributes)
          body:blocks(offset, None, None)
        {
            let mut blocks = prefix?.blocks;
            blocks.extend(body?);
            Ok(order_block_prefix_events(blocks))
        }

        #[no_eof]
        pub(in crate::grammar::document) rule nested_document_prefix(offset: usize, initial_attributes: &mut InitialAttributes<'input>) -> Result<NestedPrefix<'input>, Error>
        = eol()*
          header:(
              comment:comment_line_block(offset) eol()* { comment }
              / attribute:document_attribute_block(offset) (eol() / ![_]) { attribute }
          )*
          header:normalize_nested_header(header, initial_attributes)
          body_offset:position!()
        {
            header.map(|blocks| NestedPrefix { blocks, body_offset })
        }

        rule normalize_nested_header(header: Vec<Result<Block<'input>, Error>>, initial_attributes: &mut Vec<(crate::AttributeName<'input>, crate::DocumentAttributeAssignment<'input>)>) -> Result<Vec<Block<'input>>, Error>
        = { crate::grammar::document::table::normalize_nested_header(state, header, initial_attributes) }

        /// Parse normal cell paragraphs; only blank lines separate them.
        pub(in crate::grammar::document) rule blocks_for_table_cell(offset: usize, prepared: &mut Option<Vec<PreparedTableCellParagraph<'input>>>) -> Result<Vec<Block<'input>>, Error>
        = eol()*
        blocks:(
            start:position!()
            content:$(([^'\n']+ / !(eol()*<2,> / eol()* ![_]) [_])+)
            end:position!()
            eol()*
            {
                // Block-looking lines are text in normal cells. Keep inline
                // substitutions without invoking document block recognition.
                if let Some(prepared) = prepared.as_mut() {
                    let mut paragraph = PreparedTableCellParagraph::new(state, start, end, offset, content)?;
                    let parsed = paragraph.parse(state);
                    if let Ok(block) = &parsed {
                        paragraph.retain_simple_content(block);
                    }
                    prepared.push(paragraph);
                    parsed
                } else {
                    let (content, _) = process_inlines(
                        state,
                        &BlockParsingMetadata::new(state.options.compatibility_mode),
                        start,
                        end,
                        offset,
                        content,
                    )?;
                    Ok(Block::Paragraph(Paragraph::new(
                        content,
                        state.create_block_location(start, end, offset),
                    )))
                }
            }
        )*
        {
            blocks.into_iter().collect()
        }

        // Ordinary children return blocks directly instead of copying them
        // through the ordered driver's deferred-construction representation.
        // Both dispatchers use the same syntax recognizers and node builders.
        rule block(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>) -> Result<Block<'input>, Error>
        = eol()* !same_or_higher_level_section(offset, parent_section_level)
          block:(
            comment_line_block(offset)
            / document_attribute_block(offset)
            / &"[" block:discrete_header(offset) { block }
            / section(offset, parent_section_level, direct_parent_section_kind, ConstructionMode::Recursive)
            / section_setext(offset, parent_section_level, direct_parent_section_kind, ConstructionMode::Recursive)
            / block_generic(offset, parent_section_level)
          ) { block }

        rule recognized_block(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>, mode: ConstructionMode) -> Result<BlockStepKind<'input>, Error>
        = eol()*
        // First check: if we're at a same-or-higher-level section, fail the entire block
        // This prevents section content from consuming sibling/parent sections as paragraphs
        !same_or_higher_level_section(offset, parent_section_level)
        block:(
            value:comment_line_block(offset) { value.map(BlockStepKind::Block) } /
            value:document_attribute_block(offset) { value.map(BlockStepKind::Block) } /
            // A discrete heading is introduced by an attribute line (`[discrete]`,
            // `[#id,discrete]`, `[float]`, …) or an anchor preceding one, so only
            // attempt it when the block starts with `[`. The rule itself backtracks
            // to `section`/`block_generic` when the metadata isn't a discrete marker.
            &"[" dh:discrete_header(offset) { dh.map(BlockStepKind::Block) } /
            section:section(offset, parent_section_level, direct_parent_section_kind, mode) { section.map(BlockStepKind::Block) } /
            // Legacy underlined sections are recognized in Asciidoctor mode.
            section_setext:section_setext(offset, parent_section_level, direct_parent_section_kind, mode) { section_setext.map(BlockStepKind::Block) } /
            (
                start:position!()
                block_metadata:(bm:block_metadata(offset, parent_section_level) {?
                    bm.map_err(|_| {
                        tracing::error!("error parsing block metadata in block_generic");
                        "block metadata parse error"
                    })
                })
                block:(
                    ordered_construction(mode) params:generic_delimited_params(start, offset) { Ok(BlockStepKind::Delimited(DelimitedConstruction { metadata: BlockParsingMetadata::default(), params })) }
                    / ordered_construction(mode) params:table_params(start, offset) { Ok(BlockStepKind::Table(TableConstruction { metadata: BlockParsingMetadata::default(), params })) }
                    / verbatim:styled_verbatim_paragraph(start, offset, &block_metadata) { verbatim.map(BlockStepKind::Block) }
            / recovered:excessive_list_item(start, offset, &block_metadata) { recovered.map(BlockStepKind::Block) }
                    / extension:extension_macro(start, offset, &block_metadata) { extension.map(BlockStepKind::Block) }
                    / image:image(start, offset, &block_metadata) { image.map(BlockStepKind::Block) }
                    / audio:audio(start, offset, &block_metadata) { audio.map(BlockStepKind::Block) }
                    / video:video(start, offset, &block_metadata) { video.map(BlockStepKind::Block) }
                    / toc:toc(start, offset, &block_metadata) { toc.map(BlockStepKind::Block) }
                    / thematic_break:thematic_break(start, offset, &block_metadata) { thematic_break.map(BlockStepKind::Block) }
                    / page_break:page_break(start, offset, &block_metadata) { page_break.map(BlockStepKind::Block) }
                    / ordered_construction(mode) body_offset:position!() kind:committed_list_kind(offset, &block_metadata, true) { Ok(BlockStepKind::List(ListEntry { start, body_offset, metadata: None, kind })) }
                    / ordered_construction(mode) params:quoted_paragraph_params(start, offset) { Ok(BlockStepKind::Quote(QuoteConstruction { metadata: BlockParsingMetadata::default(), params })) }
                    / ordered_construction(mode) params:markdown_blockquote_params(start, offset) { Ok(BlockStepKind::Quote(QuoteConstruction { metadata: BlockParsingMetadata::default(), params })) }
                    / paragraph:paragraph(start, offset, &block_metadata) { paragraph.map(BlockStepKind::Block) }
                ) {
                    block.map(|content| commit_content(state, block_metadata, content))
                }
            )
        )
        { block }

        /// Single-line comment that becomes a block in the AST.
        /// Line comments begin with `//` (but not `///` or `////` which are block comment delimiters).
        rule comment_line_block(offset: usize) -> Result<Block<'input>, Error>
        = comment_line_prefix() content:$([^'\n']*) end:position!() (eol() / ![_])
        {
            // `end` is captured before consuming the trailing newline so the
            // comment's location doesn't include it.
            Ok(Block::Comment(Comment {
                kind: CommentKind::Line,
                prefix_events: None,
                content,
                location: state.create_location(span_start + offset, end + offset),
            }))
        }

        rule comment_line_prefix()
        = "//" !"/"

        rule leading_comment() -> Result<Block<'input>, Error>
        = comment_line_block(0)
        / start:position!() block:comment_block(start, 0, &BlockParsingMetadata::new(state.options.compatibility_mode)) { block }

        // Consume a block comment as a whole so its contents cannot become
        // header attributes, author information or revision information.
        rule header_comment()
        = comment()
        / start:position!() result:comment_block(start, 0, &BlockParsingMetadata::new(state.options.compatibility_mode)) {?
            result.map(|_| ()).map_err(|_| "invalid header comment")
        }

        /// Like `comment_line_block` but leaves the trailing newline unconsumed
        /// (lookahead instead of consume). Used in list continuations so that a
        /// `+` continuation following the comment can still match, since
        /// continuation markers expect a leading newline before the `+`.
        rule comment_line_block_keep_eol(offset: usize) -> Result<Block<'input>, Error>
        = comment_line_prefix() content:$([^'\n']*) end:position!() &(eol() / ![_])
        {
            Ok(Block::Comment(Comment {
                kind: CommentKind::Line,
                prefix_events: None,
                content,
                location: state.create_location(span_start + offset, end + offset),
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
        // Standalone document attributes must take effect before comparing section levels.
        = check_section_blocks() !document_attribute_match()
          (metadata:$(block_metadata_sequence_match())
          {? if metadata_marks_discrete_heading(metadata, state) { Err("discrete heading belongs to its parent") } else { Ok(()) } })
          (
            // ATX-style section check - require whitespace after marker to avoid matching
            // description list items like `#term::` as sections
            level:section_level(offset, parent_section_level) &whitespace()
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
        /// Excludes description list items (e.g., `term:: content`) which would otherwise
        /// match as setext titles.
        rule setext_section_lookahead(parent_section_level: Option<SectionLevel>) -> ()
        = title:setext_title_text() eol() underline:$(['-' | '~' | '^' | '+']+) &(eol() / ![_])
        {?
            if title_looks_like_description_list(title) {
                return Err("title looks like a description list item");
            }
            if !setext::is_enabled(state.options.compatibility_mode) {
                return Err("setext mode not enabled");
            }

            let title_width = title.trim().chars().count();
            let underline_width = underline.chars().count();
            if !setext::width_ok(title_width, underline_width) {
                return Err("underline width out of tolerance");
            }

            let underline_char = underline.chars().next().ok_or("empty underline")?;
            let level = setext::char_to_level(underline_char).ok_or("invalid setext char")?;

            // Level 0 (=) is document title, not section — unless doctype is book (parts)
            if level == 0 && !is_book_doctype(&state.document_attributes) {
                return Err("not a section, seems like you're trying to define a document title");
            }

            if let Some(parent_level) = parent_section_level {
                if level < parent_level {
                    Ok(()) // This IS a same or higher level setext section
                } else {
                    Err("not a same or higher level section")
                }
            } else {
                Err("no parent section level to compare")
            }
        }

        // Marker-only lines can close inline formatting. Require a separator and
        // title before recognizing a heading or consuming heading metadata.
        rule atx_heading_prefix()
        = ("=" / "#")*<1,6> whitespace()+ !eol() &[_]

        // Recognize heading syntax before metadata can register title macros.
        rule atx_heading_match()
        = block_metadata_sequence_match() atx_heading_prefix()

        rule discrete_header(offset: usize) -> Result<Block<'input>, Error>
        = &atx_heading_match() block_metadata:(bm:heading_metadata(offset, None) {?
            let bm = bm.map_err(|_| {
                tracing::error!("error parsing block metadata in discrete_header");
                "block metadata parse error"
            })?;
            // Backtrack to the regular `section` rule unless the attribute line
            // actually marks this as a discrete heading; a discrete heading is
            // exempt from section-level sequencing, so it must not reach `section`.
            if !bm.discrete {
                return Err("not a discrete heading");
            }
            Ok(bm)
        })
        section_level:section_level(offset, None) whitespace()
        title_start:position!() title:section_title(offset, &block_metadata) title_end:position!() &(eol()*<1,2> / ![_])
        {
            let (title, _) = title?;
            tracing::debug!(title_start, title_end, "parsing discrete header block");

            let level = section_level.1;
            // `span_end` lands at title_end here because the trailing `&(...)` is a
            // zero-width lookahead.
            let location = state.create_block_location(span_start, span_end, offset);

            // `float` is a legacy alias for the `discrete` block style (older
            // AsciiDoc called these "floating titles"). Surface its use so authors
            // can migrate to `discrete`. Only the style form reaches this rule.
            if block_metadata.metadata.style == Some("float") {
                let warning_location = state.create_error_source_location(
                    state.create_block_location(span_start, span_end, offset),
                );
                state.add_warning(Warning::new(
                    WarningKind::LegacyFloatDiscreteHeading,
                    Some(warning_location),
                ));
            }

            Ok(Block::DiscreteHeading(DiscreteHeading {
                metadata: block_metadata.metadata,
                title,
                level,
                location,
            }))
        }

        rule document_attribute_block(offset: usize) -> Result<Block<'input>, Error>
        = att:document_attribute_match()
        {
            let name = Cow::Borrowed(att.name);
            let location = state.create_block_location(span_start, span_end, offset);
            let event = state
                .apply_document_attribute(&att, false, location.clone())
                .unwrap_or_else(|| DocumentAttribute::rejected(name, location));
            Ok(Block::DocumentAttribute(event))
        }

        rule section(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>, mode: ConstructionMode) -> Result<Block<'input>, Error>
        = check_section_blocks() &atx_heading_match()
        block_metadata:(bm:heading_metadata(offset, parent_section_level) {?
            bm.map_err(|_| {
                tracing::error!("error parsing block metadata in section");
                "block metadata parse error"
            })
        })
        section_level_start:position!()
        section_level:section_level(offset, parent_section_level)
        section_level_end:position!()
        whitespace()
        title_start:position!()
        section_header:(title:section_title(offset, &block_metadata) title_end:position!() &(eol()*<1,2> / ![_]) {
            let (title, natural_title) = title?;
            let location = state.create_block_location(section_level_start, title_end, offset);
            Ok::<ParsedSectionHeader<'input>, Error>(register_section_header(
                state,
                &block_metadata,
                title,
                natural_title,
                section_level.1,
                location,
                direct_parent_section_kind,
            ))
        })
        content:section_content(mode, offset, Some(expected_child_level(
            section_level.1,
            SectionKind::from_style(block_metadata.metadata.style),
            is_book_doctype(&state.document_attributes),
        )), Some(SectionKind::from_style(block_metadata.metadata.style)))?
        {
            let ParsedSectionHeader { title, numbering, reference_text, id_settings, heading_location } = section_header?;
            tracing::debug!(offset, "parsing section block");

            if let Some(parent_level) = parent_section_level {
                if section_level.1 < parent_level || section_level.1 > 5 {
                    return Err(Error::NestedSectionLevelMismatch(
                        Box::new(state.create_error_source_location(state.create_block_location(section_level_start, section_level_end, offset))),
                        section_level.1+1,
                        parent_level + 1,
                    ));
                }
                // A section that skips a level (deeper than one below its parent)
                // is "out of sequence". asciidoctor warns but still renders it at
                // its literal level rather than aborting, so we do the same.
                if section_level.1 > parent_level {
                    let location = state.create_error_source_location(
                        state.create_block_location(section_level_start, section_level_end, offset),
                    );
                    state.add_warning(Warning::new(
                        WarningKind::SectionLevelOutOfSequence {
                            expected: parent_level,
                            got: section_level.1,
                        },
                        Some(location),
                    ));
                }
            }

            let level = section_level.1;
            let location = state.create_block_location(span_start, span_end, offset);

            // Classify the section before the post-parse numbering pass applies
            // special-section rules to the complete section tree.
            let kind = SectionKind::from_style(block_metadata.metadata.style);

            let mut section = Section::parsed(
                block_metadata.metadata,
                title,
                level,
                content.unwrap_or(Ok(Vec::new()))?,
                kind,
                numbering,
                location,
            );
            section.reference_text = Some(reference_text);
            section.id_settings = id_settings;
            section.heading_location = heading_location;
            Ok(Block::Section(section))
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
        /// The underline must be within one character of the title width.
        /// Recognized in Asciidoctor mode.
        /// Parse a setext section level from the underline character.
        /// Returns the level (1-4) corresponding to -, ~, ^, +
        rule setext_section_level(title_width: usize, parent_section_level: Option<SectionLevel>) -> u8
        = underline:$(['-' | '~' | '^' | '+']+) &(eol() / ![_])
        {?
            if !setext::is_enabled(state.options.compatibility_mode) {
                return Err("setext mode not enabled");
            }

            let underline_width = underline.chars().count();

            if !setext::width_ok(title_width, underline_width) {
                return Err("underline width out of tolerance");
            }

            let underline_char = underline.chars().next().ok_or("empty underline")?;
            let level = setext::char_to_level(underline_char).ok_or("invalid setext underline character")?;

            // Document title (level 0) uses =, not allowed here — unless doctype is book (parts)
            if level == 0 && !is_book_doctype(&state.document_attributes) {
                return Err("use = underline for document title, not section");
            }

            if let Some(parent_level) = parent_section_level
                && (level < parent_level || level > parent_level + 1 || level > 5)
            {
                return Err("section level mismatch with parent");
            }

            Ok(level)
        }

        rule setext_section_match(parent_section_level: Option<SectionLevel>)
        = block_metadata_sequence_match()
          title:setext_title_text() eol()
          setext_section_level(title.trim().chars().count(), parent_section_level)

        /// Parse a setext-style section (title followed by underline).
        /// Excludes description list items (e.g., `term:: content`) which would otherwise
        /// match as setext titles.
        rule section_setext(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>, mode: ConstructionMode) -> Result<Block<'input>, Error>
        = check_section_blocks() !check_line_is_description_list(offset)
        &setext_section_match(parent_section_level)
        block_metadata:(bm:heading_metadata(offset, parent_section_level) {?
            bm.map_err(|_| {
                tracing::error!("error parsing block metadata in section_setext");
                "block metadata parse error"
            })
        })
        title_start:position!() title:setext_title_text() title_end:position!() eol()
        setext_level:setext_section_level(title.trim().chars().count(), parent_section_level)
        section_header:({
            let (processed_title, natural_title) =
                process_inlines(state, &block_metadata, title_start, title_end, offset, title)?;
            let title = Title::new(processed_title);
            let location = state.create_block_location(title_start, title_end, offset);
            Ok::<ParsedSectionHeader<'input>, Error>(register_section_header(
                state,
                &block_metadata,
                title,
                natural_title,
                setext_level,
                location,
                direct_parent_section_kind,
            ))
        })
        content:section_content(mode, offset, Some(expected_child_level(
            setext_level,
            SectionKind::from_style(block_metadata.metadata.style),
            is_book_doctype(&state.document_attributes),
        )), Some(SectionKind::from_style(block_metadata.metadata.style)))?
        {
            let ParsedSectionHeader { title, numbering, reference_text, id_settings, heading_location } = section_header?;
            let location = state.create_block_location(span_start, span_end, offset);

            // Classify the section by its style (see the ATX section rule).
            let kind = SectionKind::from_style(block_metadata.metadata.style);

            let mut section = Section::parsed(
                block_metadata.metadata,
                title,
                setext_level,
                content.unwrap_or(Ok(Vec::new()))?,
                kind,
                numbering,
                location,
            );
            section.reference_text = Some(reference_text);
            section.id_settings = id_settings;
            section.heading_location = heading_location;
            Ok(Block::Section(section))
        }

        rule block_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<BlockParsingMetadata<'input>, Error>
        = metadata:block_metadata_with_title(offset, parent_section_level, true) { metadata }

        // Headings display their own text, so ignore macros in preceding dot-titles.
        rule heading_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<BlockParsingMetadata<'input>, Error>
        = metadata:block_metadata_with_title(offset, parent_section_level, false) { metadata }

        rule block_metadata_with_title(offset: usize, parent_section_level: Option<SectionLevel>, parse_title: bool) -> Result<BlockParsingMetadata<'input>, Error>
        = meta_start:position!() lines:block_metadata_lines(offset)
          block_start:position!() media_ahead:media_block_ahead() form:attribute_block_form(offset)
        {
            let mut attributes = ParsedAttributeList::default();
            let meta_end = lines.last().map_or(meta_start, |(_, end)| *end);

            for (line, _) in lines {
                match line {
                    BlockMetadataLine::Anchor(value) => push_metadata_anchor(state, &mut attributes.metadata, value)?,
                    BlockMetadataLine::Attributes(raw) => {
                        let parsed = parse_block_attribute_list(
                            state, raw.source, raw.start, BlockAttributeMode::Block, offset,
                        )?;
                        attributes.merge(parsed, state.options.compatibility_mode);
                    },
                    BlockMetadataLine::DocumentAttribute(declaration, location) => {
                        if let Some(event) = state.apply_document_attribute(
                            &declaration,
                            false,
                            location,
                        ) {
                            attributes.metadata.push_prefix_event(Block::DocumentAttribute(event));
                        }
                    },
                    BlockMetadataLine::Comment(comment) => {
                        let comment = metadata_comment(state, comment)?;
                        attributes.metadata.push_prefix_event(comment);
                    },
                    BlockMetadataLine::Title { source, start, end } => {
                        attributes.title = Some(TitleSource::Dot { source, start, end });
                    }
                }
            }
            let media_ahead = media_ahead && !style_overrides_block_form(state.options.compatibility_mode, attributes.metadata.style);
            let media_attributes = if media_ahead {
                media_attributes_ahead(state, block_start, offset).map(|raw| -> Result<_, Error> {
                    let parsed = parse_block_attribute_list(
                        state, raw.source, raw.start, BlockAttributeMode::Macro, offset,
                    )?;
                    attributes.title = parsed.title.or_else(|| attributes.title.take());
                    Ok(Box::new(parsed.metadata))
                }).transpose()?
            } else { None };
            if parse_title {
                if !media_ahead {
                    route_block_positions(state, &mut attributes, form);
                }
            } else {
                attributes.metadata.style = select_heading_style(
                    attributes.metadata.first_source.as_ref().map(|source| source.source),
                    attributes.metadata.style,
                    state.options.compatibility_mode,
                );
            }
            let image_ahead = media_ahead && state.source_tail(block_start + offset).is_some_and(|tail| tail.starts_with("image::"));
            let image_alt = resolve_block_content_attributes(state, &mut attributes, media_attributes.as_deref(), image_ahead)?;
            // Titles use the completed metadata attributes, including entries
            // after the title line. Only the last authored title is displayed.
            let title = if let Some(source) = attributes.title.take().filter(|_| parse_title) {
                match resolve_title(state, source, offset) {
                    Ok(title) => title,
                    Err(error) => {
                        state.add_warning(crate::Warning::new(
                            crate::WarningKind::ContentRecovery {
                                message: format!("failed to parse block title, skipping: {error:?}").into(),
                            },
                            error.source_location().cloned(),
                        ));
                        Title::default()
                    }
                }
            } else {
                Title::default()
            };
            if meta_start != meta_end {
                attributes.metadata.location = Some(state.create_block_location(meta_start, meta_end, offset));
            }
            let mut result = finish_block_parsing_metadata(
                state,
                attributes,
                title,
                parent_section_level,
            )?;
            result.media_attributes = media_attributes;
            result.image_alt = image_alt;
            Ok(result)
        }

        // Comments carry metadata forward only after an authored metadata line.
        rule block_metadata_lines(offset: usize) -> Vec<(BlockMetadataLine<'input>, usize)>
        = &block_metadata_line_match() lines:(
            line:(block_metadata_line(offset) / comment:metadata_comment_line(offset) { BlockMetadataLine::Comment(comment) })
            end:position!() eol()* { (line, end) }
        )* { lines }
        / { Vec::new() }

        rule block_metadata_line(offset: usize) -> BlockMetadataLine<'input>
        = &['['] anchor:anchor(offset) { BlockMetadataLine::Anchor(anchor) }
        / &['['] attr:raw_attributes_line() { BlockMetadataLine::Attributes(attr) }
        / &[':'] doc_attr:document_attribute_line(offset) { BlockMetadataLine::DocumentAttribute(doc_attr.0, doc_attr.1) }
        / &['.'] title:title_line() { title }

        rule metadata_comment_line(offset: usize) -> MetadataComment<'input>
        = start:position!() "//" !"/" content:$([^'\n']*) end:position!() (eol() / ![_]) {
            MetadataComment::Line(Comment { kind: CommentKind::Line, prefix_events: None, content, location: state.create_location(start + offset, end + offset) })
        }
        / start:position!() params:comment_block_params(start, offset) { MetadataComment::Delimited(params) }

        rule block_metadata_sequence_match()
        = (block_metadata_line_match() eol()* (metadata_comment_match() eol()*)*)*

        rule metadata_comment_match()
        = comment_line()
        / comment_block_match()

        // A delimited comment block, through its closing delimiter when present.
        rule comment_block_match()
        = delimiter:comment_delimiter() (eol() / ![_])
          until_block_close(delimiter) (eol() &block_close_delim(delimiter))? block_close_delim(delimiter)?

        // Reuse block-start rules before selected attributes can register inline content.
        rule attribute_block_form(offset: usize) -> AttributeBlockForm<'input>
        = form:&(open:block_open() (eol() / ![_]) { AttributeBlockForm::Delimited(open.0, open.2) }) { form }
        / &((table_delimiter() / comma_table_delimiter() / colon_table_delimiter()) (eol() / ![_])) { AttributeBlockForm::Table }
        / &(media_block_ahead_true() / at_list_item_start() / at_callout_list_item()
            / check_line_is_description_list(offset) / toc_block_ahead() / ("<<<" &eol()*<2,2>) / "> "
            / thematic_break_marker()) { AttributeBlockForm::NonParagraph }
        / { AttributeBlockForm::Paragraph }

        rule media_block_ahead_true()
        = ("image::" / "audio::" / "video::") [^'[' | '\n' | '\r']+ "[" attribute_list_content(BlockAttributeMode::Macro) block_macro_end()

        rule toc_block_ahead()
        = "toc::" "[" attribute_list_content(BlockAttributeMode::Macro) block_macro_end()

        rule block_macro_end()
        = [' ' | '\t' | '\r']* &(eol() / ![_])

        // Match metadata in lookahead without expanding attributes or titles.
        rule block_metadata_line_match()
        = &['['] attribute_or_anchor_line_match()
        / &[':'] document_attribute_match() (eol() / ![_])
        / &['.'] title_line_match()

        rule title_line_match()
        = period() ![' ' | '\t' | '\n' | '\r' | '.'] [^'\n']+ (eol() / ![_])

        rule title_line() -> BlockMetadataLine<'input>
        = period() start:position!() title:$(![' ' | '\t' | '\n' | '\r' | '.'] [^'\n']*) end:position!() eol()
        {
            tracing::debug!(start, end, "Found title line in block metadata");
            BlockMetadataLine::Title { source: title, start, end }
        }

        // A document attribute line in block metadata context
        // This allows document attributes to be set between block attributes and the block content
        // Uses the same parsing logic as document attributes in the header
        rule document_attribute_line(offset: usize) -> (AttributeDeclaration<'input>, Location)
        = start:position!() attr:document_attribute_match() end:position!() eol()
        {
            tracing::debug!("Found document attribute in block metadata");
            (attr, state.create_block_location(start, end, offset))
        }

        rule check_section_blocks()
        = {? (state.block_context == BlockContext::Document).then_some(()).ok_or("section blocks are not allowed in compound content") }

        rule section_level(offset: usize, parent_section_level: Option<SectionLevel>) -> (&'input str, SectionLevel)
        = level:$(("=" / "#")*<1,6>)
        {
            let base_level: SectionLevel = level.len().try_into().unwrap_or(1) - 1;
            let byte_offset = span_start + offset;
            (level, apply_leveloffset(base_level, byte_offset, &state.leveloffset_ranges, &state.document_attributes))
        }

        rule at_line_start(offset: usize)
        = pos:position!()
        {?
            let absolute_pos = pos + offset;
            let at_line_start = absolute_pos == 0 || {
                let prev_byte_pos = absolute_pos.saturating_sub(1);
                state.source_byte(prev_byte_pos).is_some_and(|b| b == b'\n')
            };

            if !at_line_start {
                return Err("expected line start");
            }

            Ok(())
        }

        rule section_title(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<(Title<'input>, &'input str), Error>
        = title:$([^'\n']*)
        {
            tracing::debug!(title_start = span_start, title_end = span_end, "Found section title");
            let (content, natural_title) = process_inlines(
                state,
                block_metadata,
                span_start,
                span_end,
                offset,
                title,
            )?;
            Ok((Title::new(content), natural_title))
        }

        rule section_content(mode: ConstructionMode, offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>) -> Result<Vec<Block<'input>>, Error>
        = recursive_construction(mode) blocks:blocks(offset, parent_section_level, direct_parent_section_kind) { blocks }
        / { Ok(vec![]) }

        rule block_generic(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block<'input>, Error>
        = start:position!()
          block_metadata:(metadata:block_metadata(offset, parent_section_level) {?
            metadata.map_err(|_| {
                tracing::error!("error parsing block metadata in block_generic");
                "block metadata parse error"
            })
          })
          block:(
            delimited_block(start, offset, &block_metadata)
            / styled_verbatim_paragraph(start, offset, &block_metadata)
            / excessive_list_item(start, offset, &block_metadata)
            / extension_macro(start, offset, &block_metadata)
            / image(start, offset, &block_metadata)
            / audio(start, offset, &block_metadata)
            / video(start, offset, &block_metadata)
            / toc(start, offset, &block_metadata)
            / thematic_break(start, offset, &block_metadata)
            / page_break(start, offset, &block_metadata)
            / list(start, offset, &block_metadata)
            / quoted_paragraph(start, offset, &block_metadata)
            / markdown_blockquote(start, offset, &block_metadata)
            / paragraph(start, offset, &block_metadata)
          ) {
            let mut block = block?;
            assign_block_caption(state, &mut block);
            Ok(block)
          }

        // Block parsing for continuation context - lists inside continuations cannot consume
        // further continuations (those belong to the parent item that started the continuation)
        // Comments before attached content do not consume its continuation.
        // Keep comment-only and next-item cases on the existing path.
        rule block_in_continuation(offset: usize, parent_section_level: Option<SectionLevel>, parent_marker: Option<ListMarker<'input>>) -> Result<Block<'input>, Error>
        = value:recognized_block_in_continuation(offset, parent_section_level, parent_marker, ConstructionMode::Recursive)
        { value.and_then(|value| value.into_ready(state)) }

        rule recognized_block_in_continuation(offset: usize, parent_section_level: Option<SectionLevel>, parent_marker: Option<ListMarker<'input>>, mode: ConstructionMode) -> Result<BlockStepKind<'input>, Error>
        = continuation_comments() !eol() !at_list_item_start()
          !at_callout_list_item() !check_line_is_description_list(offset)
          !("+" whitespace()* (eol() / ![_]))
          block:recognized_continuation_block(offset, parent_section_level, parent_marker, mode) { block }
        / block:recognized_continuation_block(offset, parent_section_level, parent_marker, mode) { block }

        rule continuation_comments()
        = eol()* (
            comment_line()+
            / delimiter:comment_delimiter() eol()
              until_block_close(delimiter) eol()? block_close_delim(delimiter) eol()?
        )

        rule recognized_continuation_block(offset: usize, parent_section_level: Option<SectionLevel>, parent_marker: Option<ListMarker<'input>>, mode: ConstructionMode) -> Result<BlockStepKind<'input>, Error>
        = !trailing_block_metadata_match() start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|_| {
                tracing::error!("error parsing block metadata in block_in_continuation");
                "block metadata parse error"
            })
        })
        block:(
            // A `//` line comment or `////` block comment in a continuation
            // produces a comment node (which renders to nothing), matching
            // asciidoctor. Absorb optional leading blank lines: a trailing `+`
            // leaves a blank-line newline before the comment, while an immediate
            // comment sits directly at the delimiter. Without this, the `+` would
            // backtrack and leak a stray `+` paragraph. Must precede `paragraph`,
            // which would otherwise gobble the `//` line.
            comment:(eol()* comment_start:position!() c:(
                comment_line_block_keep_eol(offset)
                / comment_block(comment_start, offset, &block_metadata)
            ) { c }) { comment.map(BlockStepKind::Block) }
            / ordered_construction(mode) params:generic_delimited_params(start, offset) { Ok(BlockStepKind::Delimited(DelimitedConstruction { metadata: BlockParsingMetadata::default(), params })) }
            / ordered_construction(mode) params:table_params(start, offset) { Ok(BlockStepKind::Table(TableConstruction { metadata: BlockParsingMetadata::default(), params })) }
            / recursive_construction(mode) delimited_block:delimited_block(start, offset, &block_metadata) { delimited_block.map(BlockStepKind::Block) }
            / verbatim:styled_verbatim_paragraph(start, offset, &block_metadata) { verbatim.map(BlockStepKind::Block) }
            / recovered:excessive_list_item(start, offset, &block_metadata) { recovered.map(BlockStepKind::Block) }
            / extension:extension_macro(start, offset, &block_metadata) { extension.map(BlockStepKind::Block) }
            / image:image(start, offset, &block_metadata) { image.map(BlockStepKind::Block) }
            / audio:audio(start, offset, &block_metadata) { audio.map(BlockStepKind::Block) }
            / video:video(start, offset, &block_metadata) { video.map(BlockStepKind::Block) }
            / toc:toc(start, offset, &block_metadata) { toc.map(BlockStepKind::Block) }
            / thematic_break:thematic_break(start, offset, &block_metadata) { thematic_break.map(BlockStepKind::Block) }
            / page_break:page_break(start, offset, &block_metadata) { page_break.map(BlockStepKind::Block) }
            // Lists in continuation context cannot consume further continuations
            / ordered_construction(mode) body_offset:position!() kind:committed_list_kind(offset, &block_metadata, false) { Ok(BlockStepKind::List(ListEntry { start, body_offset, metadata: None, kind })) }
            / recursive_construction(mode) list:list_with_continuation(start, offset, &block_metadata, false) { list.map(BlockStepKind::Block) }
            / ordered_construction(mode) params:quoted_paragraph_params(start, offset) { Ok(BlockStepKind::Quote(QuoteConstruction { metadata: BlockParsingMetadata::default(), params })) }
            / recursive_construction(mode) quoted_paragraph:quoted_paragraph(start, offset, &block_metadata) { quoted_paragraph.map(BlockStepKind::Block) }
            / ordered_construction(mode) params:markdown_blockquote_params(start, offset) { Ok(BlockStepKind::Quote(QuoteConstruction { metadata: BlockParsingMetadata::default(), params })) }
            / recursive_construction(mode) markdown_blockquote:markdown_blockquote(start, offset, &block_metadata) { markdown_blockquote.map(BlockStepKind::Block) }
            / !(eol()+ attached_list_marker())
              paragraph:paragraph_with_list_boundary(start, offset, &block_metadata, parent_marker) { paragraph.map(BlockStepKind::Block) }
        ) {
            block.map(|content| commit_content(state, block_metadata, content))
        }

        rule delimited_block(
            start: usize,
            offset: usize,
            block_metadata: &BlockParsingMetadata<'input>,
        ) -> Result<Block<'input>, Error>
        = generic_delimited_block(start, offset, block_metadata)
        / table_block(start, offset, block_metadata)

        // Every non-table delimited block shares one open/content/optional-close
        // skeleton. `block_open` recognizes which kind a delimiter introduces and
        // `build_delimited_block` constructs the right block — the same split tables
        // use (`*_table_block` rules + `parse_table_block_impl`). The optional close
        // and the `(eol() / ![_])` after the open delimiter let an opener that runs
        // to end of input still produce a block, closed at EOF (asciidoctor's
        // recovery; `build_delimited_block` emits the unterminated warning).
        rule generic_delimited_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = params:generic_delimited_params(start, offset)
        {
            build_delimited_block(state, block_metadata, &params)
        }

        rule generic_delimited_params(start: usize, offset: usize) -> DelimitedParams<'input>
            = open_start:position!() open:block_open() (eol() / ![_])
              content_start:position!()
              source_text:$(until_block_close(open.1) (eol() &block_close_delim(open.1))?) body_end:position!()
              close:(close_start:position!() close_delim:block_close_delim(open.1) { (close_start, close_delim) })?
        {
            let content = if close.is_some() {
                source_text.strip_suffix('\n').unwrap_or(source_text)
            } else {
                source_text
            };
            let content_end = body_end - (source_text.len() - content.len());
            DelimitedParams {
                kind: open.0, open_delim: open.1, content, source_text,
                open_start, start, content_start, content_end, end: span_end, offset, close,
            }
        }

        // Recognize a non-table delimited-block opening delimiter, returning its
        // kind, the literal delimiter, and (for a Markdown ``` fence) an optional
        // language. Listing (`-`×4+) is tried before open (`--`) so `----` is a
        // listing, not a too-short open block.
        rule block_open() -> (DelimitedKind, &'input str, Option<&'input str>)
            = d:comment_delimiter()  { (DelimitedKind::Comment, d, None) }
            / d:example_delimiter()  { (DelimitedKind::Example, d, None) }
            / d:listing_delimiter()  { (DelimitedKind::Listing, d, None) }
            / d:literal_delimiter()  { (DelimitedKind::Literal, d, None) }
            / d:open_delimiter()     { (DelimitedKind::Open, d, None) }
            / d:sidebar_delimiter()  { (DelimitedKind::Sidebar, d, None) }
            / d:pass_delimiter()     { (DelimitedKind::Passthrough, d, None) }
            / d:quote_delimiter()    { (DelimitedKind::Quote, d, None) }
            / d:markdown_code_delimiter() lang:markdown_language()? { (DelimitedKind::Listing, d, lang) }

        // Content up to (but not including) a closing delimiter line exactly equal
        // to `expected`, or end of input. Generic over delimiter type; the exact
        // comparison in `block_close_delim` keeps a different-length or
        // different-character run from closing the block.
        rule until_block_close(expected: &str) -> &'input str
            = &block_close_delim(expected) { "" }
            / content:$((!(eol() block_close_delim(expected)) (eol() / [^'\n']+))*) { content }

        // Match a complete delimiter line equal to `expected`.
        // Separate alternatives prevent mixed delimiter characters from matching.
        rule block_close_delim(expected: &str) -> &'input str
            = delim:$("="+ / "/"+ / "-"+ / "."+ / "*"+ / "_"+ / "+"+ / "~"+ / "`"+)
              &(eol() / ![_])
              {? if delim == expected { Ok(delim) } else { Err("delimiter mismatch") } }

        // A `////` comment block specifically. The generic `delimited_block` covers
        // this in normal flow, but a list/description-list continuation needs to
        // match *only* a comment block (to absorb it after a `+`), so this gated
        // entry point reuses the shared skeleton and builder.
        rule comment_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = params:comment_block_params(start, offset) { build_delimited_block(state, block_metadata, &params) }

        rule comment_block_params(start: usize, offset: usize) -> DelimitedParams<'input>
            = open_start:position!() open_delim:comment_delimiter() (eol() / ![_])
              content_start:position!()
              source_text:$(until_block_close(open_delim) (eol() &block_close_delim(open_delim))?) body_end:position!()
              close:(close_start:position!() close_delim:block_close_delim(open_delim) { (close_start, close_delim) })?
        {
            let content = if close.is_some() {
                source_text.strip_suffix('\n').unwrap_or(source_text)
            } else {
                source_text
            };
            let content_end = body_end - (source_text.len() - content.len());
            DelimitedParams {
                kind: DelimitedKind::Comment, open_delim, content, source_text,
                open_start, start, content_start, content_end, end: span_end, offset, close,
            }
        }

        // Delimiter recognition rules
        rule comment_delimiter() -> &'input str = delim:$("/"*<4,>) { delim }
        rule example_delimiter() -> &'input str = delim:$("="*<4,>) { delim }
        rule listing_delimiter() -> &'input str = delim:$("-"*<4,>) { delim }
        rule literal_delimiter() -> &'input str = delim:$("."*<4,>) { delim }
        rule open_delimiter() -> &'input str = delim:$("-"*<2,2> / "~"*<4,>) { delim }
        rule sidebar_delimiter() -> &'input str = delim:$("*"*<4,>) { delim }
        rule table_delimiter() -> &'input str = delim:$((['|' | ',' | ':' | '!'] "="*<3,>)) { delim }

        // Opening rules select the table's default separator. Content scanning
        // compares the complete opening token, retaining nested tables with a
        // different delimiter in the outer table's body.
        rule pipe_table_delimiter() -> &'input str = delim:$("|" "="*<3,>) { delim }
        rule excl_table_delimiter() -> &'input str = delim:$("!" "="*<3,>) { delim }
        rule comma_table_delimiter() -> &'input str = delim:$("," "="*<3,>) { delim }
        rule colon_table_delimiter() -> &'input str = delim:$(":" "="*<3,>) { delim }

        rule pass_delimiter() -> &'input str = delim:$("+"*<4,>) { delim }
        rule markdown_code_delimiter() -> &'input str = delim:$("`"*<3,>) { delim }
        rule quote_delimiter() -> &'input str = delim:$("_"*<4,>) { delim }

        // A table closes only on an exact delimiter on its own line.
        // Different lengths and delimiter-looking cell text remain in its body.
        rule matching_table_delimiter(open_delim: &str) -> &'input str
        = delim:table_delimiter() &(eol() / ![_]) {?
            (delim == open_delim).then_some(delim).ok_or("matching table delimiter")
        }

        rule until_table_delimiter(open_delim: &str) -> &'input str
        = content:$((!(eol() matching_table_delimiter(open_delim)) [_])*) { content }

        rule markdown_language() -> &'input str
        = lang:$((['a'..='z'] / ['A'..='Z'] / ['0'..='9'] / "_" / "+" / "-")+) { lang }

        // Table block dispatcher - tries each delimiter-specific variant in order.
        // This enables nested tables: |=== outer can contain !=== inner because
        // each rule only looks for its own closing delimiter.
        //
        // Terminated variants are tried first; unterminated fallbacks only match
        // when an opening delimiter runs to end-of-input without a close.
        rule table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = params:table_params(start, offset) { parse_table_block_impl(&params, state, block_metadata) }

        rule table_params(start: usize, offset: usize) -> TableParseParams<'input>
            = pipe_table_params(start, offset)
            / excl_table_params(start, offset)
            / comma_table_params(start, offset)
            / colon_table_params(start, offset)
            / unterminated_pipe_table_params(start, offset)
            / unterminated_excl_table_params(start, offset)
            / unterminated_comma_table_params(start, offset)
            / unterminated_colon_table_params(start, offset)

        rule pipe_table_params(start: usize, offset: usize) -> TableParseParams<'input>
            = table_start:position!() open_delim:pipe_table_delimiter() eol()
              content_start:position!() content:until_table_delimiter(open_delim) content_end:position!()
              eol() close_start:position!() close_delim:matching_table_delimiter(open_delim)
        {
            TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: "|",
                    closing: TableClosing::Terminated { close_delim, close_start },
            }
        }

        rule excl_table_params(start: usize, offset: usize) -> TableParseParams<'input>
            = table_start:position!() open_delim:excl_table_delimiter() eol()
              content_start:position!() content:until_table_delimiter(open_delim) content_end:position!()
              eol() close_start:position!() close_delim:matching_table_delimiter(open_delim)
        {
            TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: "!",
                    closing: TableClosing::Terminated { close_delim, close_start },
            }
        }

        rule comma_table_params(start: usize, offset: usize) -> TableParseParams<'input>
            = table_start:position!() open_delim:comma_table_delimiter() eol()
              content_start:position!() content:until_table_delimiter(open_delim) content_end:position!()
              eol() close_start:position!() close_delim:matching_table_delimiter(open_delim)
        {
            TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: ",",
                    closing: TableClosing::Terminated { close_delim, close_start },
            }
        }

        rule colon_table_params(start: usize, offset: usize) -> TableParseParams<'input>
            = table_start:position!() open_delim:colon_table_delimiter() eol()
              content_start:position!() content:until_table_delimiter(open_delim) content_end:position!()
              eol() close_start:position!() close_delim:matching_table_delimiter(open_delim)
        {
            TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: ":",
                    closing: TableClosing::Terminated { close_delim, close_start },
            }
        }

        // Unterminated table fallbacks: match an opening table delimiter
        // that runs to end-of-input without a closing delimiter. These
        // alternatives are tried only after all terminated variants fail,
        // so a document with a valid close never takes this path. When
        // taken, `parse_table_block_impl` emits an `UnterminatedTable`
        // warning and still produces a table, matching asciidoctor's
        // recovery behavior.
        //
        // The `(eol() / ![_])` after the open delimiter accepts both
        // `|===\n...` and `|===<EOF>`: the preprocessor's `normalize`
        // strips a single trailing newline (mirroring `str::lines`), so a
        // file ending with just `|===\n` reaches the grammar as `|===`.
        rule unterminated_pipe_table_params(start: usize, offset: usize) -> TableParseParams<'input>
            = table_start:position!() open_delim:pipe_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_table_delimiter(open_delim) content_end:position!()
              ![_]
        {
            TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: "|",
                    closing: TableClosing::Unterminated,
            }
        }

        rule unterminated_excl_table_params(start: usize, offset: usize) -> TableParseParams<'input>
            = table_start:position!() open_delim:excl_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_table_delimiter(open_delim) content_end:position!()
              ![_]
        {
            TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: "!",
                    closing: TableClosing::Unterminated,
            }
        }

        rule unterminated_comma_table_params(start: usize, offset: usize) -> TableParseParams<'input>
            = table_start:position!() open_delim:comma_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_table_delimiter(open_delim) content_end:position!()
              ![_]
        {
            TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: ",",
                    closing: TableClosing::Unterminated,
            }
        }

        rule unterminated_colon_table_params(start: usize, offset: usize) -> TableParseParams<'input>
            = table_start:position!() open_delim:colon_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_table_delimiter(open_delim) content_end:position!()
              ![_]
        {
            TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: ":",
                    closing: TableClosing::Unterminated,
            }
        }

        rule toc(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = &toc_block_ahead() "toc::" attributes:macro_attributes() end:position!()
          block_macro_end()
        {
            let metadata_from_attributes = attributes.metadata;
            let mut metadata = block_metadata.metadata.clone();
            merge_macro_metadata(&mut metadata, &metadata_from_attributes, state.options.compatibility_mode);
            metadata.move_positional_attributes_to_attributes();
            tracing::debug!("Found Table of Contents block");
            Ok(Block::TableOfContents(TableOfContents {
                metadata,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        rule structural_block_boundaries(block_metadata: &BlockParsingMetadata<'input>)
        = {?
            if style_overrides_block_form(state.options.compatibility_mode, block_metadata.metadata.style) {
                Err("verbatim paragraph style")
            } else {
                Ok(())
            }
        }

        // Select the media title before parsing macros in a preceding dot-title.
        rule media_block_ahead() -> bool
        = &media_block_ahead_true() { true }
        / { false }

        rule source_match()
        = ("https" / "http" / "ftp" / "irc") "://" url_path_match()
        / "mailto:" email_address()
        / path_match()

        // Disabled extensions must not update PEG's expected-token diagnostics.
        rule extensions_enabled()
        = #{|_, position| if state.options.extensions.is_some() {
            peg::RuleResult::Matched(position, ())
        } else {
            peg::RuleResult::Failed
        }}

        rule registered_macro_name() -> &'input str
        = extensions_enabled() name:$(['a'..='z' | 'A'..='Z']['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']*) "::" {?
            let context = match state.block_context {
                BlockContext::Document => crate::extensions::MacroContext::Document,
                BlockContext::Compound => crate::extensions::MacroContext::Compound,
            };
            if state.options.extensions.as_ref().and_then(|registry| registry.find(name)).is_some_and(|(_, descriptor)| descriptor.contexts.contains(&context)) {
                Ok(name)
            } else { Err("registered block macro") }
        }

        rule extension_macro_source() -> (&'input str, &'input str, &'input str, usize)
        = name:registered_macro_name() target:$([^'[' | '\n' | '\r']*)
          "[" arguments:attribute_list_content(BlockAttributeMode::Macro) end:position!() trailing:$([^'\n']*)
        { (name, target, arguments, end) }

        rule extension_macro(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = source:extension_macro_source() {
            let (name, target, arguments, end) = source;
            if state.source_slice(end + offset..span_end + offset).is_some_and(|trailing| !trailing.trim().is_empty()) {
                state.add_warning(Warning::new(WarningKind::ContentRecovery { message: format!("trailing content after block macro {name}").into() }, Some(state.create_error_source_location(state.create_location(end + offset, span_end + offset)))));
            }
            crate::extensions::recognize(state, name, target, arguments, block_metadata.metadata.clone(), block_metadata.title.clone(), state.create_block_location(start, end, offset))
        }

        rule image(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = structural_block_boundaries(block_metadata)
          &media_block_ahead_true() "image::" source:source() attributes:cached_macro_attributes(block_metadata) end:position!()
          block_macro_end()
        {
            let metadata_from_attributes = attributes.metadata;
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            route_media_attributes(&mut metadata, &metadata_from_attributes, state.options.compatibility_mode, false);
            let alt = block_metadata.image_alt.clone();
            metadata.alt_source = None;
            metadata.attributes.remove("alt");
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Image(Image {
                title,
                alt,
                source,
                metadata,
                location: state.create_block_location(start, end, offset),

            }))
        }

        rule audio(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = &media_block_ahead_true() "audio::" source:source() attributes:cached_macro_attributes(block_metadata) end:position!()
          block_macro_end()
        {
            let metadata_from_attributes = attributes.metadata;
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            merge_macro_metadata(&mut metadata, &metadata_from_attributes, state.options.compatibility_mode);
            metadata.first_source = None;
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Audio(Audio {
                title,
                source,
                metadata,
                location: state.create_block_location(start, end, offset),
            }))
        }

        // Preserve commas until the converter interprets a provider's playlist syntax.
        rule video(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = &media_block_ahead_true() "video::" target:$([^'[' | '\n' | '\r']+) attributes:cached_macro_attributes(block_metadata) end:position!()
          block_macro_end()
        {
            let target = apply_attribute_substitutions(target, &[Substitution::Attributes], &state.document_attributes);
            let source = Source::from_str_borrowed(state.intern_str(&target))?;
            let metadata_from_attributes = attributes.metadata;
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            route_media_attributes(&mut metadata, &metadata_from_attributes, state.options.compatibility_mode, true);
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Video(Video {
                title,
                source,
                metadata,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule thematic_break_marker()
        = ("'''"
               // Below are the markdown-style thematic breaks
               / "---"
               / "- - -"
               / "***"
               / "* * *"
            )

        rule thematic_break(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = thematic_break_marker()
        {
            tracing::debug!("Found thematic break block");
            Ok(Block::ThematicBreak(ThematicBreak {
                anchors: thematic_break_anchors(&block_metadata.metadata),
                title: block_metadata.title.clone(),
                location: state.create_block_location(start, span_end, offset),
            }))
        }

        rule page_break(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = "<<<" "<"* &(whitespace()* (eol() / ![_]))
        {
            tracing::debug!("Found page break block");
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();

            Ok(Block::PageBreak(PageBreak {
                title: block_metadata.title.clone(),
                metadata,
                location: state.create_block_location(start, span_end, offset),
            }))
        }

        rule list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = list_with_continuation(start, offset, block_metadata, true)

        // Reject disabled alternatives before they can register inline macros;
        // PEG backtracking does not undo those registrations.
        rule list_continuation_allowed(allow: bool)
        = {? allow.then_some(()).ok_or("continuation belongs to parent") }

        // Parameterized list rule - allow_continuation controls whether list items can consume
        // explicit continuations. Set to false when parsing lists inside continuation blocks
        // to prevent nested lists from consuming parent-level continuations.
        rule list_with_continuation(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool) -> Result<Block<'input>, Error>
        = excessive_list_item(start, offset, block_metadata)
        / callout_list(start, offset, block_metadata)
        / unordered_list(start, offset, block_metadata, None, allow_continuation, false)
        / ordered_list(start, offset, block_metadata, None, allow_continuation, false)
        / description_list(start, offset, block_metadata)


        rule unordered_kind(kind: ListKind<'input>) -> OrdinaryListKind<'input>
        = {? if let ListKind::Unordered(kind) = kind { Ok(kind) } else { Err("not unordered") } }

        rule ordered_kind(kind: ListKind<'input>) -> OrdinaryListKind<'input>
        = {? if let ListKind::Ordered(kind) = kind { Ok(kind) } else { Err("not ordered") } }

        rule callout_kind(kind: ListKind<'input>)
        = {? matches!(kind, ListKind::Callout).then_some(()).ok_or("not callout") }

        rule description_kind(kind: ListKind<'input>)
        = {? matches!(kind, ListKind::Description).then_some(()).ok_or("not description") }

        rule nested_same(kind: OrdinaryListKind<'input>) -> &'input str
        = {? kind.parent_same.ok_or("no same-type parent") }

        rule native_list_entry(offset: usize, entry: &ListEntry<'input>, parent_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = kind:unordered_kind(entry.kind) parent:nested_same(kind)
          list:unordered_list_nested(entry.start, offset, entry.metadata.as_deref().unwrap_or(parent_metadata), parent, kind.parent_other, kind.has_own_metadata) { list }
        / kind:unordered_kind(entry.kind) parent_is_none(kind.parent_same)
          list:unordered_list(entry.start, offset, entry.metadata.as_deref().unwrap_or(parent_metadata), kind.parent_other, kind.allow_continuation, !kind.has_own_metadata) { list }
        / kind:ordered_kind(entry.kind) parent:nested_same(kind)
          list:ordered_list_nested(entry.start, offset, entry.metadata.as_deref().unwrap_or(parent_metadata), parent, kind.parent_other, kind.has_own_metadata) { list }
        / kind:ordered_kind(entry.kind) parent_is_none(kind.parent_same)
          list:ordered_list(entry.start, offset, entry.metadata.as_deref().unwrap_or(parent_metadata), kind.parent_other, kind.allow_continuation, !kind.has_own_metadata) { list }
        / callout_kind(entry.kind) list:callout_list(entry.start, offset, entry.metadata.as_deref().unwrap_or(parent_metadata)) { list }
        / description_kind(entry.kind) list:description_list(entry.start, offset, entry.metadata.as_deref().unwrap_or(parent_metadata)) { list }

        rule description_list_ahead(offset: usize)
        = !at_list_item_start() !at_callout_list_item() !at_description_parent_item(offset)
          check_line_is_description_list(offset)

        rule nested_kind_allowed(kind: ListKind<'input>, metadata: &BlockParsingMetadata<'input>)
        = callout_kind(kind) structural_block_boundaries(metadata)
        / !callout_kind(kind)

        rule unordered_child_kind(offset: usize, current_marker: &'input str, parent_ordered_marker: Option<&'input str>, own: bool) -> ListKind<'input>
        = !at_ancestor_ordered_marker(parent_ordered_marker)
          &(whitespace()* ordered_list_marker() whitespace())
        { ListKind::Ordered(OrdinaryListKind { parent_other: Some(current_marker), parent_same: None, allow_continuation: false, has_own_metadata: own }) }
        / &at_deeper_unordered_marker(current_marker)
        { ListKind::Unordered(OrdinaryListKind { parent_other: parent_ordered_marker, parent_same: Some(current_marker), allow_continuation: false, has_own_metadata: own }) }
        / !at_callout_parent_item(offset) &at_callout_list_item() { ListKind::Callout }
        / &description_list_ahead(offset) { ListKind::Description }

        rule unordered_child_entry_with_metadata(offset: usize, metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_ordered_marker: Option<&'input str>) -> ListEntry<'input>
        = start:position!() own:parsed_nested_list_metadata(offset, metadata.parent_section_level)
          nested_list_metadata_gap()* kind:unordered_child_kind(offset, current_marker, parent_ordered_marker, true) nested_kind_allowed(kind, &own)
        { ListEntry { start, body_offset: span_end, metadata: Some(Box::new(own)), kind } }

        rule unordered_child_entry(offset: usize, metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_ordered_marker: Option<&'input str>) -> ListEntry<'input>
        = start:position!() kind:unordered_child_kind(offset, current_marker, parent_ordered_marker, false)
          content_metadata:(
              callout_kind(kind) child:nested_list_content_metadata(metadata) { Some(Box::new(child)) }
              / description_kind(kind) child:nested_list_content_metadata(metadata) { Some(Box::new(child)) }
              / { None }
          )
        { ListEntry { start, body_offset: start, metadata: content_metadata, kind } }

        rule ordered_child_kind(offset: usize, current_marker: &'input str, parent_unordered_marker: Option<&'input str>, own: bool) -> ListKind<'input>
        = !at_ancestor_unordered_marker(parent_unordered_marker)
          &(whitespace()* unordered_list_marker() whitespace())
        { ListKind::Unordered(OrdinaryListKind { parent_other: Some(current_marker), parent_same: None, allow_continuation: false, has_own_metadata: own }) }
        / &at_deeper_ordered_marker(current_marker)
        { ListKind::Ordered(OrdinaryListKind { parent_other: parent_unordered_marker, parent_same: Some(current_marker), allow_continuation: false, has_own_metadata: own }) }
        / !at_callout_parent_item(offset) &at_callout_list_item() { ListKind::Callout }
        / &description_list_ahead(offset) { ListKind::Description }

        rule ordered_child_entry_with_metadata(offset: usize, metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_unordered_marker: Option<&'input str>) -> ListEntry<'input>
        = start:position!() own:parsed_nested_list_metadata(offset, metadata.parent_section_level)
          nested_list_metadata_gap()* kind:ordered_child_kind(offset, current_marker, parent_unordered_marker, true) nested_kind_allowed(kind, &own)
        { ListEntry { start, body_offset: span_end, metadata: Some(Box::new(own)), kind } }

        rule ordered_child_entry(offset: usize, metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_unordered_marker: Option<&'input str>) -> ListEntry<'input>
        = start:position!() kind:ordered_child_kind(offset, current_marker, parent_unordered_marker, false)
          content_metadata:(
              callout_kind(kind) child:nested_list_content_metadata(metadata) { Some(Box::new(child)) }
              / description_kind(kind) child:nested_list_content_metadata(metadata) { Some(Box::new(child)) }
              / { None }
          )
        { ListEntry { start, body_offset: start, metadata: content_metadata, kind } }


        rule committed_list_kind(offset: usize, metadata: &BlockParsingMetadata<'input>, allow: bool) -> ListKind<'input>
        = at_callout_list_item() structural_block_boundaries(metadata) callout_list_marker() { ListKind::Callout }
        / ordinary_list_kind(allow)
        / description_list_ahead(offset) description_list_term_source() { ListKind::Description }

        #[no_eof]
        pub(in crate::grammar::document) rule first_callout_list_item(offset: usize, base: usize, metadata: &BlockParsingMetadata<'input>) -> (CalloutItemHead<'input>, CalloutScope, usize)
        = resume_at(offset) at_callout_list_item() structural_block_boundaries(metadata)
          scope:enter_callout_list(base) head:callout_list_head(base, metadata) end:position!()
        { (head, scope, end) }

        #[no_eof]
        pub(in crate::grammar::document) rule next_callout_list_item(offset: usize, base: usize, metadata: &BlockParsingMetadata<'input>) -> (Option<CalloutItemHead<'input>>, usize)
        = resume_at(offset) head:(eol()+ head:callout_list_head(base, metadata) { head })? end:position!()
        { (head, end) }

        #[no_eof]
        pub(in crate::grammar::document) rule first_description_list_item(offset: usize, base: usize, metadata: &BlockParsingMetadata<'input>) -> (DescriptionItemHead<'input>, Option<DescriptionListScope<'input>>, usize)
        = resume_at(offset) description_list_ahead(base) parent:enter_description_list(base)
          head:description_list_head(base, metadata) end:position!() { (head, parent, end) }

        #[no_eof]
        pub(in crate::grammar::document) rule next_description_list_item(offset: usize, base: usize, metadata: &BlockParsingMetadata<'input>, parent: Option<DescriptionListScope<'input>>) -> (Option<DescriptionItemHead<'input>>, usize)
        = resume_at(offset) head:(description_list_additional_prefix(base, parent) head:description_list_head(base, metadata) { head })? end:position!()
        { (head, end) }

        rule continuing_description(actual: bool)
        = {? actual.then_some(()).ok_or("no active description continuation group") }

        #[no_eof]
        pub(in crate::grammar::document) rule description_list_next(offset: usize, base: usize, metadata: &BlockParsingMetadata<'input>, delimiter: &'input str, continuing: bool) -> DescriptionStep<'input>
        = resume_at(offset) value:(
            continuing_description(continuing) description_continuation_prefix(base)
            block:recognized_block_in_continuation(base, metadata.parent_section_level, Some(ListMarker::Description(delimiter)), ConstructionMode::Ordered)
            end:position!() { DescriptionStep { child: Some(block), end, continuing: true, warn_on_error: false } }
            / eol() description_continuation_prefix(base)
              block:recognized_block_in_continuation(base, metadata.parent_section_level, Some(ListMarker::Description(delimiter)), ConstructionMode::Ordered)
              end:position!() { DescriptionStep { child: Some(block), end, continuing: true, warn_on_error: false } }
            / eol() entry:description_auto_entry(base, metadata) end:position!()
              { DescriptionStep { child: Some(Ok(BlockStepKind::List(entry))), end, continuing: false, warn_on_error: true } }
            / { DescriptionStep { child: None, end: offset, continuing: false, warn_on_error: false } }
        ) { value }

        rule ordinary_list_kind(allow: bool) -> ListKind<'input>
        = whitespace()* unordered_list_marker() &whitespace()
        { ListKind::Unordered(OrdinaryListKind { parent_other: None, parent_same: None, allow_continuation: allow, has_own_metadata: true }) }
        / whitespace()* ordered_list_marker() &whitespace()
        { ListKind::Ordered(OrdinaryListKind { parent_other: None, parent_same: None, allow_continuation: allow, has_own_metadata: true }) }

        rule list_phase(actual: ListPhase, expected: ListPhase)
        = {? (actual == expected).then_some(()).ok_or("different list phase") }

        #[no_eof]
        pub(in crate::grammar::document) rule first_ordinary_list_item(offset: usize, base: usize, kind: ListKind<'input>, metadata: &BlockParsingMetadata<'input>) -> (ListItemHead<'input>, Option<OrdinaryListScope<'input>>, usize)
        = resume_at(offset) result:(
            kind:unordered_kind(kind) prefix:unordered_list_prefix(base) empty_start:position!()
            head:unordered_list_head(base, metadata, prefix.0, prefix.1, empty_start, kind.parent_other)
            end:position!() { (head, prefix.2, end) }
            / kind:ordered_kind(kind) prefix:ordered_list_prefix(base) empty_start:position!()
            head:ordered_list_head(base, metadata, prefix.0, prefix.1, empty_start, kind.parent_other)
            end:position!() { (head, prefix.2, end) }
        ) { result }

        #[no_eof]
        pub(in crate::grammar::document) rule list_next(offset: usize, base: usize, kind: ListKind<'input>, base_marker: &'input str, marker: &'input str, metadata: &BlockParsingMetadata<'input>, phase: ListPhase) -> ListStep<'input>
        = resume_at(offset) value:(
            list_phase(phase, ListPhase::AfterPrincipal)
            entry:(
                kind:unordered_kind(kind) entry:unordered_child_after_principal(base, metadata, marker, kind.parent_other) { entry }
                / kind:ordered_kind(kind) entry:ordered_child_after_principal(base, metadata, marker, kind.parent_other) { entry }
                / callout_kind(kind) entry:callout_child_after_principal(base, metadata) { entry }
            ) end:position!() { ListStep { kind: ListStepKind::List(entry), end } }
            / list_phase(phase, ListPhase::Attachments) !at_list_separator()
              value:(
                  parent:(kind:unordered_kind(kind) { (Some(ListMarker::Unordered(marker)), kind.allow_continuation) }
                      / kind:ordered_kind(kind) { (Some(ListMarker::Ordered(marker)), kind.allow_continuation) }
                      / callout_kind(kind) { (None, true) })
                  (list_continuation_immediate_prefix(base)
                    / list_continuation_allowed(parent.1) list_continuation_ancestor_prefix(base))
                  block:recognized_block_in_continuation(base, metadata.parent_section_level, parent.0, ConstructionMode::Ordered)
                  end:position!() { ListStep { kind: ListStepKind::block(block), end } }
                  / entry:(
                      kind:unordered_kind(kind) entry:unordered_child_after_attachment(base, metadata, marker, kind.parent_other) { entry }
                      / kind:ordered_kind(kind) entry:ordered_child_after_attachment(base, metadata, marker, kind.parent_other) { entry }
                  ) end:position!() { ListStep { kind: ListStepKind::List(entry), end } }
              ) { value }
            / list_phase(phase, ListPhase::Sibling) head:(
                kind:unordered_kind(kind)
                (parent:nested_same(kind) unordered_list_nested_rest_prefix(parent, base_marker)
                    / parent_is_none(kind.parent_same) unordered_list_rest_prefix(kind.parent_other, base_marker))
                item_start:position!() whitespace()* marker:unordered_list_marker()
                head:unordered_list_head(base, metadata, marker, item_start, item_start, kind.parent_other) { head }
                / kind:ordered_kind(kind)
                (parent:nested_same(kind) ordered_list_nested_rest_prefix(parent, base_marker)
                    / parent_is_none(kind.parent_same) ordered_list_rest_prefix(kind.parent_other, base_marker))
                item_start:position!() whitespace()* marker:ordered_list_marker()
                head:ordered_list_head(base, metadata, marker, item_start, item_start, kind.parent_other) { head }
            ) end:position!() { ListStep { kind: ListStepKind::Continue(Some(head)), end } }
            / { ListStep { kind: ListStepKind::Continue(None), end: offset } }
        ) { value }

        #[no_eof]
        pub(in crate::grammar::document) rule list_item_end(offset: usize, base: usize, allow: bool) -> usize
        = resume_at(offset)
          (list_continuation_allowed(allow) list_dangling_continuation())?
          description_parent_continuation(base)? end:position!() { end }

        rule raw_unordered_list_marker() -> &'input str = $("*"+ / "-")

        rule raw_ordered_list_marker() -> &'input str = $(digits()? "."+)

        rule unordered_list_marker() -> &'input str
        = marker:raw_unordered_list_marker() {?
            ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))
                .map(|_| marker).map_err(|_| "supported list depth")
        }

        rule ordered_list_marker() -> &'input str
        = marker:raw_ordered_list_marker() {?
            ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))
                .map(|_| marker).map_err(|_| "supported list depth")
        }

        rule excessive_list_marker()
        = whitespace()* marker:(raw_unordered_list_marker() / raw_ordered_list_marker()) &whitespace() {?
            (ListItem::parse_depth_from_marker(marker).unwrap_or(1) > usize::from(ListLevel::MAX))
                .then_some(()).ok_or("excessive list depth")
        }

        rule excessive_list_item(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = &excessive_list_marker() content:$([^'\n']+) {
            let location = state.create_block_location(span_start, span_end, offset);
            state.add_warning(Warning::new(
                WarningKind::ContentRecovery {
                    message: "list marker depth exceeds 255; preserving item as literal text".into(),
                },
                Some(state.create_error_source_location(location.clone())),
            ));
            Ok(Block::Paragraph(Paragraph {
                metadata: block_metadata.metadata.clone(),
                title: block_metadata.title.clone(),
                content: vec![InlineNode::PlainText(Plain { content, location, escaped: false })],
                location: state.create_block_location(start, span_end, offset),
                source_text: Some(content),
            }))
        }

        rule description_list_marker() -> &'input str = $("::::" / ":::" / "::" / ";;")

        rule callout_list_marker() -> &'input str = $("<" (digits() / ".") ">")

        // This restricted form excludes titles and document attributes, which
        // asciidoctor does not assign to an automatically nested list.
        rule nested_list_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<BlockParsingMetadata<'input>, Error>
        = meta_start:position!() lines:(line:(
            anchor:anchor(offset) { AttributeOrAnchorLine::Anchor(anchor) }
            / attr:attributes_line_at(offset) { AttributeOrAnchorLine::Attributes(Box::new(attr)) }
        ) end:position!() eol()* { (line, end) })+
        {
            let mut attributes = ParsedAttributeList::default();
            let meta_end = lines.last().map_or(meta_start, |(_, end)| *end);
            for (line, _) in lines {
                match line {
                    AttributeOrAnchorLine::Anchor(anchor) => push_metadata_anchor(state, &mut attributes.metadata, anchor)?,
                    AttributeOrAnchorLine::Attributes(parsed) => {
                        attributes.merge(*parsed, state.options.compatibility_mode);
                    }
                }
            }
            attributes.metadata.location = Some(state.create_block_location(meta_start, meta_end, offset));
            finish_block_parsing_metadata(
                state,
                attributes,
                Title::default(),
                parent_section_level,
            )
        }

        rule parsed_nested_list_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> BlockParsingMetadata<'input>
        = metadata:nested_list_metadata(offset, parent_section_level) {?
            metadata.map_err(|_| {
                tracing::error!("error parsing nested list metadata");
                "nested list metadata parse error"
            })
        }

        rule block_attribute_start()
        = !double_open_square_bracket()
        / &("[[" !"[") {?
            (state.options.compatibility_mode == crate::CompatibilityMode::Draft)
                .then_some(()).ok_or("draft anchor shorthand")
        }

        rule attributes_line_match()
        = !empty_list_separator() block_attribute_start()
          open_square_bracket() attribute_list_content(BlockAttributeMode::Block) (eol() / ![_])

        rule anchor_line_match()
        = double_open_square_bracket() [^'\'' | ',' | ']' | '[' | ' ' | '\t' | '\n' | '\r']+
          (comma() block_anchor_reftext_match() / double_close_square_bracket()) (eol() / ![_])

        rule block_anchor_reftext_match()
        = text:$([^'\n' | '\r']+) {?
            text.strip_suffix("]]")
                .and_then(|source| resolve_block_anchor_reftext(source, &state.document_attributes, state.options.compatibility_mode))
                .map(|_| ())
                .ok_or("block anchor reference text")
        }

        rule attribute_or_anchor_line_match()
        = anchor_line_match() / attributes_line_match()

        rule nested_list_metadata_gap()
        = eol() / comment_line()

        rule nested_unordered_child_after_metadata(offset: usize, current_marker: &str, parent_ordered_marker: Option<&'input str>)
        = (attribute_or_anchor_line_match() eol()*)+ nested_list_metadata_gap()* (
            !at_ancestor_ordered_marker(parent_ordered_marker)
            &(whitespace()* ordered_list_marker() whitespace())
            / &at_deeper_unordered_marker(current_marker)
            / !at_callout_parent_item(offset) at_callout_list_item()
            / !at_list_item_start() !at_callout_list_item() check_line_is_description_list(offset)
        )

        rule nested_ordered_child_after_metadata(offset: usize, current_marker: &str, parent_unordered_marker: Option<&'input str>)
        = (attribute_or_anchor_line_match() eol()*)+ nested_list_metadata_gap()* (
            !at_ancestor_unordered_marker(parent_unordered_marker)
            &(whitespace()* unordered_list_marker() whitespace())
            / &at_deeper_ordered_marker(current_marker)
            / !at_callout_parent_item(offset) at_callout_list_item()
            / !at_list_item_start() !at_callout_list_item() check_line_is_description_list(offset)
        )

        // Helper rule to check if we're at the start of a new list item (lookahead)
        rule at_list_item_start() = excessive_list_marker() / whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace()

        rule enter_ordinary_list(offset: usize, unordered: Option<&'input str>, ordered: Option<&'input str>) -> Option<OrdinaryListScope<'input>>
        = {
            let parent = state.ordinary_list_scope;
            let mut scope = parent.filter(|scope| scope.offset == state.list_scope_offset(offset))
                .unwrap_or(OrdinaryListScope { offset: state.list_scope_offset(offset), ..OrdinaryListScope::default() });
            scope.unordered = unordered.or(scope.unordered);
            scope.ordered = ordered.or(scope.ordered);
            state.ordinary_list_scope = Some(scope);
            parent
        }

        rule ordinary_list_scope(offset: usize) -> OrdinaryListScope<'input>
        = { state.ordinary_list_scope.filter(|scope| scope.offset == state.list_scope_offset(offset)).unwrap_or_default() }

        rule at_ordinary_parent_item(offset: usize)
        = candidate:(
            whitespace()* marker:unordered_list_marker() whitespace() { ListMarker::Unordered(marker) }
            / whitespace()* marker:ordered_list_marker() whitespace() { ListMarker::Ordered(marker) }
          ) scope:ordinary_list_scope(offset) {?
            let matches = scope.unordered.is_some_and(|parent| ListMarker::Unordered(parent).is_sibling_or_ancestor(candidate))
                || scope.ordered.is_some_and(|parent| ListMarker::Ordered(parent).is_sibling_or_ancestor(candidate));
            matches.then_some(()).ok_or("not an ordinary parent item")
        }

        // Helper rule to check if we're at an ordered list marker ahead (after newlines)
        rule at_ordered_marker_ahead() = eol()+ whitespace()* ordered_list_marker()

        // Helper rule to check if we're at an unordered list marker ahead (after newlines)
        rule at_unordered_marker_ahead() = eol()+ whitespace()* unordered_list_marker()

        // Helper rule to check if we're at an ancestor-level ordered marker
        // Used in cross-type nesting to prevent consuming sibling ordered markers
        // that belong to a parent ordered list context
        rule at_ancestor_ordered_marker(ancestor: Option<&'input str>)
        = whitespace()* marker:ordered_list_marker() whitespace() {?
            match ancestor {
                Some(m) if marker.len() <= m.len() => Ok(()),
                _ => Err("not ancestor")
            }
        }

        // Helper rule to check if we're at an ancestor-level unordered marker
        // Used in cross-type nesting to prevent consuming sibling unordered markers
        // that belong to a parent unordered list context
        rule at_ancestor_unordered_marker(ancestor: Option<&'input str>)
        = whitespace()* marker:unordered_list_marker() whitespace() {?
            match ancestor {
                Some(m) if marker.len() <= m.len() => Ok(()),
                _ => Err("not ancestor")
            }
        }

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

        // Helper rule to check if we're at a deeper unordered marker (for nested same-type lists)
        // Used by unordered_list_item_nested_content to detect nested unordered lists
        rule at_deeper_unordered_marker(base_marker: &str)
        = whitespace()* marker:unordered_list_marker() whitespace() {?
            if marker.len() > base_marker.len() { Ok(()) } else { Err("same-or-shallower") }
        }

        // Helper rule to check if we're at a deeper ordered marker (for nested same-type lists)
        // Used by ordered_list_item_nested_content to detect nested ordered lists
        rule at_deeper_ordered_marker(base_marker: &str)
        = whitespace()* marker:ordered_list_marker() whitespace() {?
            if marker.len() > base_marker.len() { Ok(()) } else { Err("same-or-shallower") }
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

        rule unordered_list_principal_continuation(offset: usize, current_marker: &str, parent_ordered_marker: Option<&'input str>) -> &'input str
        = eol() line:unordered_list_principal_line(offset, current_marker, parent_ordered_marker) { line }

        rule unordered_list_principal_line(offset: usize, current_marker: &str, parent_ordered_marker: Option<&'input str>) -> &'input str
        =
          !(
              &eol()
              / &at_list_item_start()
              / &("+" whitespace()* (eol() / ![_]))
              / &at_list_separator_content()
              / &nested_unordered_child_after_metadata(offset, current_marker, parent_ordered_marker)
              / at_callout_list_item()
              / check_line_is_description_list(offset)
          )
          line:$((!eol() [_])*) { line }

        rule ordered_list_principal_continuation(offset: usize, current_marker: &str, parent_unordered_marker: Option<&'input str>) -> &'input str
        = eol() line:ordered_list_principal_line(offset, current_marker, parent_unordered_marker) { line }

        rule ordered_list_principal_line(offset: usize, current_marker: &str, parent_unordered_marker: Option<&'input str>) -> &'input str
        =
          !(
              &eol()
              / &at_list_item_start()
              / &("+" whitespace()* (eol() / ![_]))
              / &at_list_separator_content()
              / &nested_ordered_child_after_metadata(offset, current_marker, parent_unordered_marker)
              / at_callout_list_item()
              / check_line_is_description_list(offset)
          )
          line:$((!eol() [_])*) { line }

        // Block metadata in column one after a blank line starts a new description list.
        // Indented metadata-like text remains part of the current item.
        rule at_dlist_block_boundary()
        = eol()*<2,> &(
            ("[" ![']' | '['] [^']' | '\n']+ "]" whitespace()* eol())
            / ("[[" [^']']+ "]]" whitespace()* eol())
            / ("." ![' ' | '\t' | '\n' | '\r' | '.'] [^'\n']* eol())
        )

        rule unordered_list_prefix(offset: usize) -> (&'input str, usize, Option<OrdinaryListScope<'input>>)
        = whitespace()* marker_start:position!() marker:$(unordered_list_marker()) &whitespace()
          parent:enter_ordinary_list(offset, Some(marker), None)
        { (marker, marker_start, parent) }

        rule ordered_list_prefix(offset: usize) -> (&'input str, usize, Option<OrdinaryListScope<'input>>)
        = whitespace()* marker_start:position!() marker:$(ordered_list_marker()) &whitespace()
          parent:enter_ordinary_list(offset, None, Some(marker))
        { (marker, marker_start, parent) }

        rule unordered_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>, allow_continuation: bool, is_nested: bool) -> Result<Block<'input>, Error>
        // Parse the marker and enter its scope before constructing principals.
        // marker_start captures position before marker for correct first item location
        = prefix:unordered_list_prefix(offset)
        first:unordered_list_item_after_marker(offset, block_metadata, allow_continuation, prefix.0, prefix.1, parent_ordered_marker)
        rest:(unordered_list_rest_item(offset, block_metadata, parent_ordered_marker, allow_continuation, prefix.0))*
        {
            tracing::debug!("Found unordered list block");
            state.ordinary_list_scope = prefix.2;
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();

            Ok(build_ordinary_list(state, block_metadata, OrdinaryListLayout { start, end, offset, ordered: false, has_own_metadata: !is_nested }, items))
        }

        // Parse first item content after marker has been consumed by unordered_list
        // marker_start is the position where the marker began, for correct location tracking
        rule unordered_list_item_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool, marker: &'input str, marker_start: usize, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = list_continuation_allowed(allow_continuation)
          item:unordered_list_item_with_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_ordered_marker) { item }
        / item:unordered_list_item_no_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_ordered_marker) { item }

        // Zero-cost guards for the front-of-alternative branch selector in
        // `*_list_rest_item`. Keeps the expensive item parse out of the branch
        // whose trailing semantic action would have just discarded it.
        rule parent_is_some(parent: Option<&'input str>) -> ()
        = {? if parent.is_some() { Ok(()) } else { Err("parent_is_none") } }

        rule parent_is_none(parent: Option<&'input str>) -> ()
        = {? if parent.is_none() { Ok(()) } else { Err("parent_is_some") } }

        rule unordered_list_rest_prefix(parent_ordered_marker: Option<&'input str>, base_marker: &str)
        = parent_is_some(parent_ordered_marker) !at_list_separator() !eol() comment_line()* !at_ordered_marker_ahead()
        / parent_is_some(parent_ordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_unordered_marker(base_marker) !at_ordered_marker_ahead()
        // Branch: no ordered parent
        / parent_is_none(parent_ordered_marker) !at_list_separator() !eol() comment_line()*
        / parent_is_none(parent_ordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_unordered_marker(base_marker)

        rule unordered_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>, allow_continuation: bool, base_marker: &str) -> Result<(ListItem<'input>, usize), Error>
        = unordered_list_rest_prefix(parent_ordered_marker, base_marker)
          item:unordered_list_item(offset, block_metadata, allow_continuation, parent_ordered_marker) { item }


        rule ordered_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>, allow_continuation: bool, is_nested: bool) -> Result<Block<'input>, Error>
        // Parse the marker and enter its scope before constructing principals.
        // marker_start captures position before marker for correct first item location
        = prefix:ordered_list_prefix(offset)
        first:ordered_list_item_after_marker(offset, block_metadata, allow_continuation, prefix.0, prefix.1, parent_unordered_marker)
        rest:(ordered_list_rest_item(offset, block_metadata, parent_unordered_marker, allow_continuation, prefix.0))*
        {
            tracing::debug!("Found ordered list block");
            state.ordinary_list_scope = prefix.2;
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();

            Ok(build_ordinary_list(state, block_metadata, OrdinaryListLayout { start, end, offset, ordered: true, has_own_metadata: !is_nested }, items))
        }

        // Parse first item content after marker has been consumed by ordered_list
        // marker_start is the position where the marker began, for correct location tracking
        rule ordered_list_item_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool, marker: &'input str, marker_start: usize, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = list_continuation_allowed(allow_continuation)
          item:ordered_list_item_with_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_unordered_marker) { item }
        / item:ordered_list_item_no_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_unordered_marker) { item }

        rule ordered_list_rest_prefix(parent_unordered_marker: Option<&'input str>, base_marker: &str)
        = parent_is_some(parent_unordered_marker) !at_list_separator() !eol() comment_line()* !at_unordered_marker_ahead()
        / parent_is_some(parent_unordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_ordered_marker(base_marker) !at_unordered_marker_ahead()
        // Branch: no unordered parent
        / parent_is_none(parent_unordered_marker) !at_list_separator() !eol() comment_line()*
        / parent_is_none(parent_unordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_ordered_marker(base_marker)

        rule ordered_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>, allow_continuation: bool, base_marker: &str) -> Result<(ListItem<'input>, usize), Error>
        = ordered_list_rest_prefix(parent_unordered_marker, base_marker)
          item:ordered_list_item(offset, block_metadata, allow_continuation, parent_unordered_marker) { item }


        // Note: The `*_with_continuation` and `*_no_continuation` variants exist because
        // PEG parsers are greedy - nested items must NOT consume explicit continuations
        // that belong to their parent. Attempting to handle this in semantic actions
        // (by always parsing continuations then discarding them) would consume input
        // needed by the parent rule. This structural duplication is intentional.
        rule unordered_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = list_continuation_allowed(allow_continuation)
          item:unordered_list_item_with_continuation(offset, block_metadata, parent_ordered_marker) { item }
        / item:unordered_list_item_no_continuation(offset, block_metadata, parent_ordered_marker) { item }

        rule unordered_list_item_with_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = item_start:position!() whitespace()* marker:unordered_list_marker()
          head:unordered_list_head(offset, block_metadata, marker, item_start, item_start, parent_ordered_marker)
        // Try to parse nested list (ordered, or unordered with deeper markers)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Nested items cannot consume parent-level continuations (allow_continuation: false)
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for explicit_continuation
        nested:(!at_list_separator() nested_content:unordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        // Try to parse explicit continuations (+ marker)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Parent items accept both:
        // - Immediate continuations (0 empty lines) for content directly after principal text
        // - Ancestor continuations (1+ empty lines) for content that bubbles up from nested items
        // Use * to match a mixed sequence of immediate and ancestor continuations
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata, Some(ListMarker::Unordered(marker)))
            / list_explicit_continuation_ancestor(offset, block_metadata, Some(ListMarker::Unordered(marker)))
            / unordered_list_nested_after_attachment(offset, block_metadata, marker, parent_ordered_marker)
        ) { cont })*
        list_dangling_continuation()?
        description_parent_continuation(offset)?
        {

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            head.finish(state, blocks, span_end, offset)
        }

        // Version with immediate continuations only (for nested items)
        // Nested items consume continuations with 0 empty lines (immediate attachment).
        // Continuations with 1+ empty lines bubble up to ancestor items.
        rule unordered_list_item_no_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = item_start:position!() whitespace()* marker:unordered_list_marker()
          head:unordered_list_head(offset, block_metadata, marker, item_start, item_start, parent_ordered_marker)
        // Nested items can still have nested lists, but those also cannot consume parent continuations
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for immediate_continuation
        nested:(!at_list_separator() nested_content:unordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        // Parse immediate continuations (0 empty lines) - these attach to this item
        // Ancestor continuations (1+ empty lines) bubble up to parent items
        immediate_continuations:(!at_list_separator() cont:(list_explicit_continuation_immediate(offset, block_metadata, Some(ListMarker::Unordered(marker))) / unordered_list_nested_after_attachment(offset, block_metadata, marker, parent_ordered_marker)) { cont })*
        description_parent_continuation(offset)?
        {

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            head.finish(state, blocks, span_end, offset)
        }

        // After-marker variants: used when marker has already been consumed by parent rule
        // These are identical to the regular variants except they take marker as a parameter
        // instead of parsing it, and start after the marker position
        rule unordered_list_item_with_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = empty_start:position!()
          head:unordered_list_head(offset, block_metadata, marker, marker_start, empty_start, parent_ordered_marker)
        nested:(!at_list_separator() nested_content:unordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata, Some(ListMarker::Unordered(marker)))
            / list_explicit_continuation_ancestor(offset, block_metadata, Some(ListMarker::Unordered(marker)))
            / unordered_list_nested_after_attachment(offset, block_metadata, marker, parent_ordered_marker)
        ) { cont })*
        list_dangling_continuation()?
        description_parent_continuation(offset)?
        {

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            head.finish(state, blocks, span_end, offset)
        }

        rule unordered_list_item_no_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = empty_start:position!()
          head:unordered_list_head(offset, block_metadata, marker, marker_start, empty_start, parent_ordered_marker)
        nested:(!at_list_separator() nested_content:unordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        immediate_continuations:(!at_list_separator() cont:(list_explicit_continuation_immediate(offset, block_metadata, Some(ListMarker::Unordered(marker))) / unordered_list_nested_after_attachment(offset, block_metadata, marker, parent_ordered_marker)) { cont })*
        description_parent_continuation(offset)?
        {

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            head.finish(state, blocks, span_end, offset)
        }

        /// Parse nested content within an unordered list item (e.g., nested ordered or unordered list)
        /// Note: allow_continuation is false to prevent nested items from consuming parent-level continuations
        /// current_marker: the marker of the parent unordered list item (e.g., "*" or "**")
        /// parent_ordered_marker: the marker of an ancestor ordered list (if any), to prevent
        /// consuming sibling ordered markers that belong to a parent ordered list context
        rule unordered_list_item_nested_after_principal(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_ordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        = entry:unordered_child_after_principal(offset, block_metadata, current_marker, parent_ordered_marker)
          list:native_list_entry(offset, &entry, block_metadata) { Some(list) }

        rule unordered_child_after_principal(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_ordered_marker: Option<&'input str>) -> ListEntry<'input>
        = eol() comment_line()* nested:(
            unordered_child_entry_with_metadata(offset, block_metadata, current_marker, parent_ordered_marker)
            / unordered_child_entry(offset, block_metadata, current_marker, parent_ordered_marker)
          ) { nested }
        / eol()+ !at_callout_list_item() nested:unordered_child_entry(offset, block_metadata, current_marker, parent_ordered_marker) { nested }

        rule unordered_list_nested_after_attachment(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, parent_ordered_marker: Option<&'input str>) -> Result<Block<'input>, Error>
        = entry:unordered_child_after_attachment(offset, block_metadata, marker, parent_ordered_marker)
          list:native_list_entry(offset, &entry, block_metadata) { list }

        rule unordered_child_after_attachment(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, parent_ordered_marker: Option<&'input str>) -> ListEntry<'input>
        = eol() &nested_unordered_child_after_metadata(offset, marker, parent_ordered_marker)
          entry:unordered_child_entry_with_metadata(offset, block_metadata, marker, parent_ordered_marker) { entry }


        /// Parse a nested unordered list where all items have markers deeper than parent_marker.
        /// This is used to parse same-type nesting (e.g., ** inside *) as hierarchical content
        /// rather than flat siblings, enabling proper ancestor continuation handling.
        /// Uses allow_continuation=false to prevent nested items from consuming parent continuations.
        rule unordered_list_nested(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, parent_ordered_marker: Option<&'input str>, has_own_metadata: bool) -> Result<Block<'input>, Error>
        // Parse first item - must have a deeper marker than parent_marker
        = &at_deeper_unordered_marker(parent_marker)
          prefix:unordered_list_prefix(offset)
          first:unordered_list_item_after_marker(offset, block_metadata, false, prefix.0, prefix.1, parent_ordered_marker)
          // Parse rest items - only those at same level as prefix.0 (not deeper, not shallower than parent)
          rest:(unordered_list_nested_rest_item(offset, block_metadata, parent_marker, prefix.0, parent_ordered_marker))*
        {
            tracing::debug!("Found nested unordered list block");
            state.ordinary_list_scope = prefix.2;
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();

            Ok(build_ordinary_list(state, block_metadata, OrdinaryListLayout { start, end, offset, ordered: false, has_own_metadata }, items))
        }

        /// Parse rest items in a nested unordered list.
        /// Items must be deeper than parent_marker and at same-or-deeper level as prefix.0.
        /// Stops when we encounter a marker at or shallower than parent_marker.
        rule unordered_list_nested_rest_prefix(parent_marker: &str, base_marker: &str)
        = !at_list_separator() !eol() comment_line()*
          // Must not be at shallower-or-equal to parent (that would end the nested list)
          !at_shallower_or_equal_unordered_marker(parent_marker)
        // Case 2: Blank lines present - only accept same-level items (deeper would be its own nesting)
        / !at_list_separator() eol()+ comment_line()*
          // Must not be at shallower-or-equal to parent
          !at_shallower_or_equal_unordered_marker(parent_marker)
          // Must not be deeper than base (that would be nested inside this item)
          !at_deeper_unordered_marker(base_marker)

        rule unordered_list_nested_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, base_marker: &str, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = unordered_list_nested_rest_prefix(parent_marker, base_marker)
          item:unordered_list_item(offset, block_metadata, false, parent_ordered_marker) { item }


        // Helper rule to check if we're at a marker that's shallower than or equal to parent_marker
        // Used to terminate nested lists when encountering parent-level or ancestor-level items
        rule at_shallower_or_equal_unordered_marker(parent_marker: &str)
        = whitespace()* marker:unordered_list_marker() whitespace() {?
            if marker.len() <= parent_marker.len() { Ok(()) } else { Err("deeper") }
        }

        // See comment on unordered_list_item for why *_with/without_continuation variants exist.
        rule ordered_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = list_continuation_allowed(allow_continuation)
          item:ordered_list_item_with_continuation(offset, block_metadata, parent_unordered_marker) { item }
        / item:ordered_list_item_no_continuation(offset, block_metadata, parent_unordered_marker) { item }

        rule ordered_list_item_with_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = item_start:position!() whitespace()* marker:ordered_list_marker()
          head:ordered_list_head(offset, block_metadata, marker, item_start, item_start, parent_unordered_marker)
        // Try to parse nested list (unordered, or ordered with deeper markers)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Nested items cannot consume parent-level continuations (allow_continuation: false)
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for explicit_continuation
        nested:(!at_list_separator() nested_content:ordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        // Try to parse explicit continuations (+ marker)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Parent items accept both:
        // - Immediate continuations (0 empty lines) for content directly after principal text
        // - Ancestor continuations (1+ empty lines) for content that bubbles up from nested items
        // Use * to match a mixed sequence of immediate and ancestor continuations
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata, Some(ListMarker::Ordered(marker)))
            / list_explicit_continuation_ancestor(offset, block_metadata, Some(ListMarker::Ordered(marker)))
            / ordered_list_nested_after_attachment(offset, block_metadata, marker, parent_unordered_marker)
        ) { cont })*
        list_dangling_continuation()?
        description_parent_continuation(offset)?
        {

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            head.finish(state, blocks, span_end, offset)
        }

        // Version with immediate continuations only (for nested items)
        // Nested items consume continuations with 0 empty lines (immediate attachment).
        // Continuations with 1+ empty lines bubble up to ancestor items.
        rule ordered_list_item_no_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = item_start:position!() whitespace()* marker:ordered_list_marker()
          head:ordered_list_head(offset, block_metadata, marker, item_start, item_start, parent_unordered_marker)
        // Nested items can still have nested lists, but those also cannot consume parent continuations
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for immediate_continuation
        nested:(!at_list_separator() nested_content:ordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        // Parse immediate continuations (0 empty lines) - these attach to this item
        // Ancestor continuations (1+ empty lines) bubble up to parent items
        immediate_continuations:(!at_list_separator() cont:(list_explicit_continuation_immediate(offset, block_metadata, Some(ListMarker::Ordered(marker))) / ordered_list_nested_after_attachment(offset, block_metadata, marker, parent_unordered_marker)) { cont })*
        description_parent_continuation(offset)?
        {

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            head.finish(state, blocks, span_end, offset)
        }

        // After-marker variants for ordered lists: used when marker has already been consumed by parent rule
        rule ordered_list_item_with_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = empty_start:position!()
          head:ordered_list_head(offset, block_metadata, marker, marker_start, empty_start, parent_unordered_marker)
        nested:(!at_list_separator() nested_content:ordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata, Some(ListMarker::Ordered(marker)))
            / list_explicit_continuation_ancestor(offset, block_metadata, Some(ListMarker::Ordered(marker)))
            / ordered_list_nested_after_attachment(offset, block_metadata, marker, parent_unordered_marker)
        ) { cont })*
        list_dangling_continuation()?
        description_parent_continuation(offset)?
        {

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            head.finish(state, blocks, span_end, offset)
        }

        rule ordered_list_item_no_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = empty_start:position!()
          head:ordered_list_head(offset, block_metadata, marker, marker_start, empty_start, parent_unordered_marker)
        nested:(!at_list_separator() nested_content:ordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        immediate_continuations:(!at_list_separator() cont:(list_explicit_continuation_immediate(offset, block_metadata, Some(ListMarker::Ordered(marker))) / ordered_list_nested_after_attachment(offset, block_metadata, marker, parent_unordered_marker)) { cont })*
        description_parent_continuation(offset)?
        {

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            head.finish(state, blocks, span_end, offset)
        }

        /// Parse nested content within an ordered list item (e.g., nested unordered or ordered list)
        /// Note: allow_continuation is false to prevent nested items from consuming parent-level continuations
        /// current_marker: the marker of the parent ordered list item (e.g., "." or "..")
        /// parent_unordered_marker: the marker of an ancestor unordered list (if any), to prevent
        /// consuming sibling unordered markers that belong to a parent unordered list context
        rule ordered_list_item_nested_after_principal(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_unordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        = entry:ordered_child_after_principal(offset, block_metadata, current_marker, parent_unordered_marker)
          list:native_list_entry(offset, &entry, block_metadata) { Some(list) }

        rule ordered_child_after_principal(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_unordered_marker: Option<&'input str>) -> ListEntry<'input>
        = eol() comment_line()* nested:(
            ordered_child_entry_with_metadata(offset, block_metadata, current_marker, parent_unordered_marker)
            / ordered_child_entry(offset, block_metadata, current_marker, parent_unordered_marker)
          ) { nested }
        / eol()+ !at_callout_list_item() nested:ordered_child_entry(offset, block_metadata, current_marker, parent_unordered_marker) { nested }

        rule ordered_list_nested_after_attachment(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, parent_unordered_marker: Option<&'input str>) -> Result<Block<'input>, Error>
        = entry:ordered_child_after_attachment(offset, block_metadata, marker, parent_unordered_marker)
          list:native_list_entry(offset, &entry, block_metadata) { list }

        rule ordered_child_after_attachment(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, parent_unordered_marker: Option<&'input str>) -> ListEntry<'input>
        = eol() &nested_ordered_child_after_metadata(offset, marker, parent_unordered_marker)
          entry:ordered_child_entry_with_metadata(offset, block_metadata, marker, parent_unordered_marker) { entry }


        /// Parse a nested ordered list where all items have markers deeper than parent_marker.
        /// This is used to parse same-type nesting (e.g., .. inside .) as hierarchical content
        /// rather than flat siblings, enabling proper ancestor continuation handling.
        /// Uses allow_continuation=false to prevent nested items from consuming parent continuations.
        rule ordered_list_nested(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, parent_unordered_marker: Option<&'input str>, has_own_metadata: bool) -> Result<Block<'input>, Error>
        // Parse first item - must have a deeper marker than parent_marker
        = &at_deeper_ordered_marker(parent_marker)
          prefix:ordered_list_prefix(offset)
          first:ordered_list_item_after_marker(offset, block_metadata, false, prefix.0, prefix.1, parent_unordered_marker)
          // Parse rest items - only those at same level as prefix.0 (not deeper, not shallower than parent)
          rest:(ordered_list_nested_rest_item(offset, block_metadata, parent_marker, prefix.0, parent_unordered_marker))*
        {
            tracing::debug!("Found nested ordered list block");
            state.ordinary_list_scope = prefix.2;
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();

            Ok(build_ordinary_list(state, block_metadata, OrdinaryListLayout { start, end, offset, ordered: true, has_own_metadata }, items))
        }

        /// Parse rest items in a nested ordered list.
        /// Items must be deeper than parent_marker and at same-or-deeper level as prefix.0.
        /// Stops when we encounter a marker at or shallower than parent_marker.
        rule ordered_list_nested_rest_prefix(parent_marker: &str, base_marker: &str)
        = !at_list_separator() !eol() comment_line()*
          // Must not be at shallower-or-equal to parent (that would end the nested list)
          !at_shallower_or_equal_ordered_marker(parent_marker)
        // Case 2: Blank lines present - only accept same-level items (deeper would be its own nesting)
        / !at_list_separator() eol()+ comment_line()*
          // Must not be at shallower-or-equal to parent
          !at_shallower_or_equal_ordered_marker(parent_marker)
          // Must not be deeper than base (that would be nested inside this item)
          !at_deeper_ordered_marker(base_marker)

        rule ordered_list_nested_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, base_marker: &str, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = ordered_list_nested_rest_prefix(parent_marker, base_marker)
          item:ordered_list_item(offset, block_metadata, false, parent_unordered_marker) { item }


        // Helper rule to check if we're at a marker that's shallower than or equal to parent_marker
        // Used to terminate nested lists when encountering parent-level or ancestor-level items
        rule at_shallower_or_equal_ordered_marker(parent_marker: &str)
        = whitespace()* marker:ordered_list_marker() whitespace() {?
            if marker.len() <= parent_marker.len() { Ok(()) } else { Err("deeper") }
        }

        // Only a column-one marker with nonempty text starts a callout item.
        // This probe is also used by list boundaries without parsing child macros.
        rule at_callout_list_item()
        = &(callout_list_marker() whitespace()+ [^' ' | '\t' | '\r' | '\n'])

        // Stop child lists and attached paragraphs before the next parent item,
        // leaving its marker for callout_list_rest_item. The offset check keeps
        // markers in reparsed delimited blocks outside this boundary.
        rule at_callout_parent_item(offset: usize)
        = at_callout_list_item() {?
            (state.callout_list_offset == Some(state.list_scope_offset(offset))).then_some(()).ok_or("not a callout parent")
        }

        // Save the enclosing scope for callout_list to restore after parsing its
        // items. Keep pending verbatim references local for validation,
        // since child listings add their own references to the shared catalog.
        rule enter_callout_list(offset: usize) -> CalloutScope
        = {
            let parent = state.callout_list_offset.replace(state.list_scope_offset(offset));
            (parent, std::mem::take(&mut state.pending_callouts))
        }

        rule callout_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        // Check the item boundary before changing callout scope.
        = at_callout_list_item() structural_block_boundaries(block_metadata)
        context:enter_callout_list(offset)
        first:callout_list_item(offset, block_metadata)
        rest:(callout_list_rest_item(offset, block_metadata))*
        {
            let (parent, callouts) = context;
            state.callout_list_offset = parent;
            state.pending_callouts.clear();
            tracing::debug!("Found callout list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, _, item_end)| *item_end);

            Ok(build_callout_list(state, block_metadata, state.create_location(start+offset, end+offset), content, &callouts))
        }

        rule callout_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<(CalloutListItem<'input>, String, usize), Error>
        = eol()+ item:callout_list_item(offset, block_metadata)
        {?
            Ok(item)
        }

        // Ordinary lists can contain callouts, but a marker belonging to an
        // active callout parent must be left for that parent's next item.
        rule nested_callout_list(offset: usize, metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = !at_callout_parent_item(offset) start:position!()
          child_metadata:nested_list_content_metadata(metadata)
          list:callout_list(start, offset, &child_metadata) { list }

        // Probe for an implicit child without consuming input. A parent marker
        // takes precedence even when its text contains a description delimiter.
        rule callout_child_marker(offset: usize)
        = !at_callout_parent_item(offset) (
            &(whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace())
            / check_line_is_description_list(offset)
        )

        // Attach an implicit child before callout_list_item handles explicit `+`
        // blocks. Metadata must be adjacent to the principal text; a blank line
        // before metadata starts a separate list.
        rule callout_list_nested(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = entry:callout_child_after_principal(offset, block_metadata)
          list:native_list_entry(offset, &entry, block_metadata) { list }

        rule callout_child_after_principal(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> ListEntry<'input>
        = eol() child:(
            &((attribute_or_anchor_line_match() eol()*)+ callout_child_marker(offset))
            start:position!()
            metadata:parsed_nested_list_metadata(offset, block_metadata.parent_section_level)
            kind:callout_child_kind(offset) body_offset:position!()
            { ListEntry { start, body_offset, metadata: Some(Box::new(metadata)), kind } }
            / eol()* callout_child_marker(offset) start:position!()
            metadata:nested_list_content_metadata(block_metadata)
            kind:callout_child_kind(offset) body_offset:position!()
            { ListEntry { start, body_offset, metadata: Some(Box::new(metadata)), kind } }
        ) { child }

        rule callout_child_kind(offset: usize) -> ListKind<'input>
        = &(whitespace()* unordered_list_marker() whitespace())
        { ListKind::Unordered(OrdinaryListKind { parent_other: None, parent_same: None, allow_continuation: false, has_own_metadata: true }) }
        / &(whitespace()* ordered_list_marker() whitespace())
        { ListKind::Ordered(OrdinaryListKind { parent_other: None, parent_same: None, allow_continuation: false, has_own_metadata: true }) }
        / &description_list_ahead(offset) { ListKind::Description }

        // Inherit section context and text settings without copying the parent's
        // ID, title, or style onto a child that has no metadata of its own.
        rule nested_list_content_metadata(parent: &BlockParsingMetadata<'input>) -> BlockParsingMetadata<'input>
        = {
            BlockParsingMetadata {
                parent_section_level: parent.parent_section_level,
                substitutions: parent.substitutions,
                hardbreaks: parent.hardbreaks,
                ..BlockParsingMetadata::default()
            }
        }

        rule callout_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<CalloutItem<'input>, Error>
        = head:callout_list_head(offset, block_metadata)
        nested:(!at_list_separator() child:callout_list_nested(offset, block_metadata) { child })?
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata, None)
            / list_explicit_continuation_ancestor(offset, block_metadata, None)
        ) { cont })*
        list_dangling_continuation()?
        description_parent_continuation(offset)?
        {
            let blocks = nested.into_iter().chain(explicit_continuations).collect();
            head.finish(state, blocks, span_end, offset)
        }

        rule callout_list_head(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> CalloutItemHead<'input>
        = at_callout_list_item()
        marker:callout_list_marker()
        whitespace()
        first_line_start:position!()
        // Parse first line (principal text)
        first_line:$((!(eol()) [_])*)
        // Parse continuation lines that are part of the same paragraph
        // Stop at list markers, explicit continuations, blank lines, or block attributes.
        continuation_lines:(
            eol()
            line:callout_list_principal_line(offset) { line }
        )*
        first_line_end:position!()
        principal:list_item_principal(offset, block_metadata, first_line_start, first_line_end, first_line, &continuation_lines)
        {
            CalloutItemHead {
                principal, marker, start: span_start,
                end: calculate_item_end(first_line.is_empty() && continuation_lines.is_empty(), span_start, first_line_end),
            }
        }

        rule callout_list_principal_line(offset: usize) -> &'input str
        = !at_callout_list_item()
          !check_line_is_description_list(offset)
          !(whitespace()* (unordered_list_marker() / ordered_list_marker() / "[" / "+" whitespace()* eol() / eol()))
          line:$((!eol() [_])*) { line }

        rule unordered_list_head(offset: usize, metadata: &BlockParsingMetadata<'input>, marker: &'input str, item_start: usize, empty_start: usize, parent_ordered_marker: Option<&'input str>) -> ListItemHead<'input>
        = whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        first_line:$((!eol() [_])*)
        continuation_lines:unordered_list_principal_continuation(offset, marker, parent_ordered_marker)*
        first_line_end:position!()
        principal:list_item_principal(offset, metadata, first_line_start, first_line_end, first_line, &continuation_lines)
        {
            ListItemHead {
                principal, marker, checked, start: item_start,
                end: calculate_item_end(first_line.is_empty() && continuation_lines.is_empty(), empty_start, first_line_end),
            }
        }

        rule ordered_list_head(offset: usize, metadata: &BlockParsingMetadata<'input>, marker: &'input str, item_start: usize, empty_start: usize, parent_unordered_marker: Option<&'input str>) -> ListItemHead<'input>
        = whitespace()
        first_line_start:position!()
        first_line:$((!eol() [_])*)
        continuation_lines:ordered_list_principal_continuation(offset, marker, parent_unordered_marker)*
        first_line_end:position!()
        principal:list_item_principal(offset, metadata, first_line_start, first_line_end, first_line, &continuation_lines)
        {
            ListItemHead {
                principal, marker, checked: None, start: item_start,
                end: calculate_item_end(first_line.is_empty() && continuation_lines.is_empty(), empty_start, first_line_end),
            }
        }

        // Run before child parsing to register footnotes in source order;
        // the item's final action runs after its children are parsed.
        rule list_item_principal(offset: usize, metadata: &BlockParsingMetadata<'input>, start: usize, end: usize, first: &'input str, rest: &[&'input str]) -> Result<Vec<InlineNode<'input>>, Error>
        = {
            let text = assemble_principal_text(state, first, rest);
            if text.trim().is_empty() {
                Ok(Vec::new())
            } else {
                process_inlines(state, metadata, start, end, offset, text).map(|(nodes, _)| nodes)
            }
        }

        rule checklist_item() -> ListItemCheckedStatus
            = checked:(("[x]" / "[X]" / "[*]") { ListItemCheckedStatus::Checked } / "[ ]" { ListItemCheckedStatus::Unchecked }) whitespace()
        {
            checked
        }

        rule check_line_is_description_list(offset: usize)
        = pos:position!() {?
            if state.source_tail(pos + offset).is_some_and(|tail| find_dlist_marker(tail.as_bytes(), 0)) {
                Ok(())
            } else {
                Err("no dlist marker on current line")
            }
        }

        rule at_description_parent_item(offset: usize)
        = at_description_scope_item(offset, state.description_list_scope)

        rule at_description_scope_item(offset: usize, parent: Option<DescriptionListScope<'input>>)
        = scope:({? parent.filter(|scope| scope.offset == state.list_scope_offset(offset)).ok_or("no description parent") })
          (!(description_list_marker() (eol() / " " / ![_]) / eol()) [_])+
          marker:description_list_marker() &(eol() / " " / ![_]) {?
              scope.contains(marker).then_some(()).ok_or("not a description parent item")
          }

        rule enter_description_list(offset: usize) -> Option<DescriptionListScope<'input>>
        = {
            let parent = state.description_list_scope;
            state.description_list_scope = Some(parent.filter(|scope| scope.offset == state.list_scope_offset(offset))
                .unwrap_or_else(|| DescriptionListScope::new(state.list_scope_offset(offset))));
            parent
        }

        rule select_description_marker(marker: &'input str)
        = {
            if let Some(scope) = state.description_list_scope.as_mut() {
                scope.select(marker);
            }
        }

        rule description_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = !at_list_item_start() !at_callout_list_item() !at_description_parent_item(offset)
          check_line_is_description_list(offset)
        parent_scope:enter_description_list(offset)
        first_item:description_list_item(offset, block_metadata)
        additional_items:description_list_additional_items(offset, block_metadata, parent_scope)*
        {
            state.description_list_scope = parent_scope;
            tracing::debug!("Found description list block with auto-attachment support");
            let mut items = vec![first_item?];

            for additional in additional_items {
                items.push(additional?);
            }

            let actual_end = items.last().map_or(span_end, |item| {
                let loc_end = item.location.absolute_end;
                loc_end - offset
            });

            Ok(build_description_list(block_metadata, state.create_location(start+offset, actual_end+offset), items))
        }

        // Parse additional description list items (after potential auto-attached content)
        //
        // !at_dlist_block_boundary() prevents continuing the list when a blank line is
        // followed by block attributes. This allows attributes to apply to a new list.
        rule description_list_additional_prefix(offset: usize, parent_scope: Option<DescriptionListScope<'input>>)
        = !at_dlist_block_boundary()
        eol()*
        !at_description_scope_item(offset, parent_scope)
        !attribute_or_anchor_line_match()
        !at_callout_list_item()
        !at_list_item_start()
        check_line_is_description_list(offset)

        rule description_list_additional_items(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_scope: Option<DescriptionListScope<'input>>) -> Result<DescriptionListItem<'input>, Error>
        = description_list_additional_prefix(offset, parent_scope)
          item:description_list_item(offset, block_metadata) { item }

        // A missing principal can start after blank lines, with the same list boundaries.
        rule description_list_principal_line(offset: usize)
        = !eol()
        !at_callout_list_item()
        !check_line_is_description_list(offset)
        !(whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace())
        !("+" whitespace()* (eol() / ![_]))
        !(example_delimiter() (eol() / ![_]))
        !listing_delimiter()
        !literal_delimiter()
        !sidebar_delimiter()
        !quote_delimiter()
        !pass_delimiter()
        !comment_delimiter()
        !table_delimiter()
        !(open_delimiter() (whitespace()* eol()))
        !markdown_code_delimiter()
        !attribute_or_anchor_line_match()

        rule description_list_term_source() -> DescriptionTermSource<'input>
        = term_start:position!()
          term:$((!(description_list_marker() (eol() / " " / ![_]) / eol()) [_])+)
          term_end:position!()
          delim_start:position!() delimiter:description_list_marker() delim_end:position!()
        { DescriptionTermSource { raw: term, start: term_start, end: term_end, delimiter_start: delim_start, delimiter_end: delim_end, delimiter } }

        rule description_list_head(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> DescriptionItemHead<'input>
        = term:description_list_term_source() select_description_marker(term.delimiter)
        whitespace()*
        principal_start:position!()
        principal_content:$(
            (eol()+ &description_list_principal_line(offset))?
            (!eol() [_])*
            // Implicit text continuation: consume subsequent non-blank lines that
            // aren't new dlist entries, list items, continuation markers, or block
            // delimiters. This mirrors paragraph multi-line handling but with
            // dlist-specific stop conditions.
            (eol() description_list_principal_line(offset)
             (!eol() [_])+                             // continuation line content
            )*
        )
        principal:description_list_principal(offset, block_metadata, term.raw, term.start, term.end, principal_content, principal_start)
        {
            DescriptionItemHead { source: DescriptionItemSource { term, principal_content, principal_start }, principal }
        }

        rule description_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<DescriptionListItem<'input>, Error>
        = head:description_list_head(offset, block_metadata)
          attached:description_list_attached_content(offset, block_metadata, head.source.term.delimiter)*
        { head.finish(state, attached, offset) }

        // As with list_item_principal, process parent macros before attachments.
        rule description_list_principal(offset: usize, block_metadata: &BlockParsingMetadata<'input>, term: &'input str, term_start: usize, term_end: usize, principal_content: &'input str, principal_start: usize) -> Result<(Vec<InlineNode<'input>>, Vec<InlineNode<'input>>), Error>
        = {
            let trimmed_term = term.trim();
            let leading_whitespace = term.len() - term.trim_start().len();
            let term_start = term_start + leading_whitespace;
            let term_end = term_end - (term.len() - term.trim_end().len());
            let (term, _) = process_inlines(
                state,
                block_metadata,
                term_start,
                term_end,
                offset,
                trimmed_term,
            )?;

            let trimmed_principal = principal_content.trim();
            let principal_text = if trimmed_principal.is_empty() {
                Vec::new()
            } else {
                let content_start = principal_start + principal_content.len()
                    - principal_content.trim_start().len();
                let (principal, _) = process_inlines(
                    state,
                    block_metadata,
                    content_start,
                    content_start + trimmed_principal.len(),
                    offset,
                    trimmed_principal,
                )?;
                principal
            };

            Ok((term, principal_text))
        }

        rule description_list_attached_content(offset: usize, block_metadata: &BlockParsingMetadata<'input>, delimiter: &'input str) -> Result<Vec<Block<'input>>, Error>
        = eol() content:(
            // Explicit continuation - this uses +, allows any content including delimited
            // blocks
            description_list_explicit_continuation(offset, block_metadata, delimiter)
            // Auto-attach lists (even with blank lines before them)
            / description_list_auto_attached_list(offset, block_metadata)
        )
        {
            content
        }

        rule description_list_auto_attached_list(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Vec<Block<'input>>, Error>
        = entry:description_auto_entry(offset, block_metadata)
          list:native_list_entry(offset, &entry, block_metadata) { Ok(vec![list?]) }

        rule description_auto_entry(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> ListEntry<'input>
        = eol()* entry:description_auto_entry_with_metadata(offset, block_metadata) { entry }
        / eol()* !at_ordinary_parent_item(offset) start:position!()
          kind:description_ordinary_child_kind(offset, false) body_offset:position!()
          { ListEntry { start, body_offset, metadata: None, kind } }
        / !at_callout_parent_item(offset) &at_callout_list_item() start:position!()
          metadata:nested_list_content_metadata(block_metadata)
          { ListEntry { start, body_offset: start, metadata: Some(Box::new(metadata)), kind: ListKind::Callout } }

        rule description_ordinary_child_kind(offset: usize, own: bool) -> ListKind<'input>
        = scope:ordinary_list_scope(offset) kind:(
            parent_is_some(scope.unordered) &at_deeper_unordered_marker(scope.unordered.unwrap_or_default())
            { ListKind::Unordered(OrdinaryListKind { parent_other: scope.ordered, parent_same: scope.unordered, allow_continuation: false, has_own_metadata: own }) }
            / parent_is_none(scope.unordered) &(whitespace()* unordered_list_marker() whitespace())
            { ListKind::Unordered(OrdinaryListKind { parent_other: scope.ordered, parent_same: None, allow_continuation: true, has_own_metadata: own }) }
            / parent_is_some(scope.ordered) &at_deeper_ordered_marker(scope.ordered.unwrap_or_default())
            { ListKind::Ordered(OrdinaryListKind { parent_other: scope.unordered, parent_same: scope.ordered, allow_continuation: false, has_own_metadata: own }) }
            / parent_is_none(scope.ordered) &(whitespace()* ordered_list_marker() whitespace())
            { ListKind::Ordered(OrdinaryListKind { parent_other: scope.unordered, parent_same: None, allow_continuation: true, has_own_metadata: own }) }
        ) { kind }

        rule description_auto_entry_with_metadata(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> ListEntry<'input>
        = &((attribute_or_anchor_line_match() eol()*)+ !at_ordinary_parent_item(offset)
            (at_list_item_start() / !at_callout_parent_item(offset) at_callout_list_item()))
          start:position!() metadata:parsed_nested_list_metadata(offset, block_metadata.parent_section_level)
          kind:(
              &at_callout_list_item() structural_block_boundaries(&metadata) { ListKind::Callout }
              / kind:description_ordinary_child_kind(offset, true) { kind }
          ) body_offset:position!()
        { ListEntry { start, body_offset, metadata: Some(Box::new(metadata)), kind } }

        // A blank line before `+` returns from a nested description to its ordinary parent.
        rule description_continuation_gap(offset: usize)
        = gap:$(eol()*) {?
            (gap.is_empty() || state.ordinary_list_scope.is_none_or(|scope| scope.offset != state.list_scope_offset(offset)))
                .then_some(()).ok_or("continuation belongs to ordinary parent")
        }

        rule description_continuation_prefix(offset: usize)
        = description_continuation_gap(offset) "+" eol()

        // Parse one or more explicit continuations for description lists
        // Same pattern as list_explicit_continuation: + marker followed by a single block
        // Uses block_in_continuation to prevent lists inside continuations from consuming
        // further continuations that belong to the parent item
        rule description_list_explicit_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, delimiter: &'input str) -> Result<Vec<Block<'input>>, Error>
        = continuations:(
            description_continuation_prefix(offset)
            block:block_in_continuation(offset, block_metadata.parent_section_level, Some(ListMarker::Description(delimiter)))
            { block }
          )+
        {
            tracing::debug!(count = continuations.len(), "Description list explicit continuation blocks");
            Ok(continuations.into_iter().filter_map(Result::ok).collect())
        }

        // Parse a single immediate continuation (0 empty lines before +)
        // These attach to the current (most recent) list item per AsciiDoc spec.
        // Uses block_in_continuation to prevent lists inside continuations from consuming
        // further continuations that belong to the parent item.
        // Pattern: exactly one newline before + (content\n+\nblock)
        rule list_explicit_continuation_immediate(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: Option<ListMarker<'input>>) -> Result<Block<'input>, Error>
        = list_continuation_immediate_prefix(offset)
          block:block_in_continuation(offset, block_metadata.parent_section_level, parent_marker)
        {
            tracing::debug!("List immediate continuation block (0 empty lines)");
            block
        }

        // Parse a single ancestor continuation (1+ empty lines before +)
        // Per AsciiDoc spec: each empty line before + moves attachment up one nesting level.
        // 1 empty line = parent, 2 empty lines = grandparent, etc.
        // Uses block_in_continuation to prevent lists inside continuations from consuming
        // further continuations that belong to the parent item.
        // Pattern: two or more newlines before + (content\n\n+\nblock)
        rule list_explicit_continuation_ancestor(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: Option<ListMarker<'input>>) -> Result<Block<'input>, Error>
        = list_continuation_ancestor_prefix(offset)
          block:block_in_continuation(offset, block_metadata.parent_section_level, parent_marker)
        {
            tracing::debug!("List ancestor continuation block (1+ empty lines)");
            block
        }

        rule list_continuation_immediate_prefix(offset: usize)
        = eol() !eol() "+" eol() !at_description_parent_item(offset)

        rule list_continuation_ancestor_prefix(offset: usize)
        = eol() eol()+ "+" eol() !at_description_parent_item(offset)

        rule description_parent_continuation(offset: usize)
        = eol()+ "+" whitespace()* eol() &at_description_parent_item(offset)

        // Asciidoctor drops a continuation with no attachable block, including one
        // followed only by unused metadata. Leave that metadata for the block sequence
        // so any document-attribute events are retained.
        rule list_dangling_continuation()
        = eol()+ "+" whitespace()* eol()? &(eol() / ![_] / trailing_block_metadata_match())
        {
            tracing::debug!("Dropped dangling list continuation marker");
        }

        // Parse a quoted paragraph: "content" followed by `-- attribution[, citation]`
        //
        // This matches the AsciiDoc shorthand syntax for blockquotes:
        // ```
        // "I hold it that a little rebellion now and then is a good thing."
        // -- Thomas Jefferson, Papers of Thomas Jefferson
        // ```
        rule quoted_paragraph(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = params:quoted_paragraph_params(start, offset) { build_quote(state, block_metadata, &params) }

        rule quoted_paragraph_params(start: usize, offset: usize) -> QuoteParams<'input>
        = content_start:position!()
          "\"" content:$((!"\"" [_])+) "\"" eol()
          "-- " attr_start:position!() attribution_line:$([^'\n']+)
        {
            QuoteParams {
                start, end: span_end, offset,
                source: QuoteSource::Shorthand { content, content_start },
                attribution: Some(QuoteAttribution::from_line(attribution_line, attr_start)),
            }
        }

        rule markdown_blockquote(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = params:markdown_blockquote_params(start, offset) { build_quote(state, block_metadata, &params) }

        rule markdown_blockquote_params(start: usize, offset: usize) -> QuoteParams<'input>
        = lines:markdown_blockquote_content_line()+ attribution:markdown_blockquote_attribution()?
        { QuoteParams { start, end: span_end, offset, source: QuoteSource::Markdown(lines), attribution } }

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
        /// Returns (author, author_start, Option<(citation, cite_start)>)
        rule markdown_blockquote_attribution() -> QuoteAttribution<'input>
        = "> -- " author_start:position!() author:$([^(',' | '\n')]+) ", " cite_start:position!() citation:$([^'\n']+) ((eol() !">") / ![_]) {
            QuoteAttribution { author: author.trim(), author_start, citation: Some((citation.trim(), cite_start)) }
        }
        / "> -- " author_start:position!() author:$([^'\n']+) ((eol() !">") / ![_]) {
            QuoteAttribution { author: author.trim(), author_start, citation: None }
        }

        // Asciidoctor lets verbatim styles override list and macro syntax.
        // Draft matches those forms before interpreting the style.
        rule styled_verbatim_paragraph(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = !structural_block_boundaries(block_metadata) block:paragraph(start, offset, block_metadata) { block }

        // Asciidoctor keeps apparent headings, delimiters, and metadata inside
        // a styled verbatim paragraph until a blank line or list continuation.
        rule verbatim_paragraph_content() -> &'input str
        = content:$((!(
            eol()*<2,>
            / eol()* ![_]
            / eol() "+" whitespace()* (eol() / ![_])
        ) [_])+) { content }

        rule paragraph(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = block:paragraph_with_list_boundary(start, offset, block_metadata, None) { block }

        // Only Asciidoctor lets a sibling/ancestor interrupt an attached paragraph.
        // Draft uses the same paragraph boundaries inside and outside a list.
        rule attached_list_boundary(offset: usize, parent_marker: Option<ListMarker<'input>>)
        = parent:({?
            if state.options.compatibility_mode == CompatibilityMode::Asciidoctor {
                parent_marker.ok_or("no parent list")
            } else {
                Err("list items do not interrupt attached Draft paragraphs")
            }
        }) candidate:attached_list_marker() {?
            let ordinary_ancestor = matches!(parent, ListMarker::Description(_))
                && state.ordinary_list_scope.filter(|scope| scope.offset == state.list_scope_offset(offset)).is_some_and(|scope| {
                    scope.unordered.is_some_and(|marker| ListMarker::Unordered(marker).is_sibling_or_ancestor(candidate))
                        || scope.ordered.is_some_and(|marker| ListMarker::Ordered(marker).is_sibling_or_ancestor(candidate))
                });
            (parent.is_sibling_or_ancestor(candidate) || ordinary_ancestor).then_some(()).ok_or("not a sibling list item")
        }

        rule attached_list_marker() -> ListMarker<'input>
        = whitespace()* marker:unordered_list_marker() whitespace() { ListMarker::Unordered(marker) }
        / whitespace()* marker:ordered_list_marker() whitespace() { ListMarker::Ordered(marker) }
        / (!(description_list_marker() (whitespace() / eol() / ![_])) [^'\n'])+
          marker:description_list_marker() &(whitespace() / eol() / ![_]) { ListMarker::Description(marker) }

        rule paragraph_text_run(offset: usize)
        = !at_line_start(offset) #{|input, start| {
            let end = paragraph_text_end(input, start);
            if end == start {
                peg::RuleResult::Failed
            } else {
                peg::RuleResult::Matched(end, ())
            }
        }}

        rule paragraph_line_boundary(offset: usize, parent_marker: Option<ListMarker<'input>>)
        = excessive_list_marker()
        / &raw_attributes_line()
        / &anchor_line_match()
        / example_delimiter() &(eol() / ![_])
        / listing_delimiter()
        / literal_delimiter()
        / sidebar_delimiter()
        / quote_delimiter()
        / pass_delimiter()
        / table_delimiter()
        / markdown_code_delimiter()
        / comment_delimiter()
        / open_delimiter() &(whitespace()* eol())
        / at_callout_parent_item(offset)
        / attached_list_boundary(offset, parent_marker)
        / &("+" whitespace()* (eol() / ![_]))  // Only a standalone plus continues a list.

        rule paragraph_with_list_boundary(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: Option<ListMarker<'input>>) -> Result<Block<'input>, Error>
        = admonition:(structural_block_boundaries(block_metadata) value:admonition() { value })?
        content_start:position!()
        content:(
          !structural_block_boundaries(block_metadata) text:verbatim_paragraph_content() { text }
          / $((
            "[[" (!eol() [_])*
            // Only line starts and newlines need block-boundary lookahead.
            // Stop at '[' to retain the anchor-line alternative above.
            / paragraph_text_run(offset)
            / !(
            eol()*<2,>
            / eol()* ![_]
            / eol() paragraph_line_boundary(offset, parent_marker)
            ) [_]
        )+))
        {
            build_paragraph(state, block_metadata, ParagraphParams {
                start, content_start, end: span_end, offset, content, admonition,
            })
        }

        rule admonition() -> (String, usize, usize)
            = variant:$("NOTE" / "WARNING" / "TIP" / "IMPORTANT" / "CAUTION") ": "
        {
            (variant.to_string(), span_start, span_end)
        }

        // Lookahead rule that warns about anchor ID-like patterns containing whitespace.
        //
        // This uses negative lookahead and emits a warning if it detects whitespace. It
        // does not consume the input.
        rule warn_anchor_id_with_whitespace() -> ()
        = &(
            id:$([^'\'' | ',' | ']' | '.' | '#']+)
            {?
                if id.chars().any(char::is_whitespace) {
                    let location = state.create_location(span_start, span_end);
                    state.add_generic_warning_at(
                        format!("anchor id '{id}' contains whitespace which is not allowed, treating as literal text"),
                        location,
                    );
                }
                // Always fail so the lookahead doesn't match - we just want the side
                // effect
                Err::<(), &'static str>("")
            }
        )

        rule anchor(offset: usize) -> Anchor<'input>
        = result:(
            // Double-bracket [[id]] syntax - allows dots in ID since no role shorthand
            // possible.
            //
            // Whitespace is excluded per AsciiDoc documentation at
            // https://docs.asciidoctor.org/asciidoc/latest/attributes/id/#valid-id-characters
            double_open_square_bracket() warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | '[' | ' ' | '\t' | '\n' | '\r']+) comma() reftext:block_anchor_reftext(offset) {
                (id, Some(reftext))
            } /
            double_open_square_bracket() warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | '[' | ' ' | '\t' | '\n' | '\r']+) double_close_square_bracket() {
                (id, None)
            } /
            // Single-bracket [#id] shorthand - exclude '.', '%' as they start role/option
            // shorthands.
            //
            // Only the bare `[#id]` form is an anchor here; `[#id,...]` is NOT — the
            // comma introduces further block attributes (e.g. `[#id,discrete]`), so it
            // must fall through to the attribute-line parser where `#id` becomes the id
            // and the rest are positional/named attributes. Unlike `[[id,reftext]]`, a
            // single-bracket comma does not set a reftext (matching asciidoctor).
            //
            // Whitespace is excluded per AsciiDoc documentation at
            // https://docs.asciidoctor.org/asciidoc/latest/attributes/id/#valid-id-characters
            open_square_bracket() "#" warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | '.' | '%' | ' ' | '\t' | '\n' | '\r']+) close_square_bracket() {
                (id, None)
            }
        )
        end:position!()
        eol()
        {
            let (id, reftext) = result;
            let substituted_id = state.intern_cow(apply_attribute_substitutions(id, HEADER, &state.document_attributes));
            // `end` is captured before the trailing eol() so the anchor's
            // location doesn't include the newline.
            Anchor {
                id: substituted_id,
                xreflabel: None,
                location: state.create_location(span_start + offset, end + offset),
                reftext_source: reftext,
                bibliography_label: None,
                bibliography: false,
            }
        }

        rule block_anchor_reftext(offset: usize) -> usize
        = start:position!() text:$([^'\n' | '\r']+) {?
            text.strip_suffix("]]")
                .and_then(|source| resolve_block_anchor_reftext(source, &state.document_attributes, state.options.compatibility_mode))
                .map(|(value, source_map)| {
                    let source_end = start + offset + text.len() - 2;
                    let text = state.intern_cow(value);
                    crate::grammar::reference_text::capture_block(state, crate::grammar::attributes::ResolvedContentAttribute {
                        text,
                        quote: crate::grammar::attributes::AttributeQuote::Unquoted,
                        source_map: source_map.with_origin(crate::grammar::inline_preprocessor::SourceMap::default(), start + offset),
                        source_range: start + offset..source_end,
                        attributes: Rc::clone(&state.document_attributes),
                    })
                })
                .ok_or("block anchor reference text")
        }

        rule inline_anchor(offset: usize) -> InlineNode<'input>
        = double_open_square_bracket()
        // Whitespace is excluded - IDs must not contain spaces
        warn_anchor_id_with_whitespace()?
        id:$([^'\'' | ',' | ']' | '[' | ' ' | '\t' | '\n' | '\r']+)
        reftext:(
            comma() start:position!() reftext:$([^']']+) {
                Some((start, reftext))
            } /
            {
                None
            }
        )
        double_close_square_bracket()
        {?
            let substituted_id = state.intern_cow(apply_attribute_substitutions(id, HEADER, &state.document_attributes));
            let source = reftext.map(|(start, text)| crate::grammar::reference_text::capture_inline(state, text, start + offset, true));
            let xreflabel = source.map(|id| crate::grammar::reference_text::resolve(state, id)).transpose().map_err(|_| "reference label")?;
            Ok(InlineNode::InlineAnchor(Anchor {
                id: substituted_id,
                xreflabel,
                location: state.create_block_location(span_start, span_end, offset),
                reftext_source: source,
                bibliography_label: None,
                bibliography: false,
            }))
        }

        rule inline_anchor_match() -> ()
        = double_open_square_bracket() [^'\'' | ',' | ']' | '[' | ' ' | '\t' | '\n' | '\r']+ (comma() [^']']+)? double_close_square_bracket()

        rule invalid_bibliography_anchor(offset: usize) -> InlineNode<'input>
        = syntax:$("[[[" [^']' | '\n']* "]]]") {?
            let body = &syntax[3..syntax.len() - 3];
            let id = body.split_once(',').map_or(body, |(id, _)| id);
            if is_valid_bibliography_id(id) {
                Err("valid bibliography anchor")
            } else {
                Ok(InlineNode::PlainText(Plain {
                    content: syntax,
                    location: state.create_block_location(span_start, span_end, offset),
                    escaped: false,
                }))
            }
        }

        rule attributes_line() -> ParsedAttributeList<'input>
        = attributes_line_at(0)

        rule attributes_line_at(offset: usize) -> ParsedAttributeList<'input>
            // Don't match empty [] followed by blank line - that's a list separator, not
            // block attributes. Without this, `[]\n\n` would be parsed as an empty
            // attributes line, breaking list separation
            = !empty_list_separator() attributes:attributes_at(offset) eol() {
                attributes
            }

        rule raw_attributes_line() -> RawAttributeList<'input>
            = !empty_list_separator() block_attribute_start() attributes:raw_attribute_list(BlockAttributeMode::Block) eol() { attributes }

        rule raw_attribute_list(context: BlockAttributeMode) -> RawAttributeList<'input>
            = open_square_bracket() start:position!() source:attribute_list_content(context)
            { RawAttributeList { source, start } }

        // Empty brackets followed by a blank line is a list separator
        rule empty_list_separator()
            = whitespace()* "[" whitespace()* "]" whitespace()* eol() eol()

        pub(in crate::grammar::document) rule attributes() -> ParsedAttributeList<'input>
        = attributes_at(0)

        rule attributes_at(offset: usize) -> ParsedAttributeList<'input>
            = block_attribute_start()
              open_square_bracket()
              content_start:position!()
              content:attribute_list_content(BlockAttributeMode::Block)
            {?
                parse_block_attribute_list(
                    state,
                    content,
                    content_start,
                    BlockAttributeMode::Block,
                    offset,
                ).map_err(|_| "cannot map block attribute locations")
            }

        /// Macro attribute parsing - simpler than block attributes.
        ///
        /// Does NOT support shorthand syntax (.role, #id, %option).
        /// Shorthands are only valid in block-level attributes, not inside macro brackets.
        ///
        /// Asciidoctor behavior:
        /// - `image::photo.jpg[.role]` -> alt=".role" (literal text, NOT a role)
        /// - `image::photo.jpg[Diablo 4 picture of Lilith.]` -> alt="Diablo 4 picture of Lilith."
        rule macro_attributes() -> ParsedAttributeList<'input>
            = macro_attributes_for()

        rule cached_macro_attributes(block_metadata: &BlockParsingMetadata<'input>) -> ParsedAttributeList<'input>
            = attributes:raw_attribute_list(BlockAttributeMode::Macro) {?
                if let Some(metadata) = &block_metadata.media_attributes {
                    Ok(ParsedAttributeList::from_metadata((**metadata).clone()))
                } else {
                    parse_block_attribute_list(state, attributes.source, attributes.start, BlockAttributeMode::Macro, 0)
                        .map_err(|_| "cannot map block macro attribute locations")
                }
            }

        rule macro_attributes_for() -> ParsedAttributeList<'input>
            = open_square_bracket()
              content_start:position!()
              content:attribute_list_content(BlockAttributeMode::Macro)
            {?
                parse_block_attribute_list(
                    state,
                    content,
                    content_start,
                    BlockAttributeMode::Macro,
                    0,
                ).map_err(|_| "cannot map block macro attribute locations")
            }

        rule open_square_bracket() = "["
        rule close_square_bracket() = "]"
        rule attribute_list_content(context: BlockAttributeMode) -> &'input str
            = content:$((!last_close_square_bracket() [^'\n' | '\r'])*) close_square_bracket() {?
                block_attribute_list_allowed(content, &state.document_attributes, state.options.compatibility_mode, context)
                    .then_some(content)
                    .ok_or("attribute list without outer whitespace")
            }
        rule last_close_square_bracket()
            = &("]" [^']' | '\n' | '\r']* (eol() / ![_]))
        rule double_open_square_bracket() = "[["
        rule double_close_square_bracket() = "]]"
        rule comma() = ","
        rule period() = "."
        /// URL rule matches both web URLs (proto://) and mailto: URLs
        rule url() -> String =
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

        /// URL target content following `://`.
        /// Supports query parameters, fragments, and percent escapes while excluding
        /// brackets that delimit the macro attributes.
        /// Spaces must be internal to the target.
        rule url_path() -> String = path:$(url_path_match())
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.source_view(),
                state.arena,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|_| {
                tracing::error!("could not preprocess url path");
                "could not preprocess url path"
            })?;
            let result = restore_url_path(processed);
            let warnings = inline_state.drain_warnings();
            drop(inline_state);
            for warning in warnings {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(result)
        }

        rule url_path_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '.' | '_' | '~' | ':' | '/' | '?' | '#' | '@' | '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' | '%' | '\\' ]
        rule internal_url_path_spaces() = [' ']+ &url_path_char()
        rule url_path_match() = url_path_char() (url_path_char() / internal_url_path_spaces())*

        /// URL for bare autolinks — avoids capturing trailing sentence punctuation
        /// (., ;, !, etc.) by only consuming punctuation when more URL chars follow.
        rule bare_url() -> String =
        proto:$("https" / "http" / "ftp" / "irc") "://" path:bare_url_path()
        { format!("{proto}://{path}") }

        /// URL path for bare autolinks. Like url_path() but:
        /// - Trailing punctuation (. , ; ! ? : ' *) only consumed when followed by more URL chars.
        /// - `)` only consumed as part of a balanced `(...)` group, preventing capture of
        ///   sentence-level parens like `(see http://example.com)`.
        rule bare_url_path() -> String = path:$(
            bare_url_safe_char()
            ( bare_url_safe_char()
            / bare_url_paren_group()
            / "("
            / bare_url_trailing_char() &bare_url_char()
            )*
        )
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.source_view(),
                state.arena,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
                .map_err(|_| {
                    tracing::error!("could not preprocess bare url path");
                    "could not preprocess bare url path"
                })?;
            let result = restore_url_path(processed);
            let warnings = inline_state.drain_warnings();
            drop(inline_state);
            for warning in warnings {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(result)
        }

        /// Balanced parenthesized group in a URL path.
        /// Handles nested parens: `http://example.com/wiki/Foo_(bar_(baz))`
        /// Only `)` consumed via this rule — unbalanced `)` is never captured.
        rule bare_url_paren_group()
        = "(" (bare_url_safe_char() / bare_url_trailing_char() / bare_url_paren_group() / "(")* ")"

        /// URL chars that are safe to end a bare URL — won't be confused with sentence punctuation.
        /// Excludes `(` and `)` which are handled separately via `bare_url_paren_group`.
        rule bare_url_safe_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '~'
            | '/' | '#' | '@' | '$' | '&'
            | '+' | '=' | '%' | '\\']

        /// URL chars that are valid mid-URL but should not end a bare URL.
        /// Excludes `)` which is only consumed via balanced `bare_url_paren_group`.
        rule bare_url_trailing_char() = ['.' | ',' | ';' | '!' | '?' | ':' | '\'' | '*']

        /// Any valid URL path char (for lookahead in trailing char rule).
        /// Includes `(` because it can start a paren group.
        /// Excludes `)` so that trailing chars before `)` aren't greedily consumed
        /// (e.g., `http://example.com.)` keeps both `.` and `)` outside).
        rule bare_url_char() = bare_url_safe_char() / bare_url_trailing_char() / "("

        /// Fragment identifier for URLs and cross-references (e.g., `#section-id`)
        /// Only used by `xref:` and `link:` macros — other macros (`image::`, `video::`, etc.) do not support fragments
        rule path_fragment() -> String
            = "#" fragment:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+)
        {
            format!("#{fragment}")
        }

        /// Filesystem path accepted by block macros.
        ///
        /// ASCII input uses a conservative filename set. Non-ASCII Unicode characters
        /// are accepted unchanged, and `{`/`}` permit `AsciiDoc` attribute substitution.
        /// Existing percent escapes and internal spaces are preserved.
        rule path() -> String = path:$(path_match())
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.source_view(),
                state.arena,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|_| {
                tracing::error!("could not preprocess path");
                "could not preprocess path"
            })?;
            let result = processed.text.into_owned();
            let warnings = inline_state.drain_warnings();
            drop(inline_state);
            for warning in warnings {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(result)
        }

        rule path_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '{' | '}' | '_' | '-' | '.' | '/' | '\\' | '%' | '\'' | '"' | '\u{80}'..='\u{10FFFF}' ]
        rule internal_path_spaces() = [' ']+ &path_char()
        rule path_match() = path_char() (path_char() / internal_path_spaces())*


        rule source() -> Source<'input>
            = source:
        (
            u:url() {?
                let interned = state.intern_str(&u);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse URL")
            }
            / p:path() {?
                let interned = state.intern_str(&p);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse path")
            }
        )
        { source }

        rule digits() = ['0'..='9']+

        rule whitespace() = quiet!{ " " / "\t" }
        rule eol() = quiet!{ "\n" }

        rule comment_line() = quiet!{ comment() (eol() / ![_]) }
        rule comment() = quiet!{ "//" !"/" [^'\n']* (&eol() / ![_]) }

        // Separator whitespace belongs to the declaration, not its value.
        // Soft wraps arrive folded; preserved hard wraps are collected below.
        rule document_attribute_value() -> Cow<'input, str>
        = whitespace()+ value:(
            lines:backslash_continuation_lines() { Cow::Owned(lines.join("\n")) }
            / single_line:$([^'\n']+) { Cow::Borrowed(single_line) }
        ) { value }

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
        // Works identically in both header and block metadata contexts.
        // As in asciidoctor, the closing `:` must be followed by whitespace or
        // the end of the line, so `:name:text` is ordinary text.
        rule document_attribute_match() -> AttributeDeclaration<'input>
        = ":"
        key_entry:(
            "!" key:$([^':' | '\n']+) { (false, key) }
            / key:$([^('!' | ':' | '\n')]+) "!" { (false, key) }
            / key:$([^':' | '\n']+) { (true, key) }
        )
        ":" &([' ' | '\t' | '\n'] / ![_])
        value:document_attribute_value()?
        {
            let (set, key) = key_entry;
            let (attr_value, ignored_value) = if !set {
                (RawAttributeValue::Unset, value)
            } else if let Some(v) = value {
                (RawAttributeValue::Text(v), None)
            } else {
                (RawAttributeValue::Set, None)
            };
            AttributeDeclaration { name: key, value: attr_value, ignored_value }
        }
        / expected!("document attribute key starting with ':'")

        rule position() -> PositionWithOffset = offset:position!() {
            PositionWithOffset {
                offset,
                position: state.source_position(offset)
            }
        }

    }
}
