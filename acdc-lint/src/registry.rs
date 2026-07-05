use std::path::Path;

use acdc_parser::{Document, ParseResult};

use crate::{
    LintId, LintOptions,
    rules::{
        self, LintEmitter, SourceLine, attributes, blocks, document, headings, lists, markdown,
        prose, resources, whitespace,
    },
};

/// Shared inputs for a lint run.
///
/// Passes receive this context instead of reparsing or rebuilding common source
/// views. It is intentionally broader than any one pass needs.
pub(crate) struct LintContext<'context, 'source> {
    pub(crate) name_path: Option<&'context Path>,
    pub(crate) parsed: &'context ParseResult,
    pub(crate) lines: &'context [SourceLine<'source>],
    pub(crate) skipped_lines: &'context [bool],
}

impl LintContext<'_, '_> {
    pub(crate) fn document(&self) -> &Document<'_> {
        self.parsed.document()
    }
}

type LintRun = for<'options, 'context, 'source> fn(
    &mut LintEmitter<'options>,
    &LintContext<'context, 'source>,
);

/// Internal executable lint pass.
///
/// A pass is an implementation unit, not a user-visible rule. It owns one
/// checker function and declares the lint IDs that function may emit. The runner
/// uses that lint list to skip the whole pass when all of its lints are
/// configured as `allow`.
///
/// Keep user-facing rule identity and documentation in `LintId`/`LintInfo`.
/// Group lints into a pass when they share a traversal or parsing context.
#[derive(Clone, Copy)]
pub(crate) struct LintPass {
    /// Human-readable pass name for debug/test output.
    name: &'static str,
    /// Lints this pass may emit.
    lints: &'static [LintId],
    /// Checker function executed when any lint in `lints` is enabled.
    run: LintRun,
}

impl LintPass {
    /// Returns whether at least one lint emitted by this pass is enabled.
    pub(crate) fn is_enabled(self, options: &LintOptions) -> bool {
        debug_assert!(!self.name.is_empty());
        self.lints.iter().any(|lint| options.may_emit(*lint))
    }

    /// Executes this pass's checker function.
    pub(crate) fn run(self, emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
        (self.run)(emitter, context);
    }
}

const DOCUMENT_EXTENSION: &[LintId] = &[LintId::DocumentExtension];
const PROSE: &[LintId] = &[LintId::OneSentencePerLine];
const SECTION_TITLE_STYLE: &[LintId] = &[
    LintId::SectionTitleSymmetricMarker,
    LintId::SectionTitleSetextStyle,
];
const DOCUMENT_HEADER: &[LintId] = &[LintId::DocumentTitleAuthor, LintId::DocumentTitleRevision];
const PARSER_WARNINGS: &[LintId] = &[
    LintId::SectionLevelSequence,
    LintId::UnterminatedDelimitedBlock,
    LintId::UnterminatedTable,
    LintId::CounterSyntax,
    LintId::TableUnknownFormat,
    LintId::TableIncompleteRow,
    LintId::TableColumnCount,
    LintId::TableCellOverflow,
];
const MULTIPLE_DOCUMENT_TITLE: &[LintId] = &[LintId::MultipleDocumentTitle];
const DELIMITED_BLOCKS: &[LintId] = &[LintId::DelimitedBlockMinimalDelimiter];
const SECTION_TITLE_MARKER_SPACING: &[LintId] = &[LintId::SectionTitleMarkerSpacing];
const SECTION_TITLE_CAPITALIZATION: &[LintId] = &[
    LintId::SectionTitleCapitalization,
    LintId::SectionTitleCapitalizationMonospace,
];
const DELIMITED_BLOCK_LAYOUT: &[LintId] = &[
    LintId::DelimitedBlockLeadingBlankLine,
    LintId::DelimitedBlockTrailingBlankLine,
];
const SOURCE_WHITESPACE: &[LintId] = &[
    LintId::TrailingWhitespace,
    LintId::HardTab,
    LintId::ExcessiveBlankLines,
];
const LIST_MARKER_SPACING: &[LintId] = &[LintId::ListMarkerSpacing];
const ATTRIBUTE_URL_PREFIX: &[LintId] = &[LintId::AttributeUrlPrefix];
const RESOURCES: &[LintId] = &[
    LintId::Imagesdir,
    LintId::ImageAltText,
    LintId::ImageTargetExists,
];
const NESTED_UNORDERED_LIST_MARKER: &[LintId] = &[LintId::NestedUnorderedListMarker];
const ADJACENT_LIST_SEPARATOR: &[LintId] = &[LintId::AdjacentListSeparator];
const ORDERED_LIST_EXPLICIT_NUMBER: &[LintId] = &[LintId::OrderedListExplicitNumber];
const DESCRIPTION_LIST_BOLD_TERM: &[LintId] = &[LintId::DescriptionListBoldTerm];
const MARKDOWN_SYNTAX: &[LintId] = &[
    LintId::MarkdownHeading,
    LintId::MarkdownLink,
    LintId::MarkdownImage,
    LintId::MarkdownCodeFence,
    LintId::MarkdownTable,
];

