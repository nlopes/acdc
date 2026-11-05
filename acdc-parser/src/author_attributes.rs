//! Derived author attributes share the source parser's name projection.

use std::borrow::Cow;

use crate::{AttributeName, Author, Document, DocumentAttributes};

impl Document<'_> {
    /// Refresh author-derived attributes from the prepared header authors.
    ///
    /// Caller defaults, locked inputs and document assignments (including unsets)
    /// are retained. Removed authors lose their generated fields. Prepared names
    /// and initials are used as supplied; body declarations, selected content and
    /// other catalogs are unchanged.
    pub fn rebuild_author_attributes(&mut self) {
        self.attributes
            .remove_derived_attributes(is_author_attribute);
        let authors = self
            .header
            .as_ref()
            .map_or(&[][..], |header| header.authors.as_slice());
        if !authors.is_empty() && self.attributes.assignment("authorcount").is_none() {
            self.attributes
                .set_text("authorcount".into(), authors.len().to_string().into());
        }
        write_author_attrs(authors, &mut self.attributes, false);
    }
}

fn is_author_attribute(name: &str) -> bool {
    if matches!(name, "authors" | "authorcount") {
        return true;
    }
    let field = if let Some((field, suffix)) = name.rsplit_once('_') {
        if suffix.starts_with('0') || !suffix.parse::<usize>().is_ok_and(|number| number >= 2) {
            return false;
        }
        field
    } else {
        name
    };
    matches!(
        field,
        "author" | "firstname" | "middlename" | "lastname" | "authorinitials" | "email"
    )
}

pub(crate) fn set_author_attrs<'a>(
    authors: &[Author<'a>],
    attrs: &mut DocumentAttributes<'a>,
    refresh: bool,
) {
    if authors.is_empty() {
        return;
    }
    // Source parsing derives the actual count even when an input supplied one.
    attrs.set_text("authorcount".into(), authors.len().to_string().into());
    write_author_attrs(authors, attrs, refresh);
}

pub(crate) fn build_author_full_name(author: &Author) -> String {
    let mut name = author.first_name.to_string();
    if let Some(middle) = &author.middle_name {
        name.push(' ');
        name.push_str(middle);
    }
    if !author.last_name.is_empty() {
        name.push(' ');
        name.push_str(author.last_name);
    }
    name
}

fn write_author_attrs<'a>(
    authors: &[Author<'a>],
    attrs: &mut DocumentAttributes<'a>,
    refresh: bool,
) {
    if authors.is_empty() {
        return;
    }
    let all_names: Vec<String> = authors.iter().map(build_author_full_name).collect();
    // First registration retains existing defaults. Rebuilding an overridden
    // author refreshes derived name fields while preserving explicit fields.
    let mut assign = |name: AttributeName<'a>, value: Cow<'a, str>| {
        if refresh {
            attrs.set_derived_text(name, value);
        } else {
            attrs.insert_text(name, value);
        }
    };
    assign("authors".into(), all_names.join(", ").into());
    for (i, author) in authors.iter().enumerate() {
        let suffix = if i == 0 {
            String::new()
        } else {
            format!("_{}", i + 1)
        };
        assign(
            format!("author{suffix}").into(),
            build_author_full_name(author).into(),
        );
        assign(
            format!("firstname{suffix}").into(),
            Cow::Borrowed(author.first_name),
        );
        if let Some(middle) = author.middle_name {
            assign(format!("middlename{suffix}").into(), Cow::Borrowed(middle));
        }
        assign(
            format!("lastname{suffix}").into(),
            Cow::Borrowed(author.last_name),
        );
        assign(
            format!("authorinitials{suffix}").into(),
            Cow::Borrowed(author.initials),
        );
        if let Some(email) = author.email {
            assign(format!("email{suffix}").into(), Cow::Borrowed(email));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DocumentAttributes, is_author_attribute};

    #[test]
    fn derived_cleanup_retains_unrelated_fields_and_noncanonical_suffixes() {
        let mut attributes = DocumentAttributes::default();
        for name in [
            "author",
            "author_2",
            "email_12",
            "author_02",
            "author_1",
            "authorcount_2",
            "authors_2",
            "author-extra",
            "revnumber",
        ] {
            attributes.set_text(name.into(), "prepared".into());
        }
        attributes.remove_derived_attributes(is_author_attribute);
        for name in ["author", "author_2", "email_12"] {
            assert!(!attributes.is_explicit(name));
            assert_eq!(attributes.get(name), None);
            assert!(attributes.iter().all(|(key, _)| key != name));
        }
        for name in [
            "author_02",
            "author_1",
            "authorcount_2",
            "authors_2",
            "author-extra",
            "revnumber",
        ] {
            assert_eq!(
                attributes
                    .get(name)
                    .and_then(crate::DocumentAttributeValue::text),
                Some("prepared")
            );
        }
    }
}
