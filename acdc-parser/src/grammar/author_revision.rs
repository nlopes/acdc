use crate::{AttributeValue, DocumentAttributes};

/// Generate initials from first, optional middle, and last name parts
pub(crate) fn generate_initials(first: &str, middle: Option<&str>, last: &str) -> String {
    let first_initial = first.chars().next().unwrap_or_default().to_string();
    let middle_initial = middle
        .map(|m| m.chars().next().unwrap_or_default().to_string())
        .unwrap_or_default();
    let last_initial = last.chars().next().unwrap_or_default().to_string();
    first_initial + &middle_initial + &last_initial
}

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