pub(crate) const LINT_PASSES: &[LintPass] = &[
    LintPass {
        name: "document-extension",
        lints: DOCUMENT_EXTENSION,
        run: run_document_extension,
    },
    LintPass {
        name: "prose",
        lints: PROSE,
        run: run_prose,
    },
    LintPass {
        name: "section-title-style",
        lints: SECTION_TITLE_STYLE,
        run: run_section_title_style,
    },
    LintPass {
        name: "document-header",
        lints: DOCUMENT_HEADER,
        run: run_document_header,
    },
    LintPass {
        name: "parser-warnings",
        lints: PARSER_WARNINGS,
        run: run_parser_warnings,
    },
    LintPass {
        name: "multiple-document-title",
        lints: MULTIPLE_DOCUMENT_TITLE,
        run: run_multiple_document_title,
    },
    LintPass {
        name: "delimited-blocks",
        lints: DELIMITED_BLOCKS,
        run: run_delimited_blocks,
    },
    LintPass {
        name: "section-title-marker-spacing",
        lints: SECTION_TITLE_MARKER_SPACING,
        run: run_section_title_marker_spacing,
    },
    LintPass {
        name: "section-title-capitalization",
        lints: SECTION_TITLE_CAPITALIZATION,
        run: run_section_title_capitalization,
    },
    LintPass {
        name: "delimited-block-layout",
        lints: DELIMITED_BLOCK_LAYOUT,
        run: run_delimited_block_layout,
    },
    LintPass {
        name: "source-whitespace",
        lints: SOURCE_WHITESPACE,
        run: run_source_whitespace,
    },
    LintPass {
        name: "list-marker-spacing",
        lints: LIST_MARKER_SPACING,
        run: run_list_marker_spacing,
    },
    LintPass {
        name: "attribute-url-prefix",
        lints: ATTRIBUTE_URL_PREFIX,
        run: run_attribute_url_prefix,
    },
    LintPass {
        name: "resources",
        lints: RESOURCES,
        run: run_resources,
    },
    LintPass {
        name: "nested-unordered-list-marker",
        lints: NESTED_UNORDERED_LIST_MARKER,
        run: run_nested_unordered_list_marker,
    },
    LintPass {
        name: "adjacent-list-separator",
        lints: ADJACENT_LIST_SEPARATOR,
        run: run_adjacent_list_separator,
    },
    LintPass {
        name: "ordered-list-explicit-number",
        lints: ORDERED_LIST_EXPLICIT_NUMBER,
        run: run_ordered_list_explicit_number,
    },
    LintPass {
        name: "description-list-bold-term",
        lints: DESCRIPTION_LIST_BOLD_TERM,
        run: run_description_list_bold_term,
    },
    LintPass {
        name: "markdown-syntax",
        lints: MARKDOWN_SYNTAX,
        run: run_markdown_syntax,
    },
];

pub(crate) fn collect_context_lines(source: &str) -> (Vec<SourceLine<'_>>, Vec<bool>) {
    let lines = rules::collect_lines(source);
    let skipped_lines = rules::skipped_delimited_lines(&lines);
    (lines, skipped_lines)
}

fn run_document_extension(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    if let Some(path) = context.name_path {
        document::lint_document_extension(emitter, path);
    }
}

fn run_prose(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    prose::lint_one_sentence_per_line(emitter, context.document(), context.lines);
}

