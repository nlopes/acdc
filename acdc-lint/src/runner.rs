use std::path::{Path, PathBuf};

use acdc_parser::ParseResult;

use crate::{
    LintOptions, LintReport,
    rules::{self, LintEmitter, attributes, headings, prose, structure},
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

    if let Some(path) = name_path {
        structure::lint_document_extension(&mut emitter, path);
    }

    prose::lint_one_sentence_per_line(&mut emitter, document, &lines);
    headings::lint_section_title_style(&mut emitter, document, &lines, &skipped_lines);
    attributes::lint_attribute_url_prefix(&mut emitter, document, &lines);
    structure::lint_adjacent_list_separator(&mut emitter, &lines, &skipped_lines);
    headings::lint_document_header(&mut emitter, document, &lines);
    structure::lint_blocks(&mut emitter, &document.blocks, 0);

    emitter.finish()
}
