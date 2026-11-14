use std::{path::PathBuf, rc::Rc};

use crate::{
    Block, CompatibilityMode, DelimitedBlockType, Error, InlineNode, Options, WarningKind,
    grammar::{ParserState, state::BlockContext},
    model::SourceRange,
};

use super::super::{construction::ConstructionMode, coordinator::document_with};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[rstest::rstest]
#[case::open(include_str!("../../../../fixtures/tests/open_block_with_paragraphs.adoc"))]
#[case::styled_open(include_str!("../../../../fixtures/tests/styled_open_blocks.adoc"))]
#[case::open_context(include_str!("../../../../fixtures/tests/styled_open_contexts.adoc"))]
#[case::quote(include_str!("../../../../fixtures/tests/compatibility_quote_content.adoc"))]
#[case::quote_snapshots(include_str!("../../../../fixtures/tests/compatibility_quote_snapshots.adoc"))]
#[case::quote_locations(include_str!("../../../../fixtures/tests/compatibility_quote_locations.adoc"))]
#[case::admonition(include_str!("../../../../fixtures/tests/admonition_block.adoc"))]
#[case::admonition_forms(include_str!("../../../../fixtures/tests/admonition_source_forms.adoc"))]
#[case::delimited_header(include_str!("../../../../fixtures/tests/delimited_block_with_header.adoc"))]
#[case::delimited_section(include_str!("../../../../fixtures/tests/delimited_block_within_section.adoc"))]
#[case::listing(include_str!("../../../../fixtures/tests/source_block_with_delimiter_in_content.adoc"))]
#[case::literal(include_str!("../../../../fixtures/tests/literal_block_with_comments.adoc"))]
#[case::passthrough(include_str!("../../../../fixtures/tests/passthrough_block_with_comments.adoc"))]
#[case::comment(include_str!("../../../../fixtures/tests/comment_block_style.adoc"))]
#[case::stem(include_str!("../../../../fixtures/tests/stem_blocks.adoc"))]
#[case::caption_after_body(
    "= Captions\n:example-caption: Old\n\n.Before body\n====\n:example-caption: New\n\nText.\n====\n\n.After body\n====\nText.\n===="
)]
#[case::unicode("Préface 世界.\n\n== Hèading\n\n====\nÉxample 世界.\n\n****\n嵌套.\n****\n====")]
fn compound_driver_preserves_existing_construction(#[case] input: &str) -> TestResult {
    assert_equivalent(input)
}

fn assert_equivalent(input: &str) -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        crate::source_tests::assert_equivalent(input, mode)?;
        let reference_arena = bumpalo::Bump::new();
        let ordered_arena = bumpalo::Bump::new();
        let options = Rc::new(Options::builder().with_compatibility_mode(mode).build()?);
        let mut reference_state = ParserState::new(input, &reference_arena, Rc::clone(&options));
        let mut ordered_state = ParserState::new(input, &ordered_arena, options);
        let reference = document_with(
            input,
            &mut reference_state,
            ConstructionMode::Recursive,
            &mut |_, _| Ok(()),
        )??;
        let ordered = document_with(
            input,
            &mut ordered_state,
            ConstructionMode::Ordered,
            &mut |_, _| Ok(()),
        )??;
        assert_eq!(ordered, reference, "{mode:?}");
        let reference_warnings =
            std::mem::take(&mut *reference_state.warnings.borrow_mut()).into_vec();
        let ordered_warnings = std::mem::take(&mut *ordered_state.warnings.borrow_mut()).into_vec();
        assert_eq!(ordered_warnings, reference_warnings, "{mode:?}");
    }
    Ok(())
}

#[test]
fn compound_driver_commits_parents_before_constructing_children() -> TestResult {
    let input = "== Parent\n\n====\n{value}\n\n****\n{value}\n****\n====\n\n{value}";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let arena = bumpalo::Bump::new();
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut state = ParserState::new(input, &arena, Rc::new(options));
        let mut locations = Vec::new();
        let mut paragraphs = Vec::new();
        document_with(
            input,
            &mut state,
            ConstructionMode::Ordered,
            &mut |block, state| {
                locations.push(block.location().absolute_start);
                if let Block::DelimitedBlock(block) = block
                    && let DelimitedBlockType::DelimitedExample(body)
                    | DelimitedBlockType::DelimitedSidebar(body) = &block.inner
                {
                    assert_eq!(body, &Vec::<Block<'_>>::new());
                    let value = if matches!(block.inner, DelimitedBlockType::DelimitedExample(_)) {
                        "example"
                    } else {
                        "sidebar"
                    };
                    Rc::make_mut(&mut state.document_attributes)
                        .set_text("value".into(), value.into());
                }
                if let Block::Paragraph(paragraph) = block
                    && let [InlineNode::PlainText(plain)] = paragraph.content.as_slice()
                {
                    paragraphs.push(plain.content);
                }
                Ok(())
            },
        )??;
        assert_eq!(locations.len(), 6);
        assert!(
            locations
                .iter()
                .zip(locations.iter().skip(1))
                .all(|(a, b)| a < b)
        );
        assert_eq!(paragraphs, ["example", "sidebar", "sidebar"]);
        assert_eq!(state.block_context, BlockContext::Document);
    }
    Ok(())
}

