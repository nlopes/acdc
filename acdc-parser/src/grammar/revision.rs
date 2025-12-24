use crate::{AttributeValue, DocumentAttributes};

/// Parsed revision information
#[derive(Debug)]
pub(crate) struct RevisionInfo {
    pub number: String,
    pub date: Option<String>,
    pub remark: Option<String>,
}

/// Process revision info and insert into document attributes
pub(crate) fn process_revision_info(
    revision_info: RevisionInfo,
    document_attributes: &mut DocumentAttributes,
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
