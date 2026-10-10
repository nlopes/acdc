// The `peg` macro adds 5 hidden parameters to every rule function, so even
// rules with just 3 explicit params exceed clippy's 7-argument threshold.
#![allow(clippy::too_many_arguments)]

use crate::{
    Admonition, AdmonitionVariant, Anchor, AttributeValue, Attribution, Audio, Author, Block,
    BlockMetadata, CalloutList, CalloutListItem, CalloutRef, CiteTitle, Comment, CommentKind,
    DelimitedBlock, DelimitedBlockType, DescriptionList, DescriptionListItem, DiscreteHeader,
    Document, DocumentAttribute, DocumentAttributes, Error, Header, Image, InlineNode, ListItem,
    ListItemCheckedStatus, Location, OrderedList, PageBreak, Paragraph, Plain, Position, Section,
    Source, Subtitle, TableOfContents, ThematicBreak, Title, UnorderedList, Video, Warning,
    WarningKind,
    document_attribute::{AttributeDeclaration, RawAttributeValue},
    grammar::{
        ParserState,
        document::{
            author::{derive_author_attrs, register_author_attrs},
            callouts::resolve_verbatim_callouts,
            delimited::{DelimitedKind, DelimitedParams, build_delimited_block},
            doctype::{is_book_doctype, is_manpage_doctype},
            lists::{
                assemble_principal_text, build_description_list_topology, calculate_item_end,
                find_dlist_marker,
            },
            manpage::{
                ManpageNameSection, derive_manpage_header_attrs, derive_name_section_attrs,
                prepare_manpage_name_attributes,
            },
            metadata::{
                AttributeOrAnchorLine, BlockAttributeMode, BlockMetadataLine, assign_block_caption,
                extract_media_dimensions, finish_block_parsing_metadata, merge_attribute_metadata,
                metadata_marks_discrete_heading, order_document_attribute_events,
                parse_block_attribute_list, push_metadata_anchor,
            },
            references::{
                ReferenceCatalog, collect_inline_references, collect_metadata_references,
                collect_references, finalize_inline_semantics, header_reference_title,
                insert_reference, metadata_xreflabel, normalize_bibliography_lists,
                register_document_top, register_section_header, resolve_source_target,
                resolve_xref_target,
            },
            revision::{IgnoredRevisionFields, RevisionInfo, process_revision_info},
            sections::{apply_leveloffset, expected_child_level},
            table::{TableClosing, TableParseParams, parse_table_block_impl},
            verbatim::{get_literal_paragraph, resolve_verbatim_inlines, verbatim_substitutions},
        },
        helpers::{
            BlockParsingMetadata, MacroAttributeContext, PositionWithOffset,
            is_valid_bibliography_id, restore_url_path, title_looks_like_description_list,
        },
        inline_preprocessing,
        inline_preprocessor::InlinePreprocessorParserState,
        inline_processing::{adjust_and_log_parse_error, process_inlines},
        setext,
        state::BlockContext,
    },
    model::{
        ListLevel, SectionKind, SectionLevel, Substitution, caption, section, substitution::HEADER,
    },
};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    rc::Rc,
};