#[rstest::rstest]
#[case::primary(None, 3)]
#[case::included(Some(PathBuf::from("/virtual/part.adoc")), 41)]
fn compound_driver_restores_context_when_a_child_handler_fails(
    #[case] file: Option<PathBuf>,
    #[case] line: u32,
) -> TestResult {
    let input = "====\n\nfirst\n\n:later: value\n\nlast\n====";
    let mut state = ParserState::new_for_test(input);
    if let Some(file) = &file {
        state.source_ranges.push(SourceRange {
            start_offset: "====\n\n".len(),
            end_offset: input.len(),
            file: Some(file.clone()),
            file_chain: vec!["part.adoc".into()],
            start_line: 41,
            source_start_offset: 100,
            column_shift: 0,
        });
    }
    let mut calls = 0;
    let result = document_with(
        input,
        &mut state,
        ConstructionMode::Ordered,
        &mut |block, state| {
            calls += 1;
            if matches!(block, Block::Paragraph(_)) {
                return Err(Error::Extension(
                    Box::new(state.create_error_source_location(block.location().clone())),
                    "deliberate compound child failure".into(),
                ));
            }
            Ok(())
        },
    )?;
    let Err(Error::Extension(location, _)) = result else {
        return Err("expected a located child-handler failure".into());
    };
    assert_eq!(location.location.start.line, line);
    assert_eq!(location.file, file);
    assert_eq!(calls, 2);
    assert_eq!(state.document_attributes.text("later"), None);
    assert_eq!(state.block_context, BlockContext::Document);
    Ok(())
}

#[test]
fn compound_driver_keeps_recovery_bounded_without_replaying_child_handlers() -> TestResult {
    let input = "= Document\n\n== Parent\n\n[quote]\n____\n=== Child\n\nText.\n\n== Ancestor\n____";
    let mut state = ParserState::new_for_test(input);
    let mut sections = Vec::new();
    let document = document_with(
        input,
        &mut state,
        ConstructionMode::Ordered,
        &mut |block, _| {
            if let Block::Section(section) = block {
                sections.push(section.level);
            }
            Ok(())
        },
    )??;
    assert_eq!(sections, [1, 2]);
    let [Block::Section(parent)] = document.blocks.as_slice() else {
        return Err("expected the surviving parent section".into());
    };
    let [Block::DelimitedBlock(quote)] = parent.content.as_slice() else {
        return Err("expected the recovered quote".into());
    };
    let DelimitedBlockType::DelimitedQuote(body) = &quote.inner else {
        return Err("expected quote content".into());
    };
    assert_eq!(body, &Vec::<Block<'_>>::new());
    assert_eq!(document.toc_entries.len(), 1);
    let warnings = std::mem::take(&mut *state.warnings.borrow_mut()).into_vec();
    assert_eq!(warnings.len(), 1);
    let warning = warnings.first().ok_or("expected a recovery warning")?;
    assert!(matches!(warning.kind, WarningKind::ContentRecovery { .. }));
    assert_eq!(
        warning
            .location
            .as_ref()
            .map(|location| location.location.start.line),
        Some(11)
    );
    assert_eq!(state.block_context, BlockContext::Document);
    Ok(())
}

#[rstest::rstest]
#[case::nested_basic(include_str!("../../../../fixtures/tests/table_nested_basic.adoc"))]
#[case::nested_multiple(include_str!("../../../../fixtures/tests/table_nested_multiple.adoc"))]
#[case::nested_content(include_str!("../../../../fixtures/tests/table_nested_with_content.adoc"))]
#[case::header_footer(include_str!("../../../../fixtures/tests/table_with_header_and_footer.adoc"))]
#[case::rowspan(include_str!("../../../../fixtures/tests/table_rowspan_with_cols.adoc"))]
#[case::combined_spans(include_str!("../../../../fixtures/tests/table_cell_span_combined.adoc"))]
#[case::colspan(include_str!("../../../../fixtures/tests/table_cell_colspan.adoc"))]
#[case::footnotes(include_str!("../../../../fixtures/tests/table_cell_footnotes.adoc"))]
#[case::footnote_scope(include_str!("../../../../fixtures/tests/table_cell_footnote_scope.adoc"))]
#[case::incomplete(include_str!("../../../../fixtures/tests/table_incomplete_row.adoc"))]
#[case::incomplete_single_line(include_str!("../../../../fixtures/tests/table_incomplete_row_single_line.adoc"))]
#[case::csv(include_str!("../../../../fixtures/tests/table_csv_no_header.adoc"))]
#[case::duplicated_styles("[cols=\"alh\",options=noheader]\n|===\n3*|Text footnote:[note].\n|===")]
#[case::dropped_ascii_doc(
    "[cols=\"1a\",options=noheader]\n|===\n2+a|Text footnote:[discarded].\n\n== Dropped\n\n{missing}\n|Kept footnote:[kept].\n|==="
)]
#[case::unterminated("[cols=\"a\",options=noheader]\n|===\n|Text footnote:[note].")]
fn table_driver_preserves_existing_construction(#[case] input: &str) -> TestResult {
    assert_equivalent(input)
}

