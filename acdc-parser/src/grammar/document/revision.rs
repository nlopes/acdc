//! Revision values and header attributes.

use crate::DocumentAttributes;
use std::borrow::Cow;

/// Parsed revision information
#[derive(Debug)]
pub(super) struct RevisionInfo<'a> {
    pub(super) number: Cow<'a, str>,
    pub(super) date: Option<Cow<'a, str>>,
    pub(super) remark: Option<Cow<'a, str>>,
}

/// Which fields on the revision line were ignored because the
/// corresponding document attribute was already set via an earlier
/// attribute entry. The caller turns each `true` flag into a warning.
#[derive(Debug, Default)]
pub(super) struct IgnoredRevisionFields {
    pub(super) number: bool,
    pub(super) date: bool,
    pub(super) remark: bool,
}

/// Apply revision values and return the fields blocked by earlier attribute entries.
/// The caller reports a warning for each blocked field at the revision line.
pub(super) fn process_revision_info<'a>(
    revision_info: RevisionInfo<'a>,
    document_attributes: &mut DocumentAttributes<'a>,
) -> IgnoredRevisionFields {
    let mut ignored = IgnoredRevisionFields::default();

    if document_attributes.contains_key("revnumber") {
        ignored.number = true;
    } else {
        document_attributes.insert_text("revnumber".into(), revision_info.number);
    }

    if let Some(date) = revision_info.date {
        if document_attributes.contains_key("revdate") {
            ignored.date = true;
        } else {
            document_attributes.insert_text("revdate".into(), date);
        }
    }

    if let Some(remark) = revision_info.remark {
        if document_attributes.contains_key("revremark") {
            ignored.remark = true;
        } else {
            document_attributes.insert_text("revremark".into(), remark);
        }
    }

    ignored
}