fn run_section_title_style(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    headings::lint_section_title_symmetric_marker(emitter, context.document(), context.lines);
    headings::lint_section_title_setext_style(emitter, context.lines, context.skipped_lines);
}

fn run_document_header(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    headings::lint_document_title_author(emitter, context.document(), context.lines);
    headings::lint_document_title_revision(emitter, context.document(), context.lines);
}

fn run_parser_warnings(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    document::lint_parser_warnings(emitter, context.parsed);
}

fn run_multiple_document_title(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    document::lint_multiple_document_title(
        emitter,
        context.document(),
        context.lines,
        context.skipped_lines,
    );
}

fn run_delimited_blocks(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    blocks::lint_blocks(emitter, &context.document().blocks);
}

fn run_section_title_marker_spacing(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    headings::lint_section_title_marker_spacing(emitter, context.lines, context.skipped_lines);
}

fn run_section_title_capitalization(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    headings::lint_section_title_capitalization(emitter, context.document());
}

fn run_delimited_block_layout(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    blocks::lint_delimited_block_layout(emitter, context.lines);
}

fn run_source_whitespace(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    whitespace::lint_source_whitespace(emitter, context.document(), context.lines);
}

fn run_list_marker_spacing(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    lists::lint_list_marker_spacing(emitter, context.document(), context.lines);
}

fn run_attribute_url_prefix(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    attributes::lint_attribute_url_prefix(emitter, context.document(), context.lines);
}

fn run_resources(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    resources::lint_resources(emitter, context.document(), context.name_path);
}

fn run_nested_unordered_list_marker(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    lists::lint_nested_unordered_list_markers(emitter, &context.document().blocks, 0);
}

fn run_adjacent_list_separator(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    lists::lint_adjacent_list_separator(emitter, context.lines, context.skipped_lines);
}

fn run_ordered_list_explicit_number(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    lists::lint_ordered_list_explicit_numbers(emitter, context.lines, context.skipped_lines);
}

fn run_description_list_bold_term(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    lists::lint_description_list_bold_terms(emitter, context.lines, context.skipped_lines);
}

fn run_markdown_syntax(emitter: &mut LintEmitter<'_>, context: &LintContext<'_, '_>) {
    markdown::lint_markdown_syntax(emitter, context.lines, context.skipped_lines);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_lint_ids_are_unique() {
        let mut seen = Vec::new();
        for info in crate::LINTS {
            assert!(!seen.contains(&info.id));
            seen.push(info.id);
        }
    }

    #[test]
    fn registered_lint_names_are_unique_and_meaningful() {
        let mut seen = Vec::new();
        for info in crate::LINTS {
            assert!(!info.name.is_empty());
            assert!(!info.summary.is_empty());
            assert!(!info.explanation.is_empty());
            assert!(
                info.name
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch == '-')
            );
            assert!(!info.name.starts_with('-'));
            assert!(!info.name.ends_with('-'));
            assert!(!info.name.contains("--"));
            assert!(!seen.contains(&info.name));
            seen.push(info.name);
        }
    }

    #[test]
    fn lint_ids_round_trip_through_names() {
        for info in crate::LINTS {
            assert!(matches!(
                info.name.parse::<LintId>(),
                Ok(lint) if lint == info.id
            ));
            assert_eq!(info.id.name(), info.name);
        }
    }

    #[test]
    fn pass_lints_are_registered() {
        for pass in LINT_PASSES {
            assert!(!pass.name.is_empty());
            assert!(!pass.lints.is_empty());
            for lint in pass.lints {
                assert!(
                    crate::LINTS.iter().any(|info| info.id == *lint),
                    "pass {} references unregistered lint {}",
                    pass.name,
                    lint
                );
            }
        }
    }

    #[test]
    fn registered_lints_are_covered_by_a_pass() {
        for info in crate::LINTS {
            assert!(
                LINT_PASSES.iter().any(|pass| pass.lints.contains(&info.id)),
                "registered lint {} has no pass",
                info.name
            );
        }
    }

    #[test]
    fn pass_lint_lists_do_not_duplicate_lints() {
        let mut seen = Vec::new();
        for pass in LINT_PASSES {
            for lint in pass.lints {
                assert!(!seen.contains(lint));
                seen.push(*lint);
            }
        }
    }
}