struct UnusedProcessor;

impl crate::extensions::BlockMacroProcessor for UnusedProcessor {
    fn process(
        &mut self,
        _: &crate::extensions::MacroInvocation<'_>,
        _: &mut crate::extensions::MacroExecutionContext,
    ) -> Result<crate::extensions::BlockMacroOutput, String> {
        Err("internal boundary tests do not execute public processors".into())
    }
}

fn probe_options(mode: CompatibilityMode) -> Result<Options<'static>, Box<dyn std::error::Error>> {
    let mut registry = crate::extensions::Registry::builder();
    registry.register(crate::extensions::BlockMacro::new("probe", || {
        UnusedProcessor
    }))?;
    let mut options = Options::builder().with_compatibility_mode(mode).build()?;
    options.extensions = Some(registry.build());
    Ok(options)
}

#[test]
fn table_driver_commits_each_duplicate_with_isolated_attributes() -> TestResult {
    let input = "[cols=\"3*a\",options=noheader]\n|===\n3*a|probe::copy[]\n\n{value}\n|===\n\nprobe::outside[]\n\n{value}";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let arena = bumpalo::Bump::new();
        let mut state = ParserState::new(input, &arena, Rc::new(probe_options(mode)?));
        let mut calls = Vec::new();
        let mut paragraphs = Vec::new();
        document_with(
            input,
            &mut state,
            ConstructionMode::Ordered,
            &mut |block, state| {
                if let Block::Extension(extension) = block {
                    calls.push((
                        extension.invocation.target.to_string(),
                        state.document_attributes.text("value").map(str::to_owned),
                    ));
                    let value = state.intern_str(&calls.len().to_string());
                    Rc::make_mut(&mut state.document_attributes)
                        .set_text("value".into(), value.into());
                }
                if let Block::Paragraph(paragraph) = block {
                    paragraphs.push(
                        paragraph
                            .content
                            .iter()
                            .filter_map(|node| {
                                if let InlineNode::PlainText(plain) = node {
                                    Some(plain.content)
                                } else {
                                    None
                                }
                            })
                            .collect::<String>(),
                    );
                }
                Ok(())
            },
        )??;
        assert_eq!(
            calls,
            [
                ("copy".into(), None),
                ("copy".into(), None),
                ("copy".into(), None),
                ("outside".into(), None)
            ]
        );
        assert_eq!(paragraphs, ["1", "2", "3", "4"]);
        assert_eq!(state.document_attributes.text("value"), Some("4"));
    }
    Ok(())
}

#[test]
fn table_driver_ignores_macros_in_plain_literal_header_and_rejected_cells() -> TestResult {
    let input = "[cols=\"1a\",options=header]\n|===\n|probe::header[]\n\n2+a|probe::discarded[]\n|probe::accepted[]\n|===\n\n[cols=\"1l\",options=noheader]\n|===\n|probe::literal[]\n|===\n\n[cols=\"1\",options=noheader]\n|===\n|probe::plain[]\n|===";
    let arena = bumpalo::Bump::new();
    let mut state = ParserState::new(
        input,
        &arena,
        Rc::new(probe_options(CompatibilityMode::Asciidoctor)?),
    );
    let mut calls = Vec::new();
    document_with(
        input,
        &mut state,
        ConstructionMode::Ordered,
        &mut |block, _| {
            if let Block::Extension(extension) = block {
                calls.push(extension.invocation.target.to_string());
            }
            Ok(())
        },
    )??;
    assert_eq!(calls, ["accepted"]);
    assert!(
        state
            .warnings
            .borrow()
            .iter()
            .any(|warning| matches!(warning.kind, WarningKind::TableCellOverflow { .. }))
    );
    Ok(())
}

#[test]
fn table_driver_restores_cell_scope_after_handler_failure() -> TestResult {
    let input = "[cols=\"a\",options=noheader]\n|===\n|:cell-only: local\n\nprobe::fail[]\n\nprobe::later[]\n|===\n\nprobe::outside[]";
    let arena = bumpalo::Bump::new();
    let mut state = ParserState::new(
        input,
        &arena,
        Rc::new(probe_options(CompatibilityMode::Asciidoctor)?),
    );
    let outer_tracker = Rc::clone(&state.footnote_tracker);
    let mut calls = Vec::new();
    let result = document_with(
        input,
        &mut state,
        ConstructionMode::Ordered,
        &mut |block, state| {
            if let Block::Extension(extension) = block {
                calls.push(extension.invocation.target.to_string());
                assert_eq!(state.document_attributes.text("cell-only"), Some("local"));
                return Err(Error::Parse(
                    Box::new(state.create_error_source_location(block.location().clone())),
                    "stop".into(),
                ));
            }
            Ok(())
        },
    )?;
    assert!(result.is_err());
    assert_eq!(calls, ["fail"]);
    assert_eq!(state.document_attributes.text("cell-only"), None);
    assert!(Rc::ptr_eq(&state.footnote_tracker, &outer_tracker));
    assert_eq!(state.block_context, BlockContext::Document);
    assert_eq!(state.toc_entries.len(), 0);
    Ok(())
}

