pub(crate) mod attributes;
pub(crate) mod blocks;
pub(crate) mod document;
pub(crate) mod headings;
pub(crate) mod lists;
pub(crate) mod markdown;
pub(crate) mod prose;
pub(crate) mod resources;
pub(crate) mod source;
pub(crate) mod tables;
pub(crate) mod whitespace;

pub(crate) use source::*;

#[cfg(test)]
pub(super) mod test_support {
    use crate::{Error, LintId, LintOptions, LintReport, Lintable};

    pub(super) fn report_for(source: &str) -> Result<LintReport, Error> {
        source.lint(&LintOptions::default())
    }

    pub(super) fn has_lint(report: &LintReport, lint: LintId) -> bool {
        report
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.lint() == lint)
    }
}