peg::parser! {
    pub(crate) grammar document_parser(state: &mut ParserState<'input>) for str {
        use std::str::FromStr;
        use crate::model::substitute;

        // Each action gets the byte range of its preceding sequence.
        // Resolve line and column positions only when a node needs a location.
        inject span_start(_input, l, _r) -> usize { l }
        inject span_end(_input, _l, r) -> usize { r }

        // The document location excludes leading and trailing empty lines.
        pub(crate) rule document() -> Result<Document<'input>, Error>
        = eol()* start:position() comments_before_header:(comment:leading_comment() eol()* { comment })* header_result:header() prepare_manpage_front_matter() header_attributes:header_attribute_snapshot() blocks:blocks(0, None, None) end:position!() (eol()* / ![_]) {
            let header = header_result?;
            let mut blocks: Vec<Block<'_>> = comments_before_header.into_iter().collect::<Result<Vec<_>, Error>>()?.into_iter().chain(blocks?).collect();

            // Ensure end offset is on a valid UTF-8 boundary
            let mut document_end_offset = end;
            if document_end_offset > state.input.len() {
                document_end_offset = state.input.len();
            }
            // If not on a boundary, round forward to the next boundary
            while document_end_offset < state.input.len() && !state.input.is_char_boundary(document_end_offset) {
                document_end_offset += 1;
            }
            let document_end_offset = if document_end_offset == 0 {
                0
            } else {
                crate::grammar::utf8_utils::safe_decrement_offset(state.input, document_end_offset)
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
                (Position::new(1, 0), Position::new(1, 0))
            } else {
                (
                    start.position,
                    state.line_map.offset_to_position(absolute_end, state.input)
                )
            };

            // Warn when a top-level section skips level 1 (e.g. a document that
            // jumps straight to `=== Heading`). Matches asciidoctor's "section
            // title out of sequence" check.
            //
            // The document root sits at level 0 — so every top-level section is
            // expected at level 1 — once it is "anchored" by a document title or
            // by preamble body content (a paragraph, list, ...) before the first
            // section. When anchored, asciidoctor flags *each* top-level section
            // deeper than level 1 (not just the first). A document that opens
            // directly with a section (no title, no preamble) is not anchored:
            // that first section sets the base level and neither it nor its
            // same-or-shallower siblings are out of sequence. Comments are
            // transparent and never anchor. Sections nested under another section
            // are validated separately, in the `section` rule itself.
            //
            // `toc_entries` is populated while parsing and is empty exactly when
            // the document has no sections — checking it first lets section-less
            // documents skip the body scan entirely. Otherwise we walk only the
            // top-level blocks (preamble + sibling sections, never nested content)
            // and stop at the first section in the un-anchored case.
            if !state.toc_entries.is_empty() {
                let mut anchored = header.as_ref().is_some_and(|h| !h.title.is_empty());
                let mut seen_section = false;
                for block in &blocks {
                    if let Block::Section(section) = block {
                        if !anchored {
                            // Un-anchored leading section: it establishes the base
                            // level, so neither it nor its siblings can be out of
                            // sequence. Nothing left to check.
                            break;
                        }
                        if section.level > 1 {
                            let location = state
                                .create_error_source_location(section.location.clone());
                            state.add_warning(Warning::new(
                                WarningKind::SectionLevelOutOfSequence {
                                    expected: 1,
                                    got: section.level,
                                },
                                Some(location),
                            ));
                        }
                        seen_section = true;
                    } else if !seen_section && !matches!(block, Block::Comment(_)) {
                        // Preamble content before the first section anchors the
                        // document at level 0.
                        anchored = true;
                    }
                }
            }

            // Assign caption ordinals over the finished tree. Numbering here cannot be
            // disturbed by PEG backtracking, and it runs before the reference catalog so that
            // catalog can later carry a target's caption label and ordinal.
            normalize_bibliography_lists(state, &mut blocks);
            caption::renumber_captions(&mut blocks);

            let header_has_anchor = header.as_ref().is_some_and(|header| {
                header.metadata.id.is_some() || !header.metadata.anchors.is_empty()
            });
            let mut references = ReferenceCatalog {
                entries: HashMap::with_capacity(state.toc_entries.len() + usize::from(header_has_anchor)),
                next_suffix: HashMap::new(),
                natural_targets: HashMap::new(),
            };
            let mut xrefs = Vec::new();
            if let Some(header) = &header {
                if let Some(anchor) = header
                    .metadata
                    .id
                    .as_ref()
                    .or_else(|| header.metadata.anchors.last())
                {
                    let label = metadata_xreflabel(state, &header.metadata);
                    insert_reference(
                        state,
                        &mut references,
                        anchor,
                        label,
                        Some(header_reference_title(header)),
                        None,
                    );
                }
                collect_inline_references(
                    state,
                    header.title.as_ref(),
                    &mut references,
                    &mut xrefs,
                );
                if let Some(subtitle) = &header.subtitle {
                    collect_inline_references(
                        state,
                        subtitle.as_ref(),
                        &mut references,
                        &mut xrefs,
                    );
                }
                collect_metadata_references(
                    state,
                    &header.metadata,
                    &mut references,
                    &mut xrefs,
                );
            }
            collect_references(state, &mut blocks, &mut references, &mut xrefs);
            let is_book = is_book_doctype(&header_attributes);
            section::number_parsed_sections(
                &mut blocks,
                &mut state.toc_entries,
                &mut references.entries,
                is_book,
            );
            let toc_entries = state.toc_entries.clone();

            let mut document = Document {
                header,
                // Source remapping assigns each boundary its own file after parsing.
                // A document can start in the entry file and end in an included file.
                location: Location {
                    absolute_start,
                    absolute_end,
                    start: start_position,
                    end: end_position,
                },
                attributes: header_attributes,
                blocks,
                footnotes: state.footnote_tracker.borrow().footnotes.clone(),
                toc_entries,
                references: references.entries,
            };
            register_document_top(state, &mut document, &xrefs);
            let reference_ids = document.references.keys().copied().collect::<HashSet<_>>();

            // An internal `<<id>>` whose target is absent from the catalog is an
            // unresolved (broken) reference. Inter-document/external targets
            // (those addressing another resource) are not validated here.
            for xref in xrefs {
                let source_target = resolve_source_target(state, xref.target, xref.source_syntax);
                let target = source_target.unwrap_or_else(|| resolve_xref_target(
                    xref.target,
                    xref.resolve_natural_target,
                    &reference_ids,
                    &references.natural_targets,
                ));
                let target_is_local = source_target.is_some()
                    || xref.source_syntax.target_is_local(target);
                if target_is_local && xref.automatic
                    && let Some(reference) = document.references.get_mut(target)
                    && reference.is_bibliography()
                {
                    reference.automatic_citation = true;
                }
                if target_is_local && !reference_ids.contains(target)
                {
                    let source_location = state.create_error_source_location(xref.location);
                    state.add_warning(Warning::new(
                        WarningKind::UnresolvedReference {
                            target: target.to_string(),
                        },
                        Some(source_location),
                    ));
                }
            }
            finalize_inline_semantics(
                state,
                &mut document,
                &reference_ids,
                &references.natural_targets,
            );
            Ok(document)
        }

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
        / title:$([^'\n']+) eol()
          level:setext_section_level(title.trim().chars().count(), None) {?
            (level == 1 && !title_looks_like_description_list(title))
                .then_some(title.trim())
                .ok_or("not a level-one Setext manpage name section")
        }

        rule manpage_name_metadata() -> Option<AttributeDeclaration<'input>>
        = manpage_comment_block() { None }
        / "//" !"/" [^'\n']* (eol() / ![_]) { None }
        / "[[" (!"]]" [^'\n'])+ "]]" (eol() / ![_]) { None }
        / !empty_list_separator() !double_open_square_bracket()
          open_square_bracket() attribute_list_content() (eol() / ![_]) { None }
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

        // A blank line ends the header. Blank lines inside an atomic comment
        // block do not end the surrounding header.
        pub(crate) rule header() -> Result<Option<Header<'input>>, Error>
            = start:position!()
            ((document_attribute() / header_comment()) (eol() / ![_]))*
            metadata:header_metadata()
            title_authors:(title_authors:title_authors() { title_authors })?
            (eol() (document_attribute() / header_comment()))*
            end:position!()
            (eol()*<,2> / ![_])
        {
            if let Some((title, subtitle, authors)) = title_authors {
                let mut location = state.create_location(start, end);
                location.absolute_end = crate::grammar::utf8_utils::safe_decrement_offset(state.input, location.absolute_end);
                location.end.column = location.end.column.saturating_sub(1);
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
                );

                // Derive manpage attributes from header if doctype=manpage
                // This must happen during parsing so {mantitle} etc. work in body
                if is_manpage_doctype(&state.document_attributes) {
                    derive_manpage_header_attrs(
                        Some(&header),
                        Rc::make_mut(&mut state.document_attributes),
                        state.options.strict,
                        state.current_file.as_deref().map(std::path::PathBuf::as_path),
                    )?;
                }

                Ok(Some(header))
            } else {
                tracing::debug!("No title or authors found in the document header.");
                Ok(None)
            }
        }

        /// Parse block metadata lines (anchors and attributes) that can appear before a document title.
        /// Only consumes metadata if followed by a document title to avoid stealing attributes
        /// meant for the first block when there's no document title.
        rule header_metadata() -> BlockMetadata<'input>
            = lines:(
                anchor:anchor() { AttributeOrAnchorLine::Anchor(anchor) }
                / attr:attributes_line() { AttributeOrAnchorLine::Attributes((attr.0, Box::new(attr.1))) }
            )+ &document_title()
            {
                let mut metadata = BlockMetadata::default();

                for line in lines {
                    match line {
                        AttributeOrAnchorLine::Anchor(anchor) => push_metadata_anchor(&mut metadata, anchor),
                        AttributeOrAnchorLine::Attributes((_, attr_metadata)) => {
                            let attr_metadata = *attr_metadata;
                            // Merge attribute metadata - last one wins for id/style
                            if attr_metadata.id.is_some() {
                                metadata.id = attr_metadata.id;
                            }
                            if attr_metadata.style.is_some() {
                                metadata.style = attr_metadata.style;
                            }
                            metadata.roles.extend(attr_metadata.roles);
                            metadata.options.extend(attr_metadata.options);
                            for (name, value) in attr_metadata.attributes.iter() {
                                metadata.attributes.set(name.clone(), value.clone());
                            }
                            metadata.positional_attributes = attr_metadata.positional_attributes;
                        }
                    }
                }
                metadata
            }
            / { BlockMetadata::default() }

        pub(crate) rule title_authors() -> (Title<'input>, Option<Subtitle<'input>>, Vec<Author<'input>>)
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

        pub(crate) rule document_title() -> (Title<'input>, Option<Subtitle<'input>>)
        = document_title_atx()
        / document_title_setext()

        /// ATX-style document title: `= Title` or `# Title`
        rule document_title_atx() -> (Title<'input>, Option<Subtitle<'input>>)
        = &atx_heading_prefix() document_title_token() whitespace() start:position!() title:$([^'\n']*) end:position!()
        {?
            tracing::debug!("Processing ATX document title");
            let block_metadata = BlockParsingMetadata::default();

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
                        position: state.line_map.offset_to_position(sub_start_offset, state.input),
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
        rule document_title_setext() -> (Title<'input>, Option<Subtitle<'input>>)
        = title:$([^'\n']+) end:position!() eol()
          underline:$("="+) &(eol() / ![_])
        {?
            if !setext::is_enabled(state) {
                return Err("setext mode not enabled");
            }

            let title_text = title.trim();
            let title_width = title_text.chars().count();
            let underline_width = underline.chars().count();

            if !setext::width_ok(title_width, underline_width) {
                return Err("underline width out of tolerance");
            }

            if !underline.starts_with('=') {
                return Err("document title must use = underline");
            }

            tracing::debug!("Processing setext document title");
            let block_metadata = BlockParsingMetadata::default();

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
                        position: state.line_map.offset_to_position(sub_start_offset, state.input),
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
                let substituted_cow = substitute(author_line.trim(), HEADER, &state.document_attributes);
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

        pub(crate) rule authors() -> Vec<Author<'input>>
            = authors:(author() ++ (";" whitespace()*)) {
                authors
            }

        /// Parse an author in various formats:
        /// - "First Middle Last <email>"
        /// - "First Last <email>"
        /// - "First <email>"
        /// - "First Last"
        pub(crate) rule author() -> Author<'input>
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

        pub(crate) rule revision() -> ()
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
                let substituted_cow = substitute(rev_line.trim(), HEADER, &state.document_attributes);
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

        pub(crate) rule blocks(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>) -> Result<Vec<Block<'input>>, Error>
        = blocks:(!trailing_block_metadata_match() block:block(offset, parent_section_level, direct_parent_section_kind) { block })*
          trailing:trailing_block_metadata(offset)?
        {
            let mut blocks = blocks.into_iter().collect::<Result<Vec<_>, Error>>()?;
            if let Some(trailing) = trailing {
                blocks.extend(trailing?);
            }
            Ok(order_document_attribute_events(blocks))
        }

        pub(crate) rule compound_blocks(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Vec<Block<'input>>, Error>
        = content:blocks(offset, parent_section_level, None) eol()* { content }

        rule trailing_block_metadata_match()
        = eol()* &("[" / ".") (block_metadata_line_match() eol()*)+ ![_]

        // Unused block metadata has no inline effects; document attributes still take effect.
        rule trailing_block_metadata(offset: usize) -> Result<Vec<Block<'input>>, Error>
        = &trailing_block_metadata_match() eol()* events:(
            event:document_attribute_block(offset) (eol() / ![_]) eol()* { Some(event) }
            / (attribute_or_anchor_line_match() / title_line_match()) eol()* { None }
        )+
        {
            events.into_iter().flatten().collect()
        }

        pub(crate) rule nested_document_blocks(offset: usize, initial_attributes: &mut Vec<(crate::AttributeName<'input>, crate::DocumentAttributeAssignment<'input>)>) -> Result<Vec<Block<'input>>, Error>
        = eol()*
          header:(
              comment:comment_line_block(offset) eol()* { comment }
              / attribute:document_attribute_block(offset) (eol() / ![_]) { attribute }
          )*
          header:normalize_nested_header(header, initial_attributes)
          body:blocks(offset, None, None)
        {
            let mut blocks = header?;
            blocks.extend(body?);
            Ok(order_document_attribute_events(blocks))
        }

        rule normalize_nested_header(header: Vec<Result<Block<'input>, Error>>, initial_attributes: &mut Vec<(crate::AttributeName<'input>, crate::DocumentAttributeAssignment<'input>)>) -> Result<Vec<Block<'input>>, Error>
        = { crate::grammar::document::table::normalize_nested_header(state, header, initial_attributes) }

        /// Parse normal cell paragraphs; only blank lines separate them.
        pub(crate) rule blocks_for_table_cell(offset: usize) -> Result<Vec<Block<'input>>, Error>
        = eol()*
        blocks:(
            start:position!()
            content:$((!(eol()*<2,> / eol()* ![_]) [_])+)
            end:position!()
            eol()*
            {
                // Block-looking lines are text in normal cells. Keep inline
                // substitutions without invoking document block recognition.
                let (content, _) = process_inlines(
                    state,
                    &BlockParsingMetadata::default(),
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
        )*
        {
            blocks.into_iter().collect()
        }

        pub(crate) rule block(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>) -> Result<Block<'input>, Error>
        = eol()*
        // First check: if we're at a same-or-higher-level section, fail the entire block
        // This prevents section content from consuming sibling/parent sections as paragraphs
        !same_or_higher_level_section(offset, parent_section_level)
        block:(
            comment_line_block(offset) /
            document_attribute_block(offset) /
            // A discrete heading is introduced by an attribute line (`[discrete]`,
            // `[#id,discrete]`, `[float]`, …) or an anchor preceding one, so only
            // attempt it when the block starts with `[`. The rule itself backtracks
            // to `section`/`block_generic` when the metadata isn't a discrete marker.
            &"[" dh:discrete_header(offset) { dh } /
            section:section(offset, parent_section_level, direct_parent_section_kind) { section } /
            // Try setext-style sections (only enabled with setext feature + runtime flag)
            section_setext:section_setext(offset, parent_section_level, direct_parent_section_kind) { section_setext } /
            block_generic(offset, parent_section_level)
        )
        { block }

        /// Single-line comment that becomes a block in the AST.
        /// Line comments begin with `//` (but not `///` or `////` which are block comment delimiters).
        rule comment_line_block(offset: usize) -> Result<Block<'input>, Error>
        = "//" !("/") content:$([^'\n']*) end:position!() (eol() / ![_])
        {
            // `end` is captured before consuming the trailing newline so the
            // comment's location doesn't include it.
            Ok(Block::Comment(Comment {
                kind: CommentKind::Line,
                content,
                location: state.create_location(span_start + offset, end + offset),
            }))
        }

        rule leading_comment() -> Result<Block<'input>, Error>
        = comment_line_block(0)
        / start:position!() block:comment_block(start, 0, &BlockParsingMetadata::default()) { block }

        // Consume a block comment as a whole so its contents cannot become
        // header attributes, author information or revision information.
        rule header_comment()
        = comment()
        / start:position!() result:comment_block(start, 0, &BlockParsingMetadata::default()) {?
            result.map(|_| ()).map_err(|_| "invalid header comment")
        }

        /// Like `comment_line_block` but leaves the trailing newline unconsumed
        /// (lookahead instead of consume). Used in list continuations so that a
        /// `+` continuation following the comment can still match, since
        /// continuation markers expect a leading newline before the `+`.
        rule comment_line_block_keep_eol(offset: usize) -> Result<Block<'input>, Error>
        = "//" !("/") content:$([^'\n']*) end:position!() &(eol() / ![_])
        {
            Ok(Block::Comment(Comment {
                kind: CommentKind::Line,
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
          (metadata:$((block_metadata_line_match() eol()*)*)
          {? if metadata_marks_discrete_heading(metadata, state) { Err("discrete heading belongs to its parent") } else { Ok(()) } })
          (
            // ATX-style section check - require space after marker to avoid matching
            // description list items like `#term::` as sections
            level:section_level(offset, parent_section_level) &" "
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
        = title:$([^'\n']+) eol() underline:$(['-' | '~' | '^' | '+']+) &(eol() / ![_])
        {?
            if title_looks_like_description_list(title) {
                return Err("title looks like a description list item");
            }
            if !setext::is_enabled(state) {
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
        // title before ending a paragraph or consuming heading metadata.
        rule atx_heading_prefix()
        = ("=" / "#")*<1,6> whitespace()+ !eol() &[_]

        // Recognize heading syntax before metadata can register title macros.
        rule atx_heading_match()
        = (block_metadata_line_match() eol()*)* atx_heading_prefix()

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

            Ok(Block::DiscreteHeader(DiscreteHeader {
                metadata: block_metadata.metadata,
                title,
                level,
                location,
            }))
        }

        pub(crate) rule document_attribute_block(offset: usize) -> Result<Block<'input>, Error>
        = att:document_attribute_match()
        {
            let name = Cow::Borrowed(att.name);
            let location = state.create_block_location(span_start, span_end, offset);
            let event = state
                .apply_document_attribute(&att, false, location.clone())
                .unwrap_or_else(|| DocumentAttribute::rejected(name, location));
            Ok(Block::DocumentAttribute(event))
        }

        pub(crate) rule section(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>) -> Result<Block<'input>, Error>
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
            Ok::<(Title<'input>, section::SectionNumbering, &'input str), Error>(register_section_header(
                state,
                &block_metadata,
                title,
                natural_title,
                section_level.1,
                location,
                direct_parent_section_kind,
            ))
        })
        content:section_content(offset, Some(expected_child_level(
            section_level.1,
            SectionKind::from_style(block_metadata.metadata.style),
            is_book_doctype(&state.document_attributes),
        )), Some(SectionKind::from_style(block_metadata.metadata.style)))?
        {
            let (title, numbering, reference_text) = section_header?;
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
        /// The underline must be within ±2 characters of the title width.
        /// Only enabled when the setext feature is compiled in AND the runtime
        /// option is enabled.
        /// Parse a setext section level from the underline character.
        /// Returns the level (1-4) corresponding to -, ~, ^, +
        rule setext_section_level(title_width: usize, parent_section_level: Option<SectionLevel>) -> u8
        = underline:$(['-' | '~' | '^' | '+']+) &(eol() / ![_])
        {?
            if !setext::is_enabled(state) {
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
        = (block_metadata_line_match() eol()*)*
          title:$([^'\n']+) eol()
          setext_section_level(title.trim().chars().count(), parent_section_level)

        /// Parse a setext-style section (title followed by underline).
        /// Excludes description list items (e.g., `term:: content`) which would otherwise
        /// match as setext titles.
        pub(crate) rule section_setext(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>) -> Result<Block<'input>, Error>
        = check_section_blocks() !check_line_is_description_list(offset)
        &setext_section_match(parent_section_level)
        block_metadata:(bm:heading_metadata(offset, parent_section_level) {?
            bm.map_err(|_| {
                tracing::error!("error parsing block metadata in section_setext");
                "block metadata parse error"
            })
        })
        title_start:position!() title:$([^'\n']+) title_end:position!() eol()
        setext_level:setext_section_level(title.trim().chars().count(), parent_section_level)
        section_header:({
            let (processed_title, natural_title) =
                process_inlines(state, &block_metadata, title_start, title_end, offset, title)?;
            let title = Title::new(processed_title);
            let location = state.create_block_location(title_start, title_end, offset);
            Ok::<(Title<'input>, section::SectionNumbering, &'input str), Error>(register_section_header(
                state,
                &block_metadata,
                title,
                natural_title,
                setext_level,
                location,
                direct_parent_section_kind,
            ))
        })
        content:section_content(offset, Some(expected_child_level(
            setext_level,
            SectionKind::from_style(block_metadata.metadata.style),
            is_book_doctype(&state.document_attributes),
        )), Some(SectionKind::from_style(block_metadata.metadata.style)))?
        {
            let (title, numbering, reference_text) = section_header?;
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
            Ok(Block::Section(section))
        }

        rule block_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<BlockParsingMetadata<'input>, Error>
        = metadata:block_metadata_with_title(offset, parent_section_level, true) { metadata }

        // Headings display their own text, so ignore macros in preceding dot-titles.
        rule heading_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<BlockParsingMetadata<'input>, Error>
        = metadata:block_metadata_with_title(offset, parent_section_level, false) { metadata }

        rule block_metadata_with_title(offset: usize, parent_section_level: Option<SectionLevel>, parse_title: bool) -> Result<BlockParsingMetadata<'input>, Error>
        = meta_start:position!() lines:(line:(
            anchor:anchor() { BlockMetadataLine::Anchor(anchor) }
            / attr:attributes_line() { BlockMetadataLine::Attributes((attr.0, Box::new(attr.1))) }
            / doc_attr:document_attribute_line(offset) { BlockMetadataLine::DocumentAttribute(doc_attr.0, doc_attr.1) }
            / title_line()
        ) end:position!() eol()* { (line, end) })*
        {
            let mut metadata = BlockMetadata::default();
            let mut discrete = false;
            let mut title_source = None;
            let meta_end = lines.last().map_or(meta_start, |(_, end)| *end);

            for (line, _) in lines {
                match line {
                    BlockMetadataLine::Anchor(value) => push_metadata_anchor(&mut metadata, value),
                    BlockMetadataLine::Attributes((attr_discrete, attr_metadata)) => {
                        discrete = attr_discrete;
                        merge_attribute_metadata(&mut metadata, *attr_metadata);
                    },
                    BlockMetadataLine::DocumentAttribute(declaration, location) => {
                        if let Some(event) = state.apply_document_attribute(
                            &declaration,
                            false,
                            location,
                        ) {
                            metadata.push_document_attribute(event);
                        }
                    },
                    BlockMetadataLine::Title { source, start, end } => {
                        title_source = Some((source, start, end));
                    }
                }
            }
            // Titles use the completed metadata attributes, including entries
            // after the title line. Only the last authored title is displayed.
            let title = if let Some((source, start, end)) = title_source.filter(|_| parse_title) {
                match process_inlines(
                    state, &BlockParsingMetadata::default(), start, end, offset, source,
                ) {
                    Ok((title, _)) => title.into(),
                    Err(error) => {
                        state.add_generic_warning(format!("failed to parse block title, skipping: {error:?}"));
                        Title::default()
                    }
                }
            } else {
                Title::default()
            };
            if meta_start != meta_end {
                metadata.location = Some(state.create_block_location(meta_start, meta_end, offset));
            }
            finish_block_parsing_metadata(
                state,
                metadata,
                title,
                parent_section_level,
                discrete,
                offset,
            )
        }

        // Match metadata in lookahead without expanding attributes or titles.
        rule block_metadata_line_match()
        = attribute_or_anchor_line_match()
        / document_attribute_match() (eol() / ![_])
        / title_line_match()

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
                state.input.as_bytes().get(prev_byte_pos).is_some_and(|&b| b == b'\n')
            };

            if !at_line_start {
                return Err("expected line start");
            }

            Ok(())
        }

        rule heading_boundary(offset: usize)
        = at_line_start(offset)
          (check_section_blocks() (attribute_or_anchor_line_match() eol()*)*
          / (attribute_or_anchor_line_match() eol()*)+)
          at_line_start(offset) atx_heading_prefix()

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

        rule section_content(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>) -> Result<Vec<Block<'input>>, Error>
        = blocks(offset, parent_section_level, direct_parent_section_kind) / { Ok(vec![]) }

        pub(crate) rule block_generic(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block<'input>, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|_| {
                tracing::error!("error parsing block metadata in block_generic");
                "block metadata parse error"
            })
        })
        block:(
            delimited_block:delimited_block(start, offset, &block_metadata) { delimited_block }
            / verbatim:styled_verbatim_paragraph(start, offset, &block_metadata) { verbatim }
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
            let mut block = block?;
            assign_block_caption(state, &mut block);
            Ok(block)
        }

        // Block parsing for continuation context - lists inside continuations cannot consume
        // further continuations (those belong to the parent item that started the continuation)
        rule block_in_continuation(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block<'input>, Error>
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
            ) { c }) { comment }
            / delimited_block:delimited_block(start, offset, &block_metadata) { delimited_block }
            / verbatim:styled_verbatim_paragraph(start, offset, &block_metadata) { verbatim }
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
            let mut block = block?;
            assign_block_caption(state, &mut block);
            Ok(block)
        }

        rule delimited_block(
            start: usize,
            offset: usize,
            block_metadata: &BlockParsingMetadata<'input>,
        ) -> Result<Block<'input>, Error>
        = generic_delimited_block(start, offset, block_metadata)
        / table_block(start, offset, block_metadata)

        // Every non-table delimited block shares one open/content/optional-close
        // skeleton. `block_open` recognises which kind a delimiter introduces and
        // `build_delimited_block` constructs the right block — the same split tables
        // use (`*_table_block` rules + `parse_table_block_impl`). The optional close
        // and the `(eol() / ![_])` after the open delimiter let an opener that runs
        // to end of input still produce a block, closed at EOF (asciidoctor's
        // recovery; `build_delimited_block` emits the unterminated warning).
        rule generic_delimited_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
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
            let kind = match (open.0, block_metadata.metadata.style) {
                (DelimitedKind::Open, Some("source" | "listing")) => DelimitedKind::Listing,
                (DelimitedKind::Open, Some("literal")) => DelimitedKind::Literal,
                (kind, _) => kind,
            };
            build_delimited_block(state, block_metadata, &DelimitedParams {
                kind, open_delim: open.1, lang: open.2, content, source_text,
                open_start, start, content_start, content_end, end: span_end, offset, close,
            })
        }

        // Recognise a non-table delimited-block opening delimiter, returning its
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
            / d:pass_delimiter()     { (DelimitedKind::Pass, d, None) }
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
            build_delimited_block(state, block_metadata, &DelimitedParams {
                kind: DelimitedKind::Comment, open_delim, lang: None, content, source_text,
                open_start, start, content_start, content_end, end: span_end, offset, close,
            })
        }

        // Delimiter recognition rules
        rule comment_delimiter() -> &'input str = delim:$("/"*<4,>) { delim }
        rule example_delimiter() -> &'input str = delim:$("="*<4,>) { delim }
        rule listing_delimiter() -> &'input str = delim:$("-"*<4,>) { delim }
        rule literal_delimiter() -> &'input str = delim:$("."*<4,>) { delim }
        rule open_delimiter() -> &'input str = delim:$("-"*<2,2> / "~"*<4,>) { delim }
        rule sidebar_delimiter() -> &'input str = delim:$("*"*<4,>) { delim }
        rule table_delimiter() -> &'input str = delim:$((['|' | ',' | ':' | '!'] "="*<3,>)) { delim }

        // Delimiter-specific table delimiter rules for nested table support.
        // PEG negative lookahead can't accept runtime parameters, so we need
        // separate rules for each delimiter type to correctly parse nested tables.
        rule pipe_table_delimiter() -> &'input str = delim:$("|" "="*<3,>) { delim }
        rule excl_table_delimiter() -> &'input str = delim:$("!" "="*<3,>) { delim }
        rule comma_table_delimiter() -> &'input str = delim:$("," "="*<3,>) { delim }
        rule colon_table_delimiter() -> &'input str = delim:$(":" "="*<3,>) { delim }

        rule pass_delimiter() -> &'input str = delim:$("+"*<4,>) { delim }
        rule markdown_code_delimiter() -> &'input str = delim:$("`"*<3,>) { delim }
        rule quote_delimiter() -> &'input str = delim:$("_"*<4,>) { delim }

        rule until_table_delimiter() -> &'input str
        = content:$((!(eol() table_delimiter()) [_])*) { content }

        // Delimiter-specific content rules for nested table support.
        // Each rule only looks ahead for its specific delimiter, allowing
        // nested tables with different delimiters to be parsed correctly.
        rule until_pipe_table_delimiter() -> &'input str
        = content:$((!(eol() pipe_table_delimiter()) [_])*) { content }

        rule until_excl_table_delimiter() -> &'input str
        = content:$((!(eol() excl_table_delimiter()) [_])*) { content }

        rule until_comma_table_delimiter() -> &'input str
        = content:$((!(eol() comma_table_delimiter()) [_])*) { content }

        rule until_colon_table_delimiter() -> &'input str
        = content:$((!(eol() colon_table_delimiter()) [_])*) { content }

        rule markdown_language() -> &'input str
        = lang:$((['a'..='z'] / ['A'..='Z'] / ['0'..='9'] / "_" / "+" / "-")+) { lang }

        // Table block dispatcher - tries each delimiter-specific variant in order.
        // This enables nested tables: |=== outer can contain !=== inner because
        // each rule only looks for its own closing delimiter.
        //
        // Terminated variants are tried first; unterminated fallbacks only match
        // when an opening delimiter runs to end-of-input without a close.
        rule table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = pipe_table_block(start, offset, block_metadata)
            / excl_table_block(start, offset, block_metadata)
            / comma_table_block(start, offset, block_metadata)
            / colon_table_block(start, offset, block_metadata)
            / unterminated_pipe_table_block(start, offset, block_metadata)
            / unterminated_excl_table_block(start, offset, block_metadata)
            / unterminated_comma_table_block(start, offset, block_metadata)
            / unterminated_colon_table_block(start, offset, block_metadata)

        rule pipe_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:pipe_table_delimiter() eol()
              content_start:position!() content:until_pipe_table_delimiter() content_end:position!()
              eol() close_start:position!() close_delim:pipe_table_delimiter()
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: "|",
                    closing: TableClosing::Terminated { close_delim, close_start },
                },
                state,
                block_metadata,
            )
        }

        rule excl_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:excl_table_delimiter() eol()
              content_start:position!() content:until_excl_table_delimiter() content_end:position!()
              eol() close_start:position!() close_delim:excl_table_delimiter()
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: "!",
                    closing: TableClosing::Terminated { close_delim, close_start },
                },
                state,
                block_metadata,
            )
        }

        rule comma_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:comma_table_delimiter() eol()
              content_start:position!() content:until_comma_table_delimiter() content_end:position!()
              eol() close_start:position!() close_delim:comma_table_delimiter()
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: ",",
                    closing: TableClosing::Terminated { close_delim, close_start },
                },
                state,
                block_metadata,
            )
        }

        rule colon_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:colon_table_delimiter() eol()
              content_start:position!() content:until_colon_table_delimiter() content_end:position!()
              eol() close_start:position!() close_delim:colon_table_delimiter()
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: ":",
                    closing: TableClosing::Terminated { close_delim, close_start },
                },
                state,
                block_metadata,
            )
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
        rule unterminated_pipe_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:pipe_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_pipe_table_delimiter() content_end:position!()
              ![_]
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: "|",
                    closing: TableClosing::Unterminated,
                },
                state,
                block_metadata,
            )
        }

        rule unterminated_excl_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:excl_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_excl_table_delimiter() content_end:position!()
              ![_]
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: "!",
                    closing: TableClosing::Unterminated,
                },
                state,
                block_metadata,
            )
        }

        rule unterminated_comma_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:comma_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_comma_table_delimiter() content_end:position!()
              ![_]
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: ",",
                    closing: TableClosing::Unterminated,
                },
                state,
                block_metadata,
            )
        }

        rule unterminated_colon_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:colon_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_colon_table_delimiter() content_end:position!()
              ![_]
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: ":",
                    closing: TableClosing::Unterminated,
                },
                state,
                block_metadata,
            )
        }

        rule toc(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = "toc::" attributes:attributes() end:position!()
          trailing:$([^'\n']*)
        {
            let (_discrete, metadata_from_attributes, _title_position) = attributes;
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            metadata.move_positional_attributes_to_attributes();
            state.warn_trailing_macro_content("toc", trailing, end, offset);
            tracing::debug!("Found Table of Contents block");
            Ok(Block::TableOfContents(TableOfContents {
                metadata,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        rule normal_paragraph_style(block_metadata: &BlockParsingMetadata<'input>)
        = {?
            if matches!(
                block_metadata.metadata.style,
                Some("listing" | "source" | "literal" | "verse")
            ) {
                Err("verbatim paragraph style")
            } else {
                Ok(())
            }
        }

        rule image(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = normal_paragraph_style(block_metadata)
          "image::" source:source() attributes:image_macro_attributes() end:position!()
          trailing:$([^'\n']*)
        {
            state.warn_trailing_macro_content("image", trailing, end, offset);
            let (_discrete, mut metadata_from_attributes, _title_position) = attributes;
            extract_media_dimensions(&mut metadata_from_attributes);
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            if let Some(style) = metadata.style {
                metadata.style = None; // Clear style to avoid confusion
                metadata
                    .attributes
                    .set("alt".into(), AttributeValue::String(Cow::Borrowed(style)));
            }
            let _ = metadata.take_positional_attributes::<2>();
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Image(Image {
                title,
                source,
                metadata,
                location: state.create_block_location(start, end, offset),

            }))
        }

        rule audio(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = "audio::" source:source() attributes:macro_attributes() end:position!()
          trailing:$([^'\n']*)
        {
            state.warn_trailing_macro_content("audio", trailing, end, offset);
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
        rule video(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = "video::" sources:(source() ** comma()) attributes:macro_attributes() end:position!()
          trailing:$([^'\n']*)
        {
            state.warn_trailing_macro_content("video", trailing, end, offset);
            let (_discrete, mut metadata_from_attributes, _title_position) = attributes;
            extract_media_dimensions(&mut metadata_from_attributes);
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            if let Some(style) = metadata.style {
                metadata.style = None;
                if style == "youtube" || style == "vimeo" {
                    tracing::debug!("transforming video metadata style into attribute");
                    metadata
                        .attributes
                        .set(Cow::Borrowed(style), AttributeValue::Bool(true));
                } else {
                    // assume poster
                    tracing::debug!("transforming video metadata style into attribute, assuming poster");
                    metadata.attributes.set(
                        "poster".into(),
                        AttributeValue::String(Cow::Borrowed(style)),
                    );
                }
            }
            let _ = metadata.take_positional_attributes::<2>();
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Video(Video {
                title,
                sources,
                metadata,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule thematic_break(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = ("'''"
               // Below are the markdown-style thematic breaks
               / "---"
               / "- - -"
               / "***"
               / "* * *"
            )
        {
            tracing::debug!("Found thematic break block");
            Ok(Block::ThematicBreak(ThematicBreak {
                anchors: block_metadata.metadata.anchors.clone(),
                title: block_metadata.title.clone(),
                location: state.create_block_location(start, span_end, offset),
            }))
        }

        rule page_break(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = "<<<" &eol()*<2,2>
        {
            tracing::debug!("Found page break block");
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();

            Ok(Block::PageBreak(PageBreak {
                title: block_metadata.title.clone(),
                metadata,
                location: state.create_location(start+offset, span_end+offset),
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
        = callout_list(start, offset, block_metadata)
        / unordered_list(start, offset, block_metadata, None, allow_continuation, false)
        / ordered_list(start, offset, block_metadata, None, allow_continuation, false)
        / description_list(start, offset, block_metadata, allow_continuation)

        rule unordered_list_marker() -> &'input str = $("*"+ / "-")

        rule ordered_list_marker() -> &'input str = $(digits()? "."+)

        rule description_list_marker() -> &'input str = $("::::" / ":::" / "::" / ";;")

        rule callout_list_marker() -> &'input str = $("<" (digits() / ".") ">")

        rule section_level_marker() -> &'input str = $(("=" / "#")+)

        // This restricted form excludes titles and document attributes, which
        // asciidoctor does not assign to an automatically nested list.
        rule nested_list_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<BlockParsingMetadata<'input>, Error>
        = meta_start:position!() lines:(line:(
            anchor:anchor() { AttributeOrAnchorLine::Anchor(anchor) }
            / attr:attributes_line() { AttributeOrAnchorLine::Attributes((attr.0, Box::new(attr.1))) }
        ) end:position!() eol()* { (line, end) })+
        {
            let mut metadata = BlockMetadata::default();
            let mut discrete = false;
            let meta_end = lines.last().map_or(meta_start, |(_, end)| *end);
            for (line, _) in lines {
                match line {
                    AttributeOrAnchorLine::Anchor(anchor) => push_metadata_anchor(&mut metadata, anchor),
                    AttributeOrAnchorLine::Attributes((attr_discrete, attr_metadata)) => {
                        discrete = attr_discrete;
                        merge_attribute_metadata(&mut metadata, *attr_metadata);
                    }
                }
            }
            metadata.location = Some(state.create_block_location(meta_start, meta_end, offset));
            finish_block_parsing_metadata(
                state,
                metadata,
                Title::default(),
                parent_section_level,
                discrete,
                offset,
            )
        }

        rule parsed_nested_list_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> BlockParsingMetadata<'input>
        = metadata:nested_list_metadata(offset, parent_section_level) {?
            metadata.map_err(|_| {
                tracing::error!("error parsing nested list metadata");
                "nested list metadata parse error"
            })
        }

        rule attributes_line_match()
        = !empty_list_separator() !double_open_square_bracket()
          open_square_bracket() attribute_list_content() (eol() / ![_])

        rule anchor_line_match()
        = double_open_square_bracket() [^'\'' | ',' | ']' | ' ' | '\t' | '\n' | '\r']+
          (comma() [^']']+)? double_close_square_bracket() (eol() / ![_])

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
        )

        rule nested_ordered_child_after_metadata(offset: usize, current_marker: &str, parent_unordered_marker: Option<&'input str>)
        = (attribute_or_anchor_line_match() eol()*)+ nested_list_metadata_gap()* (
            !at_ancestor_unordered_marker(parent_unordered_marker)
            &(whitespace()* unordered_list_marker() whitespace())
            / &at_deeper_ordered_marker(current_marker)
            / !at_callout_parent_item(offset) at_callout_list_item()
        )

        // Helper rule to check if we're at the start of a new list item (lookahead)
        rule at_list_item_start() = whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace()

        // Helper rule to check if we're at the start of a section heading (lookahead)
        // This is used to terminate list continuations when a section follows
        rule at_section_start() = (attribute_or_anchor_line_match() eol()*)* atx_heading_prefix()

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
        = eol()
          !(
              &eol()
              / &at_list_item_start()
              / &"+"
              / &at_section_start()
              / &at_list_separator_content()
              / &nested_unordered_child_after_metadata(offset, current_marker, parent_ordered_marker)
              / at_callout_list_item()
          )
          line:$((!eol() [_])*) { line }

        rule ordered_list_principal_continuation(offset: usize, current_marker: &str, parent_unordered_marker: Option<&'input str>) -> &'input str
        = eol()
          !(
              &eol()
              / &at_list_item_start()
              / &"+"
              / &at_section_start()
              / &at_list_separator_content()
              / &nested_ordered_child_after_metadata(offset, current_marker, parent_unordered_marker)
              / at_callout_list_item()
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

        rule unordered_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>, allow_continuation: bool, is_nested: bool) -> Result<Block<'input>, Error>
        // Parse whitespace + marker first to capture base_marker for rest items
        // marker_start captures position before marker for correct first item location
        = whitespace()* marker_start:position!() base_marker:$(unordered_list_marker()) &whitespace()
        first:unordered_list_item_after_marker(offset, block_metadata, allow_continuation, base_marker, marker_start, parent_ordered_marker)
        rest:(unordered_list_rest_item(offset, block_metadata, parent_ordered_marker, allow_continuation, base_marker))*
        {
            tracing::debug!("Found unordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or("", |item| item.marker);

            Ok(Block::UnorderedList(UnorderedList {
                title: if is_nested { Title::default() } else { block_metadata.title.clone() },
                metadata: if is_nested { BlockMetadata::default() } else { block_metadata.metadata.clone() },
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
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

        rule unordered_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>, allow_continuation: bool, base_marker: &str) -> Result<(ListItem<'input>, usize), Error>
        // `parent_ordered_marker` is fixed for the whole `unordered_list` call, so
        // rather than parse the (expensive) item first and reject via a trailing
        // `{? }` action on three of four alternatives, guard each alternative at
        // the front with a zero-cost check and only parse when the branch applies.
        // The `!at_ordered_marker_ahead()` lookahead is kept only in the
        // `parent_ordered_marker.is_some()` branch where it actually pays off.
        // See fixtures: nested_unordered_in_ordered.adoc, nested_ordered_in_unordered.adoc
        //
        // Branch: parent is ordered
        = parent_is_some(parent_ordered_marker) !at_list_separator() !eol() comment_line()* !at_ordered_marker_ahead() item:unordered_list_item(offset, block_metadata, allow_continuation, parent_ordered_marker)
          { item }
        / parent_is_some(parent_ordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_unordered_marker(base_marker) !at_ordered_marker_ahead() item:unordered_list_item(offset, block_metadata, allow_continuation, parent_ordered_marker)
          { item }
        // Branch: no ordered parent
        / parent_is_none(parent_ordered_marker) !at_list_separator() !eol() comment_line()* item:unordered_list_item(offset, block_metadata, allow_continuation, parent_ordered_marker)
          { item }
        / parent_is_none(parent_ordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_unordered_marker(base_marker) item:unordered_list_item(offset, block_metadata, allow_continuation, parent_ordered_marker)
          { item }

        rule ordered_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>, allow_continuation: bool, is_nested: bool) -> Result<Block<'input>, Error>
        // Parse whitespace + marker first to capture base_marker for rest items
        // marker_start captures position before marker for correct first item location
        = whitespace()* marker_start:position!() base_marker:$(ordered_list_marker()) &whitespace()
        first:ordered_list_item_after_marker(offset, block_metadata, allow_continuation, base_marker, marker_start, parent_unordered_marker)
        rest:(ordered_list_rest_item(offset, block_metadata, parent_unordered_marker, allow_continuation, base_marker))*
        {
            tracing::debug!("Found ordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or("", |item| item.marker);

            Ok(Block::OrderedList(OrderedList {
                title: if is_nested { Title::default() } else { block_metadata.title.clone() },
                metadata: if is_nested { BlockMetadata::default() } else { block_metadata.metadata.clone() },
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        // Parse first item content after marker has been consumed by ordered_list
        // marker_start is the position where the marker began, for correct location tracking
        rule ordered_list_item_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool, marker: &'input str, marker_start: usize, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = list_continuation_allowed(allow_continuation)
          item:ordered_list_item_with_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_unordered_marker) { item }
        / item:ordered_list_item_no_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_unordered_marker) { item }

        rule ordered_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>, allow_continuation: bool, base_marker: &str) -> Result<(ListItem<'input>, usize), Error>
        // Mirror of `unordered_list_rest_item`'s front-guard structure. See that
        // rule's comment for the rationale.
        //
        // Branch: parent is unordered
        = parent_is_some(parent_unordered_marker) !at_list_separator() !eol() comment_line()* !at_unordered_marker_ahead() item:ordered_list_item(offset, block_metadata, allow_continuation, parent_unordered_marker)
          { item }
        / parent_is_some(parent_unordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_ordered_marker(base_marker) !at_unordered_marker_ahead() item:ordered_list_item(offset, block_metadata, allow_continuation, parent_unordered_marker)
          { item }
        // Branch: no unordered parent
        / parent_is_none(parent_unordered_marker) !at_list_separator() !eol() comment_line()* item:ordered_list_item(offset, block_metadata, allow_continuation, parent_unordered_marker)
          { item }
        / parent_is_none(parent_unordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_ordered_marker(base_marker) item:ordered_list_item(offset, block_metadata, allow_continuation, parent_unordered_marker)
          { item }

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
        = whitespace()*
        marker:unordered_list_marker()
        whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        // Parse first line (principal text)
        first_line:$((!(eol()) [_])*)
        // Parse continuation lines that are part of the same paragraph
        // Stop at: blank line, list item start, explicit continuation marker, section heading, or list separator
        continuation_lines:unordered_list_principal_continuation(offset, marker, parent_ordered_marker)*
        first_line_end:position!()
        principal:list_item_principal(offset, block_metadata, first_line_start, first_line_end, first_line, &continuation_lines)
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
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        list_dangling_continuation()?
        {
            tracing::debug!(first_line_len = first_line.len(), continuation_count = continuation_lines.len(), "found unordered list item");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let item_end = calculate_item_end(first_line.is_empty() && continuation_lines.is_empty(), span_start, first_line_end);
            let principal = principal?;

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(span_start+offset, actual_end+offset),
            }, actual_end))
        }

        // Version with immediate continuations only (for nested items)
        // Nested items consume continuations with 0 empty lines (immediate attachment).
        // Continuations with 1+ empty lines bubble up to ancestor items.
        rule unordered_list_item_no_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()*
        marker:unordered_list_marker()
        whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:unordered_list_principal_continuation(offset, marker, parent_ordered_marker)*
        first_line_end:position!()
        principal:list_item_principal(offset, block_metadata, first_line_start, first_line_end, first_line, &continuation_lines)
        // Nested items can still have nested lists, but those also cannot consume parent continuations
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for immediate_continuation
        nested:(!at_list_separator() nested_content:unordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        // Parse immediate continuations (0 empty lines) - these attach to this item
        // Ancestor continuations (1+ empty lines) bubble up to parent items
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        {
            tracing::debug!(first_line_len = first_line.len(), continuation_count = continuation_lines.len(), "found unordered list item (immediate continuation only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let item_end = calculate_item_end(first_line.is_empty() && continuation_lines.is_empty(), span_start, first_line_end);
            let principal = principal?;

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(span_start+offset, actual_end+offset),
            }, actual_end))
        }

        // After-marker variants: used when marker has already been consumed by parent rule
        // These are identical to the regular variants except they take marker as a parameter
        // instead of parsing it, and start after the marker position
        rule unordered_list_item_with_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:unordered_list_principal_continuation(offset, marker, parent_ordered_marker)*
        first_line_end:position!()
        principal:list_item_principal(offset, block_metadata, first_line_start, first_line_end, first_line, &continuation_lines)
        nested:(!at_list_separator() nested_content:unordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        list_dangling_continuation()?
        {
            tracing::debug!(first_line_len = first_line.len(), continuation_count = continuation_lines.len(), "found unordered list item (after marker)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let item_end = calculate_item_end(first_line.is_empty() && continuation_lines.is_empty(), span_start, first_line_end);
            let principal = principal?;

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        rule unordered_list_item_no_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:unordered_list_principal_continuation(offset, marker, parent_ordered_marker)*
        first_line_end:position!()
        principal:list_item_principal(offset, block_metadata, first_line_start, first_line_end, first_line, &continuation_lines)
        nested:(!at_list_separator() nested_content:unordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        {
            tracing::debug!(first_line_len = first_line.len(), continuation_count = continuation_lines.len(), "found unordered list item (after marker, immediate only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let item_end = calculate_item_end(first_line.is_empty() && continuation_lines.is_empty(), span_start, first_line_end);
            let principal = principal?;

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        /// Parse nested content within an unordered list item (e.g., nested ordered or unordered list)
        /// Note: allow_continuation is false to prevent nested items from consuming parent-level continuations
        /// current_marker: the marker of the parent unordered list item (e.g., "*" or "**")
        /// parent_ordered_marker: the marker of an ancestor ordered list (if any), to prevent
        /// consuming sibling ordered markers that belong to a parent ordered list context
        rule unordered_list_item_nested_after_principal(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_ordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        = eol() nested:(
            unordered_list_item_nested_content_with_metadata(offset, block_metadata, current_marker, parent_ordered_marker)
            / unordered_list_item_nested_content(offset, block_metadata, current_marker, parent_ordered_marker)
          ) { nested }
        / eol()+ !at_callout_list_item() nested:unordered_list_item_nested_content(offset, block_metadata, current_marker, parent_ordered_marker) { nested }

        rule unordered_list_item_nested_content_with_metadata(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_ordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        = nested_start:position!()
          metadata:parsed_nested_list_metadata(offset, block_metadata.parent_section_level)
          nested_list_metadata_gap()*
          list:(
              !at_ancestor_ordered_marker(parent_ordered_marker)
              list:ordered_list(nested_start, offset, &metadata, Some(current_marker), false, false) { list }
              / &at_deeper_unordered_marker(current_marker)
                list:unordered_list_nested(nested_start, offset, &metadata, current_marker, parent_ordered_marker, true) { list }
              / !at_callout_parent_item(offset)
                list:callout_list(nested_start, offset, &metadata) { list }
          )
        {
            Some(list)
        }

        rule unordered_list_item_nested_content(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_ordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        // !at_ancestor_ordered_marker() prevents sibling ordered markers from a parent
        // ordered list context from being consumed by this nested unordered item.
        = !at_ancestor_ordered_marker(parent_ordered_marker) nested_start:position!() list:ordered_list(nested_start, offset, block_metadata, Some(current_marker), false, true) {
            Some(list)
        }
        // Nested unordered list with deeper markers (e.g., ** inside *)
        // Uses unordered_list_nested which only parses items deeper than current_marker
        / &at_deeper_unordered_marker(current_marker)
          nested_start:position!()
          list:unordered_list_nested(nested_start, offset, block_metadata, current_marker, parent_ordered_marker, false)
        {
            Some(list)
        }
        / list:nested_callout_list(offset, block_metadata) { Some(list) }

        /// Parse a nested unordered list where all items have markers deeper than parent_marker.
        /// This is used to parse same-type nesting (e.g., ** inside *) as hierarchical content
        /// rather than flat siblings, enabling proper ancestor continuation handling.
        /// Uses allow_continuation=false to prevent nested items from consuming parent continuations.
        rule unordered_list_nested(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, parent_ordered_marker: Option<&'input str>, has_own_metadata: bool) -> Result<Block<'input>, Error>
        // Parse first item - must have a deeper marker than parent_marker
        = &at_deeper_unordered_marker(parent_marker)
          whitespace()* marker_start:position!() base_marker:$(unordered_list_marker()) &whitespace()
          first:unordered_list_item_after_marker(offset, block_metadata, false, base_marker, marker_start, parent_ordered_marker)
          // Parse rest items - only those at same level as base_marker (not deeper, not shallower than parent)
          rest:(unordered_list_nested_rest_item(offset, block_metadata, parent_marker, base_marker, parent_ordered_marker))*
        {
            tracing::debug!("Found nested unordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or("", |item| item.marker);

            Ok(Block::UnorderedList(UnorderedList {
                title: if has_own_metadata { block_metadata.title.clone() } else { Title::default() },
                metadata: if has_own_metadata { block_metadata.metadata.clone() } else { BlockMetadata::default() },
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        /// Parse rest items in a nested unordered list.
        /// Items must be deeper than parent_marker and at same-or-deeper level as base_marker.
        /// Stops when we encounter a marker at or shallower than parent_marker.
        rule unordered_list_nested_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, base_marker: &str, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        // Case 1: No blank lines - accept same-level or deeper items
        = !at_list_separator() !eol() comment_line()*
          // Must not be at shallower-or-equal to parent (that would end the nested list)
          !at_shallower_or_equal_unordered_marker(parent_marker)
          item:unordered_list_item(offset, block_metadata, false, parent_ordered_marker)
        { item }
        // Case 2: Blank lines present - only accept same-level items (deeper would be its own nesting)
        / !at_list_separator() eol()+ comment_line()*
          // Must not be at shallower-or-equal to parent
          !at_shallower_or_equal_unordered_marker(parent_marker)
          // Must not be deeper than base (that would be nested inside this item)
          !at_deeper_unordered_marker(base_marker)
          item:unordered_list_item(offset, block_metadata, false, parent_ordered_marker)
        { item }

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
        = whitespace()*
        marker:ordered_list_marker()
        whitespace()
        first_line_start:position!()
        // Parse first line (principal text)
        first_line:$((!(eol()) [_])*)
        // Parse continuation lines that are part of the same paragraph
        // Stop at: blank line, list item start, explicit continuation marker, section heading, or list separator
        continuation_lines:ordered_list_principal_continuation(offset, marker, parent_unordered_marker)*
        first_line_end:position!()
        principal:list_item_principal(offset, block_metadata, first_line_start, first_line_end, first_line, &continuation_lines)
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
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        list_dangling_continuation()?
        {
            tracing::debug!(first_line_len = first_line.len(), continuation_count = continuation_lines.len(), "found ordered list item");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let item_end = calculate_item_end(first_line.is_empty() && continuation_lines.is_empty(), span_start, first_line_end);
            let principal = principal?;

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked: None,
                location: state.create_location(span_start+offset, actual_end+offset),
            }, actual_end))
        }

        // Version with immediate continuations only (for nested items)
        // Nested items consume continuations with 0 empty lines (immediate attachment).
        // Continuations with 1+ empty lines bubble up to ancestor items.
        rule ordered_list_item_no_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()*
        marker:ordered_list_marker()
        whitespace()
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:ordered_list_principal_continuation(offset, marker, parent_unordered_marker)*
        first_line_end:position!()
        principal:list_item_principal(offset, block_metadata, first_line_start, first_line_end, first_line, &continuation_lines)
        // Nested items can still have nested lists, but those also cannot consume parent continuations
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for immediate_continuation
        nested:(!at_list_separator() nested_content:ordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        // Parse immediate continuations (0 empty lines) - these attach to this item
        // Ancestor continuations (1+ empty lines) bubble up to parent items
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        {
            tracing::debug!(first_line_len = first_line.len(), continuation_count = continuation_lines.len(), "found ordered list item (immediate continuation only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let item_end = calculate_item_end(first_line.is_empty() && continuation_lines.is_empty(), span_start, first_line_end);
            let principal = principal?;

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked: None,
                location: state.create_location(span_start+offset, actual_end+offset),
            }, actual_end))
        }

        // After-marker variants for ordered lists: used when marker has already been consumed by parent rule
        rule ordered_list_item_with_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:ordered_list_principal_continuation(offset, marker, parent_unordered_marker)*
        first_line_end:position!()
        principal:list_item_principal(offset, block_metadata, first_line_start, first_line_end, first_line, &continuation_lines)
        nested:(!at_list_separator() nested_content:ordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        list_dangling_continuation()?
        {
            tracing::debug!(first_line_len = first_line.len(), continuation_count = continuation_lines.len(), "found ordered list item (after marker)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let item_end = calculate_item_end(first_line.is_empty() && continuation_lines.is_empty(), span_start, first_line_end);
            let principal = principal?;

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked: None,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        rule ordered_list_item_no_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:ordered_list_principal_continuation(offset, marker, parent_unordered_marker)*
        first_line_end:position!()
        principal:list_item_principal(offset, block_metadata, first_line_start, first_line_end, first_line, &continuation_lines)
        nested:(!at_list_separator() nested_content:ordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        {
            tracing::debug!(first_line_len = first_line.len(), continuation_count = continuation_lines.len(), "found ordered list item (after marker, immediate only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let item_end = calculate_item_end(first_line.is_empty() && continuation_lines.is_empty(), span_start, first_line_end);
            let principal = principal?;

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked: None,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        /// Parse nested content within an ordered list item (e.g., nested unordered or ordered list)
        /// Note: allow_continuation is false to prevent nested items from consuming parent-level continuations
        /// current_marker: the marker of the parent ordered list item (e.g., "." or "..")
        /// parent_unordered_marker: the marker of an ancestor unordered list (if any), to prevent
        /// consuming sibling unordered markers that belong to a parent unordered list context
        rule ordered_list_item_nested_after_principal(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_unordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        = eol() nested:(
            ordered_list_item_nested_content_with_metadata(offset, block_metadata, current_marker, parent_unordered_marker)
            / ordered_list_item_nested_content(offset, block_metadata, current_marker, parent_unordered_marker)
          ) { nested }
        / eol()+ !at_callout_list_item() nested:ordered_list_item_nested_content(offset, block_metadata, current_marker, parent_unordered_marker) { nested }

        rule ordered_list_item_nested_content_with_metadata(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_unordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        = nested_start:position!()
          metadata:parsed_nested_list_metadata(offset, block_metadata.parent_section_level)
          nested_list_metadata_gap()*
          list:(
              !at_ancestor_unordered_marker(parent_unordered_marker)
              list:unordered_list(nested_start, offset, &metadata, Some(current_marker), false, false) { list }
              / &at_deeper_ordered_marker(current_marker)
                list:ordered_list_nested(nested_start, offset, &metadata, current_marker, parent_unordered_marker, true) { list }
              / !at_callout_parent_item(offset)
                list:callout_list(nested_start, offset, &metadata) { list }
          )
        {
            Some(list)
        }

        rule ordered_list_item_nested_content(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_unordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        // !at_ancestor_unordered_marker() prevents sibling unordered markers from a parent
        // unordered list context from being consumed by this nested ordered item.
        = !at_ancestor_unordered_marker(parent_unordered_marker) nested_start:position!() list:unordered_list(nested_start, offset, block_metadata, Some(current_marker), false, true) {
            Some(list)
        }
        // Nested ordered list with deeper markers (e.g., .. inside .)
        // Uses ordered_list_nested which only parses items deeper than current_marker
        / &at_deeper_ordered_marker(current_marker)
          nested_start:position!()
          list:ordered_list_nested(nested_start, offset, block_metadata, current_marker, parent_unordered_marker, false)
        {
            Some(list)
        }
        / list:nested_callout_list(offset, block_metadata) { Some(list) }

        /// Parse a nested ordered list where all items have markers deeper than parent_marker.
        /// This is used to parse same-type nesting (e.g., .. inside .) as hierarchical content
        /// rather than flat siblings, enabling proper ancestor continuation handling.
        /// Uses allow_continuation=false to prevent nested items from consuming parent continuations.
        rule ordered_list_nested(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, parent_unordered_marker: Option<&'input str>, has_own_metadata: bool) -> Result<Block<'input>, Error>
        // Parse first item - must have a deeper marker than parent_marker
        = &at_deeper_ordered_marker(parent_marker)
          whitespace()* marker_start:position!() base_marker:$(ordered_list_marker()) &whitespace()
          first:ordered_list_item_after_marker(offset, block_metadata, false, base_marker, marker_start, parent_unordered_marker)
          // Parse rest items - only those at same level as base_marker (not deeper, not shallower than parent)
          rest:(ordered_list_nested_rest_item(offset, block_metadata, parent_marker, base_marker, parent_unordered_marker))*
        {
            tracing::debug!("Found nested ordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or("", |item| item.marker);

            Ok(Block::OrderedList(OrderedList {
                title: if has_own_metadata { block_metadata.title.clone() } else { Title::default() },
                metadata: if has_own_metadata { block_metadata.metadata.clone() } else { BlockMetadata::default() },
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        /// Parse rest items in a nested ordered list.
        /// Items must be deeper than parent_marker and at same-or-deeper level as base_marker.
        /// Stops when we encounter a marker at or shallower than parent_marker.
        rule ordered_list_nested_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, base_marker: &str, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        // Case 1: No blank lines - accept same-level or deeper items
        = !at_list_separator() !eol() comment_line()*
          // Must not be at shallower-or-equal to parent (that would end the nested list)
          !at_shallower_or_equal_ordered_marker(parent_marker)
          item:ordered_list_item(offset, block_metadata, false, parent_unordered_marker)
        { item }
        // Case 2: Blank lines present - only accept same-level items (deeper would be its own nesting)
        / !at_list_separator() eol()+ comment_line()*
          // Must not be at shallower-or-equal to parent
          !at_shallower_or_equal_ordered_marker(parent_marker)
          // Must not be deeper than base (that would be nested inside this item)
          !at_deeper_ordered_marker(base_marker)
          item:ordered_list_item(offset, block_metadata, false, parent_unordered_marker)
        { item }

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
            (state.callout_list_offset == Some(offset)).then_some(()).ok_or("not a callout parent")
        }

        // Save the enclosing scope for callout_list to restore after parsing its
        // items. Keep pending verbatim references local for validation,
        // since child listings add their own references to the shared catalog.
        rule enter_callout_list(offset: usize) -> (Option<usize>, Vec<CalloutRef>)
        = {
            let parent = state.callout_list_offset.replace(offset);
            (parent, std::mem::take(&mut state.pending_callouts))
        }

        rule callout_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        // Match the item prefix and style before changing callout scope.
        = at_callout_list_item() normal_paragraph_style(block_metadata)
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

            let mut auto_number = 0;
            let mut items: Vec<CalloutListItem> = Vec::with_capacity(content.len());

            for (expected_number, (mut item, marker, _end)) in (1..).zip(content) {
                let actual_number = if marker == "<.>" {
                    auto_number += 1;
                    auto_number.to_string()
                } else {
                    marker.trim_start_matches('<').trim_end_matches('>').to_string()
                };
                if actual_number != expected_number.to_string() {
                    state.add_generic_warning_at(
                        format!(
                            "callout list item index: expected {expected_number}, got {actual_number}"
                        ),
                        item.location.clone(),
                    );
                }
                // List labels use their ordinal even when the source marker is
                // invalid or mixes explicit and automatic numbering.
                item.callout.number = expected_number;

                // Check if the EXPECTED callout exists in the verbatim block
                // (This warns when sequence is broken and the expected number is missing)
                let callout_exists = callouts.iter()
                    .any(|c| c.number == expected_number);
                if !callout_exists {
                    state.add_generic_warning_at(
                        format!("no callout found for <{expected_number}>"),
                        item.location.clone(),
                    );
                }
                items.push(item);
            }

            Ok(Block::CalloutList(CalloutList {
                title: block_metadata.title.clone(),
                metadata: block_metadata.metadata.clone(),
                items,
                location: state.create_location(start+offset, end+offset),
            }))
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
          child_metadata:callout_child_metadata(metadata)
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
        = eol() child:(
            &((attribute_or_anchor_line_match() eol()*)+ callout_child_marker(offset))
            start:position!()
            metadata:parsed_nested_list_metadata(offset, block_metadata.parent_section_level)
            list:callout_child_list(start, offset, &metadata) { list }
            / eol()* callout_child_marker(offset) start:position!()
            metadata:callout_child_metadata(block_metadata)
            list:callout_child_list(start, offset, &metadata) { list }
        ) { child }

        // Inherit section context and text settings without copying the parent's
        // ID, title, or style onto a child that has no metadata of its own.
        rule callout_child_metadata(parent: &BlockParsingMetadata<'input>) -> BlockParsingMetadata<'input>
        = {
            BlockParsingMetadata {
                parent_section_level: parent.parent_section_level,
                substitutions: parent.substitutions,
                hardbreaks: parent.hardbreaks,
                ..BlockParsingMetadata::default()
            }
        }

        // Disable child continuations so `+` blocks remain for the owning callout item.
        rule callout_child_list(start: usize, offset: usize, metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = unordered_list(start, offset, metadata, None, false, false)
        / ordered_list(start, offset, metadata, None, false, false)
        / description_list(start, offset, metadata, false)

        rule callout_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<(CalloutListItem<'input>, String, usize), Error>
        = at_callout_list_item()
        marker:callout_list_marker()
        whitespace()
        first_line_start:position!()
        // Parse first line (principal text)
        first_line:$((!(eol()) [_])*)
        // Parse continuation lines that are part of the same paragraph
        // Stop at list markers, explicit continuations, blank lines, section
        // headers, or block attributes.
        continuation_lines:(
            eol()
            !at_callout_list_item()
            !check_line_is_description_list(offset)
            !(whitespace()* (unordered_list_marker() / ordered_list_marker() / section_level_marker() whitespace() / "[" / "+" whitespace()* eol() / eol()))
            line:$((!(eol()) [_])*)
            { line }
        )*
        first_line_end:position!()
        principal:list_item_principal(offset, block_metadata, first_line_start, first_line_end, first_line, &continuation_lines)
        nested:(!at_list_separator() child:callout_list_nested(offset, block_metadata) { child })?
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        list_dangling_continuation()?
        {
            let principal = principal?;
            let item_end = calculate_item_end(first_line.is_empty() && continuation_lines.is_empty(), span_start, first_line_end);

            let blocks = nested
                .into_iter()
                .chain(explicit_continuations)
                .collect::<Result<Vec<_>, _>>()?;

            let location = state.create_location(span_start+offset, item_end+offset);

            // Create a placeholder callout - will be resolved in callout_list
            // We pass the marker string to the parent rule for resolution
            let callout = if marker == "<.>" {
                CalloutRef::auto(0, location.clone()) // Number will be resolved later
            } else {
                CalloutRef::explicit(0, location.clone())
            };

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((CalloutListItem {
                callout,
                principal,
                blocks,
                location: state.create_location(span_start+offset, actual_end+offset),
            }, marker.to_string(), actual_end))
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

        rule check_start_of_description_list(offset: usize)
        = pos:position!() {?
            if find_dlist_marker(state.input.as_bytes(), pos + offset, true, true) {
                Ok(())
            } else {
                Err("no dlist marker before next blank line")
            }
        }

        /// Like check_start_of_description_list but restricted to the current line.
        /// Used by setext section rules to avoid false positives when a description
        /// list marker (::, ;;) appears later in the document but not on the current line.
        rule check_line_is_description_list(offset: usize)
        = pos:position!() {?
            if find_dlist_marker(state.input.as_bytes(), pos + offset, false, true) {
                Ok(())
            } else {
                Err("no dlist marker on current line")
            }
        }

        rule check_start_of_description_list_in_context(offset: usize, scan_across_eol: bool)
        = pos:position!() {?
            if find_dlist_marker(state.input.as_bytes(), pos + offset, scan_across_eol, true) {
                Ok(())
            } else {
                Err("no dlist marker in this block context")
            }
        }

        rule description_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, scan_across_eol: bool) -> Result<Block<'input>, Error>
        = check_start_of_description_list_in_context(offset, scan_across_eol)
        first_item:description_list_item(offset, block_metadata)
        additional_items:description_list_additional_items(offset, block_metadata)*
        {
            tracing::debug!("Found description list block with auto-attachment support");
            let mut items = vec![first_item?];

            for additional in additional_items {
                items.push(additional?);
            }

            let actual_end = items.last().map_or(span_end, |item| {
                let loc_end = item.location.absolute_end;
                loc_end - offset
            });

            Ok(Block::DescriptionList(DescriptionList {
                title: block_metadata.title.clone(),
                metadata: block_metadata.metadata.clone(),
                items: build_description_list_topology(items),
                location: state.create_location(start+offset, actual_end+offset),
            }))
        }

        // Parse additional description list items (after potential auto-attached content)
        //
        // !at_dlist_block_boundary() prevents continuing the list when a blank line is
        // followed by block attributes. This allows attributes to apply to a new list.
        rule description_list_additional_items(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<DescriptionListItem<'input>, Error>
        = !at_dlist_block_boundary()
        eol()*
        !attribute_or_anchor_line_match()
        !at_callout_list_item()
        check_start_of_description_list(offset)
        item:description_list_item(offset, block_metadata)
        {
            tracing::debug!("Found additional description list item");
            item
        }

        rule description_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<DescriptionListItem<'input>, Error>
        = term_start:position!()
        term:$((!(description_list_marker() (eol() / " " / ![_]) / eol()*<2,2>) [_])+)
        term_end:position!()
        delim_start:position!() delimiter:description_list_marker() delim_end:position!()
        whitespace()?
        principal_start:position!()
        principal_content:$(
            (!eol() [_])*
            // Implicit text continuation: consume subsequent non-blank lines that
            // aren't new dlist entries, list items, continuation markers, or block
            // delimiters. This mirrors paragraph multi-line handling but with
            // dlist-specific stop conditions.
            (eol()
             !eol()                                    // not a blank line
             !at_callout_list_item()
             !check_line_is_description_list(offset)
             !(whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace())  // not a list item
             !("+" (whitespace() / eol() / ![_]))      // not a continuation marker
             !example_delimiter()                      // not a block delimiter
             !listing_delimiter()
             !literal_delimiter()
             !sidebar_delimiter()
             !quote_delimiter()
             !pass_delimiter()
             !comment_delimiter()
             !table_delimiter()
             !(open_delimiter() (whitespace()* eol()))
             !markdown_code_delimiter()
             !attribute_or_anchor_line_match()             // not block metadata
             !heading_boundary(offset)  // not a section heading
             (!eol() [_])+                             // continuation line content
            )*
        )
        principal:description_list_principal(offset, block_metadata, term, term_start, term_end, principal_content, principal_start)
        // Now handle auto-attachment and explicit continuation
        attached_content:description_list_attached_content(offset, block_metadata)*
        {
            tracing::debug!("parsing description list item with auto-attachment");

            let (term, principal_text) = principal?;

            // Collect all attached blocks (auto-attached and explicitly continued)
            let mut description = Vec::with_capacity(attached_content.len());
            for content in attached_content {
                match content {
                    Ok(blocks) => description.extend(blocks),
                    Err(e) => {
                        state.add_warning(Warning::new(
                            WarningKind::ContentRecovery {
                                message: format!("discarded attached content: {e}").into(),
                            },
                            e.source_location().cloned(),
                        ));
                        tracing::error!("Error processing attached content");
                    }
                }
            }

            // Calculate actual end from last attached block, or fall back to end of principal/term.
            // The injected `span_end` captures position after consuming blank lines looking for more
            // continuations (start of the next item), so it's not the right end either — we want
            // the actual content end.
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

            let delimiter_location = state.create_block_location(delim_start, delim_end, offset);
            Ok(DescriptionListItem {
                anchors: vec![],
                term,
                delimiter,
                delimiter_location: Some(delimiter_location),
                principal_text,
                description,
                location: state.create_location(span_start+offset, actual_end+offset),
            })
        }

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

        rule description_list_attached_content(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Vec<Block<'input>>, Error>
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

        rule description_list_auto_attached_list(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Vec<Block<'input>>, Error>
        = eol()* list:description_list_auto_attached_list_with_metadata(offset, block_metadata) { list }
        / eol()* // Consume any blank lines before the list
        &(whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace())
        list_start:position!()
        list:(unordered_list(list_start, offset, block_metadata, None, true, true) / ordered_list(list_start, offset, block_metadata, None, true, true))
        {
            tracing::debug!("Auto-attaching list to description list item");
            Ok(vec![list?])
        }
        / list:nested_callout_list(offset, block_metadata) { Ok(vec![list?]) }

        // Metadata belongs to a nested list only when a list marker follows it.
        rule description_list_auto_attached_list_with_metadata(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Vec<Block<'input>>, Error>
        = &((attribute_or_anchor_line_match() eol()*)+ (whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace() / !at_callout_parent_item(offset) at_callout_list_item()))
          list_start:position!()
          metadata:parsed_nested_list_metadata(offset, block_metadata.parent_section_level)
          list:(callout_list(list_start, offset, &metadata) / unordered_list(list_start, offset, &metadata, None, true, false) / ordered_list(list_start, offset, &metadata, None, true, false))
        {
            Ok(vec![list?])
        }

        // Parse one or more explicit continuations for description lists
        // Same pattern as list_explicit_continuation: + marker followed by a single block
        // Uses block_in_continuation to prevent lists inside continuations from consuming
        // further continuations that belong to the parent item
        rule description_list_explicit_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Vec<Block<'input>>, Error>
        = continuations:(
            eol()* "+" eol()
            block:block_in_continuation(offset, block_metadata.parent_section_level)
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
        rule list_explicit_continuation_immediate(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = eol() !eol() "+" eol()
          block:block_in_continuation(offset, block_metadata.parent_section_level)
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
        rule list_explicit_continuation_ancestor(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = eol() eol()+ "+" eol()
          block:block_in_continuation(offset, block_metadata.parent_section_level)
        {
            tracing::debug!("List ancestor continuation block (1+ empty lines)");
            block
        }

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
        = content_start:position!()
          "\"" quoted_content:$((!"\"" [_])+) "\""
          eol()
          "-- " attr_start:position!() attribution_line:$([^'\n']+)
        {
            tracing::debug!("found quoted paragraph");

            // Parse attribution line: "Author Name, Source Title" or just "Author Name"
            // Intern the slices into the parser arena so downstream inline parsing
            // can produce nodes with the `'input` lifetime.
            let (attr_str, cite_str): (&'input str, Option<&'input str>) = match attribution_line.split_once(',') {
                Some((attr, cite)) => (state.intern_str(attr.trim()), Some(state.intern_str(cite.trim()))),
                None => (state.intern_str(attribution_line.trim()), None),
            };

            let attr_end_offset = attr_start + attr_str.len();
            let (attr_inlines, _) = process_inlines(
                state,
                block_metadata,
                attr_start,
                attr_end_offset,
                offset,
                attr_str,
            )?;

            let cite_inlines = if let Some(cite) = cite_str {
                let cite_offset_in_line = attribution_line.find(',').unwrap_or(0) + 1;
                let cite_raw_start = attr_start + cite_offset_in_line + (attribution_line[cite_offset_in_line..].len() - attribution_line[cite_offset_in_line..].trim_start().len());
                let cite_pos = PositionWithOffset {
                    offset: cite_raw_start,
                    position: state.line_map.offset_to_position(cite_raw_start, state.input),
                };
                let (cite_inlines, _) = process_inlines(
                    state,
                    block_metadata,
                    cite_pos.offset,
                    cite_raw_start + cite.len(),
                    offset,
                    cite,
                )?;
                Some(cite_inlines)
            } else {
                None
            };

            let blocks = document_parser::blocks(quoted_content, state, content_start + offset, block_metadata.parent_section_level, None).unwrap_or_else(|e| {
                adjust_and_log_parse_error(&e, quoted_content, content_start + offset, state, "Error parsing content as blocks in quoted paragraph");
                Ok(Vec::new())
            })?;

            let mut metadata = block_metadata.metadata.clone();
            metadata.style = Some("quote");
            metadata.attribution = Some(Attribution::new(attr_inlines));
            if let Some(inlines) = cite_inlines {
                metadata.citetitle = Some(CiteTitle::new(inlines));
            }

            Ok(Block::DelimitedBlock(DelimitedBlock {
                source_text: None,
                metadata,
                delimiter: "\"",
                inner: DelimitedBlockType::DelimitedQuote(blocks),
                title: block_metadata.title.clone(),
                location: state.create_block_location(start, span_end, offset),
                open_delimiter_location: None,
                close_delimiter_location: None,
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
        rule markdown_blockquote(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = lines:markdown_blockquote_content_line()+ attribution:markdown_blockquote_attribution()?
        {
            tracing::debug!("found markdown blockquote");

            let content: &'input str = state.intern_join(lines.iter(), "\n");
            let content_start = start;

            let mut metadata = block_metadata.metadata.clone();
            metadata.style = Some("quote");
            if let Some((author, author_start, citation)) = attribution {
                let author: &'input str = state.intern_str(&author);
                let author_pos = PositionWithOffset {
                    offset: author_start,
                    position: state.line_map.offset_to_position(author_start, state.input),
                };
                let attr_end_offset = author_start + author.len();
                let (attr_inlines, _) = process_inlines(
                    state,
                    block_metadata,
                    author_pos.offset,
                    attr_end_offset,
                    offset,
                    author,
                )?;
                metadata.attribution = Some(Attribution::new(attr_inlines));

                if let Some((cite, cite_start)) = citation {
                    let cite: &'input str = state.intern_str(&cite);
                    let cite_pos = PositionWithOffset {
                        offset: cite_start,
                        position: state.line_map.offset_to_position(cite_start, state.input),
                    };
                    let (cite_inlines, _) = process_inlines(
                        state,
                        block_metadata,
                        cite_pos.offset,
                        cite_start + cite.len(),
                        offset,
                        cite,
                    )?;
                    metadata.citetitle = Some(CiteTitle::new(cite_inlines));
                }
            }

            let location = state.create_block_location(start, span_end, offset);

            let blocks = if content.trim().is_empty() {
                Vec::new()
            } else {
                document_parser::blocks(content, state, content_start + offset, block_metadata.parent_section_level, None).unwrap_or_else(|e| {
                    adjust_and_log_parse_error(&e, content, content_start + offset, state, "Error parsing content as blocks in markdown blockquote");
                    Ok(Vec::new())
                })?
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                source_text: None,
                metadata,
                delimiter: ">",
                inner: DelimitedBlockType::DelimitedQuote(blocks),
                title: block_metadata.title.clone(),
                location,
                open_delimiter_location: None,
                close_delimiter_location: None,
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
        /// Returns (author, author_start, Option<(citation, cite_start)>)
        rule markdown_blockquote_attribution() -> (String, usize, Option<(String, usize)>)
        = "> -- " author_start:position!() author:$([^(',' | '\n')]+) ", " cite_start:position!() citation:$([^'\n']+) ((eol() !">") / ![_]) {
            (author.trim().to_string(), author_start, Some((citation.trim().to_string(), cite_start)))
        }
        / "> -- " author_start:position!() author:$([^'\n']+) ((eol() !">") / ![_]) {
            (author.trim().to_string(), author_start, None)
        }

        // Explicit verbatim styles take precedence over list and macro syntax,
        // but an opening block delimiter still selects a delimited block.
        rule styled_verbatim_paragraph(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = !normal_paragraph_style(block_metadata) block:paragraph(start, offset, block_metadata) { block }

        // Once started, only a blank line, list continuation, or EOF ends this
        // paragraph. Apparent headings, delimiters, and metadata are its content.
        rule verbatim_paragraph_content() -> &'input str
        = content:$((!(
            eol()*<2,>
            / eol()* ![_]
            / eol() "+" whitespace()* (eol() / ![_])
        ) [_])+) { content }

        rule paragraph(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = admonition:(normal_paragraph_style(block_metadata) value:admonition() { value })?
        content_start:position!()
        content:(
          !normal_paragraph_style(block_metadata) text:verbatim_paragraph_content() { text }
          / $((
            "[[" (!eol() [_])*
            / !(
            eol()*<2,>
            / eol()* ![_]
            / eol() &attributes_line()
            / eol() &anchor_line_match()
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
            / eol() at_callout_parent_item(offset)
            // Callout markers do not interrupt ordinary paragraph text.
            / eol() !at_callout_list_item() list(start, offset, block_metadata)
            / eol() &("+" whitespace()* (eol() / ![_]))  // Only a standalone plus continues a list.
            / eol()* &heading_boundary(offset)
            ) [_]
        )+))
        {
            let is_styled_verbatim = matches!(
                block_metadata.metadata.style,
                Some("source" | "listing" | "literal")
            );

            // A `[comment]`-styled paragraph is a comment that produces no
            // output; keep its raw text on the `Comment` for tooling.
            if block_metadata.metadata.style == Some("comment") {
                return Ok(Block::Comment(Comment {
                    kind: CommentKind::Paragraph,
                    content,
                    location: state.create_block_location(start, span_end, offset),
                }));
            }

            // Indentation selects literal substitutions before ordinary inline processing.
            if content.starts_with(' ')
                && !is_styled_verbatim
                && block_metadata.metadata.style != Some("verse")
            {
                return get_literal_paragraph(state, content, start, content_start, span_end, offset, block_metadata);
            }

            let source_text = content;
            let content = if is_styled_verbatim {
                let content_location =
                    state.create_block_location(content_start, span_end, offset);
                let (verbatim_content, callouts) = resolve_verbatim_callouts(
                    state,
                    content,
                    content_location,
                    block_metadata
                        .substitutions
                        .enabled(&Substitution::Callouts),
                    !block_metadata.metadata.attributes.contains_key("line-comment"),
                );
                let content = if callouts.is_empty() {
                    let verbatim_metadata = BlockParsingMetadata {
                        substitutions: verbatim_substitutions(block_metadata),
                        ..BlockParsingMetadata::default()
                    };
                    process_inlines(
                        state,
                        &verbatim_metadata,
                        content_start,
                        span_end,
                        offset,
                        content,
                    )?
                    .0
                } else {
                    resolve_verbatim_inlines(state, block_metadata, verbatim_content)?
                };
                            state.pending_callouts.extend(callouts);
                content
            } else {
                process_inlines(
                    state,
                    block_metadata,
                    content_start,
                    span_end,
                    offset,
                    content,
                )?
                .0
            };

            // Title should either be an attribute named title, or the title parsed from the block metadata
            let title: Title = if let Some(AttributeValue::String(title)) = block_metadata.metadata.attributes.get("title") {
                vec![InlineNode::PlainText(Plain {
                    content: state.intern_cow(title.clone()),
                    location: state.create_location(start+offset, (start+offset).saturating_add(title.len()).saturating_sub(1)),
                    escaped: false,
                })].into()
            } else {
                block_metadata.title.clone()
            };

            if let Some((variant, admonition_start, admonition_end)) = admonition {
                let Ok(parsed_variant) = AdmonitionVariant::from_str(&variant) else {
                    tracing::error!("invalid admonition variant");
                    return Err(Error::InvalidAdmonitionVariant(
                        Box::new(state.create_error_source_location(state.create_location(admonition_start + offset, admonition_end + offset - 1))),
                        variant
                    ));
                };
                tracing::debug!("found admonition block with variant");
                Ok(Block::Admonition(Admonition{
                    metadata: block_metadata.metadata.clone(),
                    title,
                    blocks: vec![Block::Paragraph(Paragraph {
                        source_text: Some(source_text),
                        content,
                        metadata: block_metadata.metadata.clone(),
                        title: Title::default(),
                        location: state.create_block_location(content_start, span_end, offset),
                    })],
                    location: state.create_block_location(start, span_end, offset),
                    variant: parsed_variant,

                }))
            } else {
                let mut metadata = block_metadata.metadata.clone();
                metadata.move_positional_attributes_to_attributes();

                tracing::debug!(node_count = content.len(), "found paragraph block");
                Ok(Block::Paragraph(Paragraph {
                    source_text: Some(source_text),
                    content,
                    metadata,
                    title,
                    location: state.create_block_location(start, span_end, offset),
                }))
            }
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

        rule anchor() -> Anchor<'input>
        = result:(
            // Double-bracket [[id]] syntax - allows dots in ID since no role shorthand
            // possible.
            //
            // Whitespace is excluded per AsciiDoc documentation at
            // https://docs.asciidoctor.org/asciidoc/latest/attributes/id/#valid-id-characters
            double_open_square_bracket() warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | ' ' | '\t' | '\n' | '\r']+) comma() reftext:$([^']']+) double_close_square_bracket() {
                (id, Some(reftext))
            } /
            double_open_square_bracket() warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | ' ' | '\t' | '\n' | '\r']+) double_close_square_bracket() {
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
            let substituted_id = state.intern_cow(substitute(id, HEADER, &state.document_attributes));
            let substituted_reftext = reftext.map(|rt| state.intern_cow(substitute(rt, HEADER, &state.document_attributes)));
            // `end` is captured before the trailing eol() so the anchor's
            // location doesn't include the newline.
            Anchor {
                id: substituted_id,
                xreflabel: substituted_reftext,
                location: state.create_location(span_start, end),
                bibliography_label: None,
                bibliography: false,
            }
        }

        rule inline_anchor(offset: usize) -> InlineNode<'input>
        = double_open_square_bracket()
        // Whitespace is excluded - IDs must not contain spaces
        warn_anchor_id_with_whitespace()?
        id:$([^'\'' | ',' | ']' | '[' | ' ' | '\t' | '\n' | '\r']+)
        reftext:(
            comma() reftext:$([^']']+) {
                Some(reftext)
            } /
            {
                None
            }
        )
        double_close_square_bracket()
        {
            let substituted_id = state.intern_cow(substitute(id, HEADER, &state.document_attributes));
            let substituted_reftext = reftext.map(|rt| state.intern_cow(substitute(rt, HEADER, &state.document_attributes)));
            InlineNode::InlineAnchor(Anchor {
                id: substituted_id,
                xreflabel: substituted_reftext,
                location: state.create_block_location(span_start, span_end, offset),
                bibliography_label: None,
                bibliography: false,
            })
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

        rule attributes_line() -> (bool, BlockMetadata<'input>)
            // Don't match empty [] followed by blank line - that's a list separator, not
            // block attributes. Without this, `[]\n\n` would be parsed as an empty
            // attributes line, breaking list separation
            = !empty_list_separator() attributes:attributes() eol() {
                let (discrete, metadata, _title_position) = attributes;
                (discrete, metadata)
            }

        // Empty brackets followed by a blank line is a list separator
        rule empty_list_separator()
            = whitespace()* "[" whitespace()* "]" whitespace()* eol() eol()

        pub(crate) rule attributes() -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = !double_open_square_bracket()
              open_square_bracket()
              content_start:position!()
              content:attribute_list_content()
            {
                parse_block_attribute_list(
                    state,
                    content,
                    content_start,
                    span_end,
                    BlockAttributeMode::Block,
                )
            }

        /// Macro attribute parsing - simpler than block attributes.
        ///
        /// Does NOT support shorthand syntax (.role, #id, %option).
        /// Shorthands are only valid in block-level attributes, not inside macro brackets.
        ///
        /// Asciidoctor behavior:
        /// - `image::photo.jpg[.role]` -> alt=".role" (literal text, NOT a role)
        /// - `image::photo.jpg[Diablo 4 picture of Lilith.]` -> alt="Diablo 4 picture of Lilith."
        pub(crate) rule macro_attributes() -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = macro_attributes_for(MacroAttributeContext::General)

        rule image_macro_attributes() -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = macro_attributes_for(MacroAttributeContext::Image)

        rule macro_attributes_for(context: MacroAttributeContext) -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = open_square_bracket()
              content_start:position!()
              content:attribute_list_content()
            {
                parse_block_attribute_list(
                    state,
                    content,
                    content_start,
                    span_end,
                    BlockAttributeMode::Macro(context),
                )
            }

        rule open_square_bracket() = "["
        rule close_square_bracket() = "]"
        rule attribute_list_content() -> &'input str
            = content:$((!last_close_square_bracket() [^'\n' | '\r'])*) close_square_bracket() { content }
        rule last_close_square_bracket()
            = &("]" [^']' | '\n' | '\r']* (eol() / ![_]))
        rule double_open_square_bracket() = "[["
        rule double_close_square_bracket() = "]]"
        rule comma() = ","
        rule period() = "."
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

        /// URL target content following `://`.
        /// Supports query parameters, fragments, and percent escapes while excluding
        /// brackets that delimit the macro attributes.
        /// Spaces must be internal to the target.
        rule url_path() -> String = path:$(url_path_char() (url_path_char() / internal_url_path_spaces())*)
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                state.input,
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
                state.line_map.clone(),
                state.input,
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
        pub rule path() -> String = path:$(path_char() (path_char() / internal_path_spaces())*)
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                state.input,
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

        rule path_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '{' | '}' | '_' | '-' | '.' | '/' | '\\' | '%' | '\u{80}'..='\u{10FFFF}' ]
        rule internal_path_spaces() = [' ']+ &path_char()


        pub rule source() -> Source<'input>
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
        // Works identically in both header and block metadata contexts
        rule document_attribute_match() -> AttributeDeclaration<'input>
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
            let attr_value = if !set {
                RawAttributeValue::Unset
            } else if let Some(v) = value {
                RawAttributeValue::Text(v)
            } else {
                RawAttributeValue::Set
            };
            AttributeDeclaration { name: key, value: attr_value }
        }
        / expected!("document attribute key starting with ':'")

        rule position() -> PositionWithOffset = offset:position!() {
            PositionWithOffset {
                offset,
                position: state.line_map.offset_to_position(offset, state.input)
            }
        }

    }
}