#[rstest::rstest]
#[case::checklist_contexts(include_str!("../../../../fixtures/tests/checklist_contexts.adoc"))]
#[case::compatibility_list_attachments(include_str!("../../../../fixtures/tests/compatibility_list_attachments.adoc"))]
#[case::compatibility_list_principal(include_str!("../../../../fixtures/tests/compatibility_list_principal.adoc"))]
#[case::compatibility_list_contexts(include_str!("../../../../fixtures/tests/compatibility_list_contexts.adoc"))]
#[case::list_dangling_continuation(include_str!("../../../../fixtures/tests/list_dangling_continuation.adoc"))]
#[case::list_continuation_comments(include_str!("../../../../fixtures/tests/list_continuation_comments.adoc"))]
#[case::list_continuation_comment_contexts(include_str!("../../../../fixtures/tests/list_continuation_comment_contexts.adoc"))]
#[case::list_anchor_ownership(include_str!("../../../../fixtures/tests/list_anchor_ownership.adoc"))]
#[case::list_ancestor_continuation(include_str!("../../../../fixtures/tests/list_ancestor_continuation.adoc"))]
#[case::list_to_section_transition(include_str!("../../../../fixtures/tests/list_to_section_transition.adoc"))]
#[case::list_separator_comment(include_str!("../../../../fixtures/tests/list_separator_comment.adoc"))]
#[case::list_multiline(include_str!("../../../../fixtures/tests/list_multiline.adoc"))]
#[case::list_followed_by_paragraph(include_str!("../../../../fixtures/tests/list_followed_by_paragraph.adoc"))]
#[case::ordered_list_with_explicit_continuation(include_str!("../../../../fixtures/tests/ordered_list_with_explicit_continuation.adoc"))]
#[case::ordered_list_with_checkmarks(include_str!("../../../../fixtures/tests/ordered_list_with_checkmarks.adoc"))]
#[case::ordered_list(include_str!("../../../../fixtures/tests/ordered_list.adoc"))]
#[case::nested_ordered_list_with_checkmarks(include_str!("../../../../fixtures/tests/nested_ordered_list_with_checkmarks.adoc"))]
#[case::nested_list_with_parent_continuation(include_str!("../../../../fixtures/tests/nested_list_with_parent_continuation.adoc"))]
#[case::nested_list_block_attributes(include_str!("../../../../fixtures/tests/nested_list_block_attributes.adoc"))]
#[case::nested_list_block_attribute_boundaries(include_str!("../../../../fixtures/tests/nested_list_block_attribute_boundaries.adoc"))]
#[case::unordered_list_with_explicit_continuation(include_str!("../../../../fixtures/tests/unordered_list_with_explicit_continuation.adoc"))]
#[case::unordered_list_continuation_comment_midchain(include_str!("../../../../fixtures/tests/unordered_list_continuation_comment_midchain.adoc"))]
#[case::unordered_list(include_str!("../../../../fixtures/tests/unordered_list.adoc"))]
#[case::unindented_cross_type_list_nesting(include_str!("../../../../fixtures/tests/unindented_cross_type_list_nesting.adoc"))]
#[case::titled_list_with_nested(include_str!("../../../../fixtures/tests/titled_list_with_nested.adoc"))]
#[case::rejected_callout_style("* Parent\n[listing]\n<1> Text")]
fn ordinary_list_driver_preserves_existing_construction(#[case] input: &str) -> TestResult {
    assert_equivalent(input)
}

#[test]
fn ordinary_list_driver_runs_attached_macros_before_later_principals() -> TestResult {
    let input = "* First.\n+\nprobe::one[]\n* {value}\n** Nested.\n+\nprobe::two[]\n** {value}\n* {value}\n\n// Separate lists.\n\n. Ordered.\n+\nprobe::three[]\n. {value}";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let arena = bumpalo::Bump::new();
        let mut state = ParserState::new(input, &arena, Rc::new(probe_options(mode)?));
        let mut calls = Vec::new();
        let document = document_with(
            input,
            &mut state,
            ConstructionMode::Ordered,
            &mut |block, state| {
                if let Block::Extension(extension) = block {
                    calls.push(extension.invocation.target.to_string());
                    Rc::make_mut(&mut state.document_attributes)
                        .set_text("value".into(), extension.invocation.target.clone());
                }
                Ok(())
            },
        )??;
        assert_eq!(calls, ["one", "two", "three"]);
        let Some(Block::UnorderedList(list)) = document.blocks.first() else {
            return Err("missing list".into());
        };
        let Some(second) = list.items.get(1) else {
            return Err("missing second item".into());
        };
        let Some(InlineNode::PlainText(text)) = second.principal.first() else {
            return Err("missing principal".into());
        };
        assert_eq!(text.content, "one");
        let Some(Block::UnorderedList(nested)) = second.blocks.first() else {
            return Err("missing nested list".into());
        };
        let Some(item) = nested.items.get(1) else {
            return Err("missing nested second item".into());
        };
        let Some(InlineNode::PlainText(text)) = item.principal.first() else {
            return Err("missing nested principal".into());
        };
        assert_eq!(text.content, "two");
        let Some(item) = list.items.last() else {
            return Err("missing last item".into());
        };
        let Some(InlineNode::PlainText(text)) = item.principal.first() else {
            return Err("missing last principal".into());
        };
        assert_eq!(text.content, "two");
        let Some(Block::OrderedList(list)) = document.blocks.last() else {
            return Err("missing ordered list".into());
        };
        let Some(item) = list.items.last() else {
            return Err("missing ordered second item".into());
        };
        let Some(InlineNode::PlainText(text)) = item.principal.first() else {
            return Err("missing ordered principal".into());
        };
        assert_eq!(text.content, "three");
    }
    Ok(())
}

