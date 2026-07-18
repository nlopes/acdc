//! Backend traits and their intrinsic document attributes.

use std::borrow::Cow;

use acdc_parser::{AttributeValue, DocumentAttributes};

use crate::Doctype;

/// Intrinsic properties of a converter backend.
///
/// These values mirror Asciidoctor's backend traits. Applying them makes the
/// selected backend visible to preprocessing and attribute substitution through
/// `backend`, `basebackend`, `filetype`, `outfilesuffix`, and their convenience
/// attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackendTraits {
    backend: &'static str,
    basebackend: &'static str,
    filetype: &'static str,
    outfilesuffix: &'static str,
    htmlsyntax: Option<&'static str>,
}

impl BackendTraits {
    /// Create backend traits without an HTML syntax.
    #[must_use]
    pub const fn new(
        backend: &'static str,
        basebackend: &'static str,
        filetype: &'static str,
        outfilesuffix: &'static str,
    ) -> Self {
        Self {
            backend,
            basebackend,
            filetype,
            outfilesuffix,
            htmlsyntax: None,
        }
    }

    /// Declare the HTML syntax exposed by this backend.
    #[must_use]
    pub const fn with_htmlsyntax(mut self, htmlsyntax: &'static str) -> Self {
        self.htmlsyntax = Some(htmlsyntax);
        self
    }

    /// Return the canonical backend name.
    #[must_use]
    pub const fn backend(self) -> &'static str {
        self.backend
    }

    /// Return the generic backend on which this backend is based.
    #[must_use]
    pub const fn basebackend(self) -> &'static str {
        self.basebackend
    }

    /// Return the output file type without its leading period.
    #[must_use]
    pub const fn filetype(self) -> &'static str {
        self.filetype
    }

    /// Return the default output file suffix, including its leading period.
    #[must_use]
    pub const fn outfilesuffix(self) -> &'static str {
        self.outfilesuffix
    }

    /// Return the backend's HTML syntax, when it declares one.
    #[must_use]
    pub const fn htmlsyntax(self) -> Option<&'static str> {
        self.htmlsyntax
    }

    /// Apply the backend's intrinsic attributes for the selected document type.
    ///
    /// Backend identity, base backend, file type, and convenience attributes are
    /// intrinsic and therefore replace conflicting values. An explicit
    /// `outfilesuffix` is retained, matching Asciidoctor's initialization
    /// behavior. A valid `doctype` already present in the map takes precedence
    /// over `default_doctype`.
    ///
    /// Converters apply their traits on construction; parse using the
    /// converter's [`document_attributes`](crate::Converter::document_attributes)
    /// so preprocessing sees the selected backend and the converter's defaults.
    pub fn apply(self, attributes: &mut DocumentAttributes<'_>, default_doctype: Doctype) {
        remove_backend_convenience_attributes(attributes);

        // Read after the removal above: it only clears hyphen-suffixed
        // convenience flags, never the bare `doctype` value.
        let doctype = attributes
            .get_string("doctype")
            .and_then(|value| value.parse::<Doctype>().ok())
            .unwrap_or(default_doctype)
            .as_str();
        set_string(attributes, "backend", self.backend);
        set_flag(attributes, format!("backend-{}", self.backend));
        set_flag(
            attributes,
            format!("backend-{}-doctype-{doctype}", self.backend),
        );

        set_string(attributes, "basebackend", self.basebackend);
        set_flag(attributes, format!("basebackend-{}", self.basebackend));
        set_flag(
            attributes,
            format!("basebackend-{}-doctype-{doctype}", self.basebackend),
        );

        set_string(attributes, "doctype", doctype);
        set_flag(attributes, format!("doctype-{doctype}"));

        set_string(attributes, "filetype", self.filetype);
        set_flag(attributes, format!("filetype-{}", self.filetype));

        if matches!(
            attributes.get("outfilesuffix"),
            None | Some(AttributeValue::None | AttributeValue::Bool(false))
        ) {
            set_string(attributes, "outfilesuffix", self.outfilesuffix);
        }

        if let Some(htmlsyntax) = self.htmlsyntax {
            set_string(attributes, "htmlsyntax", htmlsyntax);
        }
    }
}

fn remove_backend_convenience_attributes(attributes: &mut DocumentAttributes<'_>) {
    const PREFIXES: [&str; 4] = ["backend-", "basebackend-", "doctype-", "filetype-"];
    let names: Vec<_> = attributes
        .iter()
        .filter(|(name, _)| PREFIXES.iter().any(|prefix| name.starts_with(prefix)))
        .map(|(name, _)| name.clone())
        .collect();
    for name in names {
        attributes.remove(name.as_ref());
    }
}

fn set_string(attributes: &mut DocumentAttributes<'_>, name: &'static str, value: &'static str) {
    attributes.set(
        Cow::Borrowed(name),
        AttributeValue::String(Cow::Borrowed(value)),
    );
}

fn set_flag(attributes: &mut DocumentAttributes<'_>, name: String) {
    attributes.set(Cow::Owned(name), AttributeValue::String(Cow::Borrowed("")));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_all_backend_intrinsic_attributes() {
        let mut attributes = DocumentAttributes::default();
        let traits = BackendTraits::new("pdf", "html", "pdf", ".pdf").with_htmlsyntax("html");

        traits.apply(&mut attributes, Doctype::Book);

        assert_eq!(attributes.get_string("backend").as_deref(), Some("pdf"));
        assert_eq!(
            attributes.get_string("basebackend").as_deref(),
            Some("html")
        );
        assert_eq!(attributes.get_string("doctype").as_deref(), Some("book"));
        assert_eq!(attributes.get_string("filetype").as_deref(), Some("pdf"));
        assert_eq!(
            attributes.get_string("outfilesuffix").as_deref(),
            Some(".pdf")
        );
        assert_eq!(attributes.get_string("htmlsyntax").as_deref(), Some("html"));
        for name in [
            "backend-pdf",
            "backend-pdf-doctype-book",
            "basebackend-html",
            "basebackend-html-doctype-book",
            "doctype-book",
            "filetype-pdf",
        ] {
            assert_eq!(attributes.get_string(name).as_deref(), Some(""), "{name}");
        }
    }

    #[test]
    fn replaces_stale_traits_but_preserves_explicit_output_suffix() {
        let mut attributes = DocumentAttributes::default();
        BackendTraits::new("html5", "html", "html", ".html")
            .with_htmlsyntax("html")
            .apply(&mut attributes, Doctype::Article);
        attributes.set("outfilesuffix".into(), ".custom".into());
        attributes.set("doctype".into(), "book".into());

        BackendTraits::new("pdf", "html", "pdf", ".pdf")
            .with_htmlsyntax("html")
            .apply(&mut attributes, Doctype::Book);

        assert!(!attributes.contains_key("backend-html5"));
        assert!(!attributes.contains_key("backend-html5-doctype-article"));
        assert!(!attributes.contains_key("basebackend-html-doctype-article"));
        assert!(!attributes.contains_key("doctype-article"));
        assert!(!attributes.contains_key("filetype-html"));
        assert_eq!(
            attributes.get_string("outfilesuffix").as_deref(),
            Some(".custom")
        );
    }
}
