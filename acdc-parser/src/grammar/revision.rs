use crate::{AttributeValue, DocumentAttributes};
use std::borrow::Cow;

/// Parsed revision information
#[derive(Debug)]
pub(crate) struct RevisionInfo<'a> {
    pub number: Cow<'a, str>,
    pub date: Option<Cow<'a, str>>,
    pub remark: Option<Cow<'a, str>>,
}

/// Process revision info and insert into document attributes
pub(crate) fn process_revision_info<'a>(
    revision_info: RevisionInfo<'a>,
    document_attributes: &mut DocumentAttributes<'a>,
) {
    if document_attributes.contains_key("revnumber") {
        tracing::warn!(
            "Revision number found in revision line but ignoring due to being set through attribute entries."
        );
    } else {
        document_attributes.insert(
            "revnumber".into(),
            AttributeValue::String(revision_info.number),
        );
    }

    if let Some(date) = revision_info.date {
        if document_attributes.contains_key("revdate") {
            tracing::warn!(
                "Revision date found in revision line but ignoring due to being set through attribute entries."
            );
        } else {
            document_attributes.insert("revdate".into(), AttributeValue::String(date));
        }
    }

    if let Some(remark) = revision_info.remark {
        if document_attributes.contains_key("revremark") {
            tracing::warn!(
                "Revision remark found in revision line but ignoring due to being set through attribute entries."
            );
        } else {
            document_attributes.insert("revremark".into(), AttributeValue::String(remark));
        }
    }
}