#[test]
fn ordinary_list_driver_restores_scopes_when_an_attached_handler_fails() -> TestResult {
    let input = "* Parent\n** Nested\n+\n====\nprobe::fail[]\n====\n** Later\n\nprobe::outside[]";
    let arena = bumpalo::Bump::new();
    let mut state = ParserState::new(
        input,
        &arena,
        Rc::new(probe_options(CompatibilityMode::Asciidoctor)?),
    );
    let mut calls = Vec::new();
    let result = document_with(
        input,
        &mut state,
        ConstructionMode::Ordered,
        &mut |block, state| {
            if let Block::Extension(extension) = block {
                calls.push(extension.invocation.target.to_string());
                return Err(Error::Parse(
                    Box::new(state.create_error_source_location(block.location().clone())),
                    "stop".into(),
                ));
            }
            Ok(())
        },
    )?;
    assert!(result.is_err());
    assert_eq!(calls, ["fail"]);
    assert!(state.ordinary_list_scope.is_none());
    assert!(state.description_list_scope.is_none());
    assert!(state.callout_list_offset.is_none());
    assert_eq!(state.block_context, BlockContext::Document);
    Ok(())
}

#[rstest::rstest]
#[case::callout_list_basic(include_str!("../../../../fixtures/tests/callout_list_basic.adoc"))]
#[case::callout_list_auto_numbering(include_str!("../../../../fixtures/tests/callout_list_auto_numbering.adoc"))]
#[case::callout_nested_lists(include_str!("../../../../fixtures/tests/callout_nested_lists.adoc"))]
#[case::callout_list_with_language(include_str!("../../../../fixtures/tests/callout_list_with_language.adoc"))]
#[case::callout_list_not_after_verbatim(include_str!("../../../../fixtures/tests/callout_list_not_after_verbatim.adoc"))]
#[case::callout_list_non_sequential(include_str!("../../../../fixtures/tests/callout_list_non_sequential.adoc"))]
#[case::callout_list_multiple_callouts_same_line(include_str!("../../../../fixtures/tests/callout_list_multiple_callouts_same_line.adoc"))]
#[case::callout_list_mixed_auto_explicit(include_str!("../../../../fixtures/tests/callout_list_mixed_auto_explicit.adoc"))]
#[case::callout_list_literal_block(include_str!("../../../../fixtures/tests/callout_list_literal_block.adoc"))]
#[case::callout_list_continuation(include_str!("../../../../fixtures/tests/callout_list_continuation.adoc"))]
#[case::footnote_order(
    "----\ncode <1> <2>\n----\n<1> footnote:[first]\n+\nA footnote:[child].\n<2> footnote:[second]"
)]
fn callout_list_driver_preserves_existing_construction(#[case] input: &str) -> TestResult {
    assert_equivalent(input)
}

#[test]
fn callout_list_driver_commits_attached_macros_before_later_items() -> TestResult {
    let input =
        "----\ncode <1> <2>\n----\n<1> First.\n+\nprobe::one[]\n<2> {value}\n+\nprobe::two[]";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let arena = bumpalo::Bump::new();
        let mut state = ParserState::new(input, &arena, Rc::new(probe_options(mode)?));
        let mut calls = Vec::new();
        let document = document_with(
            input,
            &mut state,
            ConstructionMode::Ordered,
            &mut |block, state| {
                if let Block::Extension(extension) = block {
                    calls.push(extension.invocation.target.to_string());
                    Rc::make_mut(&mut state.document_attributes)
                        .set_text("value".into(), extension.invocation.target.clone());
                }
                Ok(())
            },
        )??;
        assert_eq!(calls, ["one", "two"]);
        let Some(Block::CalloutList(list)) = document.blocks.last() else {
            return Err("missing callout list".into());
        };
        let Some(item) = list.items.last() else {
            return Err("missing callout item".into());
        };
        let Some(InlineNode::PlainText(text)) = item.principal.first() else {
            return Err("missing callout principal".into());
        };
        assert_eq!(text.content, "one");
        assert_eq!(item.callout.number, 2);
        assert_eq!(state.warnings.borrow().len(), 0);
        assert!(state.callout_list_offset.is_none());
    }
    Ok(())
}

