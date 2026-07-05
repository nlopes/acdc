use std::path::{Path, PathBuf};

use acdc_parser::ParseResult;

use crate::{
    LintOptions, LintReport,
    registry::{self, LINT_PASSES, LintContext},
    rules::LintEmitter,
};

pub(crate) fn lint_parsed(
    file: Option<PathBuf>,
    name_path: Option<&Path>,
    source: &str,
    parsed: &ParseResult,
    options: &LintOptions,
) -> LintReport {
    let (lines, skipped_lines) = registry::collect_context_lines(source);
    let context = LintContext {
        name_path,
        parsed,
        lines: &lines,
        skipped_lines: &skipped_lines,
    };
    let mut emitter = LintEmitter::new(file, options);

    for pass in LINT_PASSES {
        if pass.is_enabled(options) {
            pass.run(&mut emitter, &context);
        }
    }

    emitter.finish()
}
