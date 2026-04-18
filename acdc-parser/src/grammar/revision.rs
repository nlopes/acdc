use crate::{AttributeValue, DocumentAttributes};
use std::borrow::Cow;

/// Parsed revision information
#[derive(Debug)]
pub(crate) struct RevisionInfo<'a> {
    pub number: Cow<'a, str>,
    pub date: Option<Cow<'a, str>>,
    pub remark: Option<Cow<'a, str>>,
}

/// Which fields on the revision line were ignored because the
/// corresponding document attribute was already set via an earlier
/// attribute entry. The caller turns each `true` flag into a warning.
#[derive(Debug, Default)]
pub(crate) struct IgnoredRevisionFields {
    pub number: bool,
    pub date: bool,
    pub remark: bool,
}

/// Process revision info and insert into document attributes, returning
/// the set of fields that were ignored because they were already set via
/// attribute entries. The caller is responsible for surfacing warnings
/// for those fields (it has the revision-line location and access to the
/// shared warning sink).
pub(crate) fn process_revision_info<'a>(
    revision_info: RevisionInfo<'a>,
    document_attributes: &mut DocumentAttributes<'a>,
) -> IgnoredRevisionFields {
    let mut ignored = IgnoredRevisionFields::default();

    if document_attributes.contains_key("revnumber") {
        ignored.number = true;
    } else {
        document_attributes.insert(
            "revnumber".into(),
            AttributeValue::String(revision_info.number),
        );
    }

    if let Some(date) = revision_info.date {
        if document_attributes.contains_key("revdate") {
            ignored.date = true;
        } else {
            document_attributes.insert("revdate".into(), AttributeValue::String(date));
        }
    }

    if let Some(remark) = revision_info.remark {
        if document_attributes.contains_key("revremark") {
            ignored.remark = true;
        } else {
            document_attributes.insert("revremark".into(), AttributeValue::String(remark));
        }
    }

    ignored
}