#[test]
fn callout_list_driver_restores_scope_after_handler_failure() -> TestResult {
    let input =
        "----\ncode <1> <2>\n----\n<1> First.\n+\nprobe::fail[]\n<2> Later.\n+\nprobe::later[]";
    let arena = bumpalo::Bump::new();
    let mut state = ParserState::new(
        input,
        &arena,
        Rc::new(probe_options(CompatibilityMode::Asciidoctor)?),
    );
    let mut calls = Vec::new();
    let result = document_with(
        input,
        &mut state,
        ConstructionMode::Ordered,
        &mut |block, state| {
            if let Block::Extension(extension) = block {
                calls.push(extension.invocation.target.to_string());
                assert!(state.callout_list_offset.is_some());
                return Err(Error::Parse(
                    Box::new(state.create_error_source_location(block.location().clone())),
                    "stop".into(),
                ));
            }
            Ok(())
        },
    )?;
    assert!(result.is_err());
    assert_eq!(calls, ["fail"]);
    assert!(state.callout_list_offset.is_none());
    assert_eq!(state.pending_callouts, Vec::<crate::CalloutMarker>::new());
    assert!(state.ordinary_list_scope.is_none());
    Ok(())
}

#[rstest::rstest]
#[case::description_list_anchor_boundary(include_str!("../../../../fixtures/tests/description_list_anchor_boundary.adoc"))]
#[case::description_list_basic(include_str!("../../../../fixtures/tests/description_list_basic.adoc"))]
#[case::description_list_block_attributes_boundary(include_str!("../../../../fixtures/tests/description_list_block_attributes_boundary.adoc"))]
#[case::description_list_boundaries(include_str!("../../../../fixtures/tests/description_list_boundaries.adoc"))]
#[case::description_list_complex_terms(include_str!("../../../../fixtures/tests/description_list_complex_terms.adoc"))]
#[case::description_list_continuations(include_str!("../../../../fixtures/tests/description_list_continuations.adoc"))]
#[case::description_list_delimiter_transitions(include_str!("../../../../fixtures/tests/description_list_delimiter_transitions.adoc"))]
#[case::description_list_different_delimiters(include_str!("../../../../fixtures/tests/description_list_different_delimiters.adoc"))]
#[case::description_list_effective_styles(include_str!("../../../../fixtures/tests/description_list_effective_styles.adoc"))]
#[case::description_list_empty_description(include_str!("../../../../fixtures/tests/description_list_empty_description.adoc"))]
#[case::description_list_explicit_continuation(include_str!("../../../../fixtures/tests/description_list_explicit_continuation.adoc"))]
#[case::description_list_followed_by_table_with_attributes(include_str!("../../../../fixtures/tests/description_list_followed_by_table_with_attributes.adoc"))]
#[case::description_list_hash_term_in_section(include_str!("../../../../fixtures/tests/description_list_hash_term_in_section.adoc"))]
#[case::description_list_implicit_continuation(include_str!("../../../../fixtures/tests/description_list_implicit_continuation.adoc"))]
#[case::description_list_in_sidebar_section(include_str!("../../../../fixtures/tests/description_list_in_sidebar_section.adoc"))]
#[case::description_list_mixed_content(include_str!("../../../../fixtures/tests/description_list_mixed_content.adoc"))]
#[case::description_list_multiline_formatting_locations(include_str!("../../../../fixtures/tests/description_list_multiline_formatting_locations.adoc"))]
#[case::description_list_multiple_blank_lines(include_str!("../../../../fixtures/tests/description_list_multiple_blank_lines.adoc"))]
#[case::description_list_nested_lists(include_str!("../../../../fixtures/tests/description_list_nested_lists.adoc"))]
#[case::description_list_no_blank_line(include_str!("../../../../fixtures/tests/description_list_no_blank_line.adoc"))]
#[case::description_list_no_inline_with_lists(include_str!("../../../../fixtures/tests/description_list_no_inline_with_lists.adoc"))]
#[case::description_list_principal_source_locations(include_str!("../../../../fixtures/tests/description_list_principal_source_locations.adoc"))]
#[case::description_list_separated_paragraphs(include_str!("../../../../fixtures/tests/description_list_separated_paragraphs.adoc"))]
#[case::description_list_term_inlines(include_str!("../../../../fixtures/tests/description_list_term_inlines.adoc"))]
#[case::description_list_with_admonition(include_str!("../../../../fixtures/tests/description_list_with_admonition.adoc"))]
#[case::description_list_with_auto_attached_list(include_str!("../../../../fixtures/tests/description_list_with_auto_attached_list.adoc"))]
#[case::description_list_with_continuation(include_str!("../../../../fixtures/tests/description_list_with_continuation.adoc"))]
#[case::description_list_with_ordered_list(include_str!("../../../../fixtures/tests/description_list_with_ordered_list.adoc"))]
#[case::description_list_with_surrounding_blocks(include_str!("../../../../fixtures/tests/description_list_with_surrounding_blocks.adoc"))]
#[case::nested_description_lists(include_str!("../../../../fixtures/tests/nested_description_lists.adoc"))]
#[case::dlist_continuation_trailing_comment(include_str!("../../../../fixtures/tests/dlist_continuation_trailing_comment.adoc"))]
#[case::dlist_continuation_trailing_block_comment(include_str!("../../../../fixtures/tests/dlist_continuation_trailing_block_comment.adoc"))]
#[case::dlist_continuation_comment_at_eof(include_str!("../../../../fixtures/tests/dlist_continuation_comment_at_eof.adoc"))]
fn description_list_driver_preserves_existing_construction(#[case] input: &str) -> TestResult {
    assert_equivalent(input)
}

