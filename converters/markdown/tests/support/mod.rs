use std::path::{Path, PathBuf};

use acdc_parser::{Warning, WarningKind};

pub(crate) fn expected_fixture_path(directory: &Path, stem: &str, warnings: &[Warning]) -> PathBuf {
    // Cargo may enable parser substitutions through another workspace member.
    // Use the parser's diagnostic rather than a Markdown-local feature flag.
    let ignored = warnings.iter().any(|warning| {
        matches!(&warning.kind, WarningKind::ContentRecovery { message }
            if message.starts_with("The subs= attribute is not honoured in this build"))
    });
    let extension = if ignored { "no-subs.md" } else { "md" };
    directory.join(format!("{stem}.{extension}"))
}
