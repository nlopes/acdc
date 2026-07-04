use acdc_parser::WarningKind;

use crate::LintId;

pub(crate) fn lint_for_parser_warning(kind: &WarningKind) -> Option<LintId> {
    match kind {
        WarningKind::TableUnknownFormat { .. } => Some(LintId::TableUnknownFormat),
        WarningKind::TableIncompleteRow => Some(LintId::TableIncompleteRow),
        WarningKind::TableColumnCount { .. } => Some(LintId::TableColumnCount),
        WarningKind::TableCellOverflow { .. } => Some(LintId::TableCellOverflow),
        WarningKind::SectionLevelOutOfSequence { .. }
        | WarningKind::UnterminatedTable { .. }
        | WarningKind::UnterminatedDelimitedBlock { .. }
        | WarningKind::NonStandardAuthorLine { .. }
        | WarningKind::UnresolvedReference { .. }
        | WarningKind::LegacyFloatDiscreteHeading
        | WarningKind::Other(_)
        | _ => None,
    }
}