#[test]
fn description_list_driver_runs_attachments_before_later_principals() -> TestResult {
    let input = "Term:: First.\n+\nprobe::one[]\nNext:: {value}\n+\nprobe::two[]";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let arena = bumpalo::Bump::new();
        let mut state = ParserState::new(input, &arena, Rc::new(probe_options(mode)?));
        let mut calls = Vec::new();
        let document = document_with(
            input,
            &mut state,
            ConstructionMode::Ordered,
            &mut |block, state| {
                if let Block::Extension(extension) = block {
                    calls.push(extension.invocation.target.to_string());
                    Rc::make_mut(&mut state.document_attributes)
                        .set_text("value".into(), extension.invocation.target.clone());
                }
                Ok(())
            },
        )??;
        assert_eq!(calls, ["one", "two"]);
        let Some(Block::DescriptionList(list)) = document.blocks.last() else {
            return Err("missing description list".into());
        };
        let Some(definition) = list.items.last().and_then(|item| item.definition.as_ref()) else {
            return Err("missing definition".into());
        };
        let Some(InlineNode::PlainText(text)) = definition
            .principal
            .as_ref()
            .and_then(|principal| principal.first())
        else {
            return Err("missing principal".into());
        };
        assert_eq!(text.content, "one");
        assert!(state.description_list_scope.is_none());
    }
    Ok(())
}

#[rstest::rstest]
#[case::explicit("Term:: First.\n+\nprobe::fail[]\nNext:: Later.\n+\nprobe::later[]")]
#[case::ordinary_child(
    "Term:: First.\n* Child.\n+\nprobe::fail[]\nNext:: Later.\n+\nprobe::later[]"
)]
#[case::table_cell(
    "Term:: First.\n+\n[cols=\"a\",options=noheader]\n|===\n|probe::fail[]\n|===\nNext:: Later.\n+\nprobe::later[]"
)]
fn description_list_driver_does_not_recover_handler_failure(#[case] input: &str) -> TestResult {
    let arena = bumpalo::Bump::new();
    let mut state = ParserState::new(
        input,
        &arena,
        Rc::new(probe_options(CompatibilityMode::Asciidoctor)?),
    );
    let mut calls = Vec::new();
    let result = document_with(
        input,
        &mut state,
        ConstructionMode::Ordered,
        &mut |block, state| {
            if let Block::Extension(extension) = block {
                calls.push(extension.invocation.target.to_string());
                return Err(Error::Parse(
                    Box::new(state.create_error_source_location(block.location().clone())),
                    "stop".into(),
                ));
            }
            Ok(())
        },
    )?;
    assert!(result.is_err());
    assert_eq!(calls, ["fail"]);
    assert!(state.description_list_scope.is_none());
    assert!(state.ordinary_list_scope.is_none());
    assert!(state.warnings.borrow().is_empty());
    Ok(())
}

#[rstest::rstest]
#[case::explicit("Term:: First.\n+\nprobe::[]\n+\nprobe::kept[]\nNext:: Last.")]
#[case::ordinary_child("Term:: First.\n* Child.\n+\nprobe::[]\n+\nprobe::kept[]\nNext:: Last.")]
fn description_list_driver_preserves_native_macro_recovery(#[case] input: &str) -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let mut registry = crate::extensions::Registry::builder();
        registry.register(
            crate::extensions::BlockMacro::new("probe", || UnusedProcessor)
                .with_target(crate::extensions::MacroTarget::Required),
        )?;
        let mut options = Options::builder().with_compatibility_mode(mode).build()?;
        options.extensions = Some(registry.build());
        let options = Rc::new(options);
        let expected_arena = bumpalo::Bump::new();
        let ordered_arena = bumpalo::Bump::new();
        let mut expected_state = ParserState::new(input, &expected_arena, Rc::clone(&options));
        let mut ordered_state = ParserState::new(input, &ordered_arena, options);
        let expected = document_with(
            input,
            &mut expected_state,
            ConstructionMode::Recursive,
            &mut |_, _| Ok(()),
        )??;
        let mut calls = Vec::new();
        let ordered = document_with(
            input,
            &mut ordered_state,
            ConstructionMode::Ordered,
            &mut |block, _| {
                if let Block::Extension(extension) = block {
                    calls.push(extension.invocation.target.to_string());
                }
                Ok(())
            },
        )??;
        assert_eq!(calls, ["kept"]);
        assert_eq!(ordered, expected);
        assert_eq!(
            &**ordered_state.warnings.borrow(),
            &**expected_state.warnings.borrow()
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case::quoted_paragraph(include_str!("../../../../fixtures/tests/quoted_paragraph.adoc"))]
#[case::markdown_blockquote(include_str!("../../../../fixtures/tests/markdown_blockquote.adoc"))]
#[case::quote_with_url_attribution(include_str!("../../../../fixtures/tests/quote_with_url_attribution.adoc"))]
#[case::quote_with_attribution(include_str!("../../../../fixtures/tests/quote_with_attribution.adoc"))]
#[case::quote_block_with_url_citation(include_str!("../../../../fixtures/tests/quote_block_with_url_citation.adoc"))]
#[case::quote_block_with_paragraphs(include_str!("../../../../fixtures/tests/quote_block_with_paragraphs.adoc"))]
#[case::compatibility_quote_locations(include_str!("../../../../fixtures/tests/compatibility_quote_locations.adoc"))]
#[case::compatibility_quote_content(include_str!("../../../../fixtures/tests/compatibility_quote_content.adoc"))]
#[case::compatibility_quote_snapshots(include_str!("../../../../fixtures/tests/compatibility_quote_snapshots.adoc"))]
#[case::unsupported_counters(
    "\"Body {counter:sequence}.\"\n-- Author {counter:sequence}, Citation {counter:sequence}"
)]
#[case::empty_markdown(">\n>\n> -- Author")]
fn abbreviated_quote_driver_preserves_existing_construction(#[case] input: &str) -> TestResult {
    assert_equivalent(input)
}

