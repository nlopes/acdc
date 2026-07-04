use std::path::{Path, PathBuf};

use acdc_parser::ParseResult;

use crate::{
    LintOptions, LintReport,
    rules::{
        self, LintEmitter, attributes, blocks, document, headings, lists, markdown, prose,
        resources, whitespace,
    },
};

pub(crate) fn lint_parsed(
    file: Option<PathBuf>,
    name_path: Option<&Path>,
    source: &str,
    parsed: &ParseResult,
    options: &LintOptions,
) -> LintReport {
    let lines = rules::collect_lines(source);
    let skipped_lines = rules::skipped_delimited_lines(&lines);
    let mut emitter = LintEmitter::new(file, options);
    let document = parsed.document();

    document::lint_parser_warnings(&mut emitter, parsed);
    if let Some(path) = name_path {
        document::lint_document_extension(&mut emitter, path);
    }

    whitespace::lint_source_whitespace(&mut emitter, &lines);
    prose::lint_one_sentence_per_line(&mut emitter, document, &lines);
    headings::lint_section_title_styles(&mut emitter, document, &lines, &skipped_lines);
    headings::lint_section_title_marker_spacing(&mut emitter, &lines, &skipped_lines);
    headings::lint_section_title_capitalization(&mut emitter, document);
    attributes::lint_attribute_url_prefix(&mut emitter, document, &lines);
    lists::lint_adjacent_list_separator(&mut emitter, &lines, &skipped_lines);
    lists::lint_list_marker_spacing(&mut emitter, &lines, &skipped_lines);
    lists::lint_ordered_list_explicit_numbers(&mut emitter, &lines, &skipped_lines);
    lists::lint_description_list_bold_terms(&mut emitter, &lines, &skipped_lines);
    lists::lint_nested_unordered_list_markers(&mut emitter, &document.blocks, 0);
    headings::lint_document_header(&mut emitter, document, &lines);
    document::lint_multiple_document_title(&mut emitter, document, &lines, &skipped_lines);
    blocks::lint_delimited_block_layout(&mut emitter, &lines);
    blocks::lint_blocks(&mut emitter, &document.blocks);
    markdown::lint_markdown_syntax(&mut emitter, &lines, &skipped_lines);
    resources::lint_resources(&mut emitter, document, name_path);

    emitter.finish()
}