#[rstest::rstest]
#[case::shorthand("\"probe::inside[]\n\n{value}\"\n-- Author\n\nprobe::outside[]")]
#[case::markdown("> probe::inside[]\n>\n> {value}\n\nprobe::outside[]")]
fn abbreviated_quote_driver_commits_children_in_source_order(#[case] input: &str) -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let arena = bumpalo::Bump::new();
        let mut state = ParserState::new(input, &arena, Rc::new(probe_options(mode)?));
        let mut calls = Vec::new();
        let mut paragraphs = Vec::new();
        document_with(
            input,
            &mut state,
            ConstructionMode::Ordered,
            &mut |block, state| {
                if let Block::Extension(extension) = block {
                    calls.push(extension.invocation.target.to_string());
                    Rc::make_mut(&mut state.document_attributes)
                        .set_text("value".into(), extension.invocation.target.clone());
                }
                if let Block::Paragraph(paragraph) = block {
                    paragraphs.push(
                        paragraph
                            .content
                            .iter()
                            .filter_map(|node| {
                                if let InlineNode::PlainText(text) = node {
                                    Some(text.content)
                                } else {
                                    None
                                }
                            })
                            .collect::<String>(),
                    );
                }
                Ok(())
            },
        )??;
        assert_eq!(calls, ["inside", "outside"]);
        assert_eq!(paragraphs, ["inside"]);
        assert_eq!(state.block_context, BlockContext::Document);
    }
    Ok(())
}

#[rstest::rstest]
#[case::shorthand("\"probe::fail[]\n\nprobe::later[]\"\n-- Author\n\nprobe::outside[]")]
#[case::markdown("> probe::fail[]\n>\n> probe::later[]\n\nprobe::outside[]")]
fn abbreviated_quote_driver_stops_after_handler_failure(#[case] input: &str) -> TestResult {
    let arena = bumpalo::Bump::new();
    let mut state = ParserState::new(
        input,
        &arena,
        Rc::new(probe_options(CompatibilityMode::Asciidoctor)?),
    );
    let mut calls = Vec::new();
    let result = document_with(
        input,
        &mut state,
        ConstructionMode::Ordered,
        &mut |block, state| {
            if let Block::Extension(extension) = block {
                calls.push(extension.invocation.target.to_string());
                return Err(Error::Parse(
                    Box::new(state.create_error_source_location(block.location().clone())),
                    "stop".into(),
                ));
            }
            Ok(())
        },
    )?;
    assert!(result.is_err());
    assert_eq!(calls, ["fail"]);
    assert_eq!(state.block_context, BlockContext::Document);
    Ok(())
}

#[rstest::rstest]
fn abbreviated_quote_driver_preserves_attribution_footnote_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let input =
        "\"Body footnote:[body].\"\n-- Author footnote:[author], Citation footnote:[citation]";
    let arena = bumpalo::Bump::new();
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let mut state = ParserState::new(input, &arena, Rc::new(options));
    let document = document_with(input, &mut state, ConstructionMode::Ordered, &mut |_, _| {
        Ok(())
    })??;
    let Some(Block::DelimitedBlock(block)) = document.blocks.first() else {
        return Err("missing quote".into());
    };
    let numbers = |nodes: &[InlineNode<'_>]| {
        nodes
            .iter()
            .filter_map(|node| {
                if let InlineNode::Macro(crate::InlineMacro::Footnote(note)) = node {
                    Some(note.number)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        block
            .metadata
            .attribution
            .as_ref()
            .map(|content| numbers(content.as_inlines())),
        Some(vec![1])
    );
    assert_eq!(
        block
            .metadata
            .citetitle
            .as_ref()
            .map(|content| numbers(content.as_inlines())),
        Some(vec![2])
    );
    let DelimitedBlockType::DelimitedQuote(children) = &block.inner else {
        return Err("missing quote children".into());
    };
    let Some(Block::Paragraph(paragraph)) = children.first() else {
        return Err("missing quote paragraph".into());
    };
    assert_eq!(numbers(&paragraph.content), [3]);
    Ok(())
}
