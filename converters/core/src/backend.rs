//! Backend profiles and their intrinsic document attributes.

use acdc_parser::AttributeValue;

use crate::Doctype;

/// Intrinsic properties of a converter backend.
///
/// These values mirror Asciidoctor's backend traits. Applying them makes the
/// selected backend visible to preprocessing and attribute substitution through
/// `backend`, `basebackend`, `filetype`, `outfilesuffix`, and their convenience
/// attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackendProfile {
    backend: &'static str,
    basebackend: &'static str,
    filetype: &'static str,
    outfilesuffix: &'static str,
    htmlsyntax: Option<&'static str>,
}

impl BackendProfile {
    /// Create a backend profile without an HTML syntax.
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
    /// behavior. A valid supplied `doctype` takes precedence
    /// over `default_doctype`.
    ///
    /// Converters apply their profile on construction; parse using the
    /// converter's [`parser_options`](crate::Converter::parser_options)
    /// so preprocessing sees the selected backend and the converter's defaults.
    #[must_use]
    pub fn apply(
        self,
        mut builder: acdc_parser::OptionsBuilder<'_>,
        default_doctype: Doctype,
        embedded: bool,
    ) -> acdc_parser::OptionsBuilder<'_> {
        let doctype = builder
            .attribute("doctype")
            .and_then(|value| match value {
                AttributeValue::String(value) => value.parse::<Doctype>().ok(),
                AttributeValue::Bool(_) | AttributeValue::None | _ => None,
            })
            .unwrap_or(default_doctype)
            .as_str();
        builder = builder
            .with_default_attribute("backend", self.backend)
            .with_default_attribute("basebackend", self.basebackend)
            .with_default_attribute("filetype", self.filetype)
            .with_default_attribute("doctype", doctype)
            .with_default_attribute("embedded", embedded);
        if builder.attribute("outfilesuffix").is_none() {
            builder = builder.with_default_attribute("outfilesuffix", self.outfilesuffix);
        }
        builder.with_default_attribute(
            "htmlsyntax",
            self.htmlsyntax
                .map_or(AttributeValue::None, AttributeValue::from),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::{Error, Options};

    #[test]
    fn applies_all_backend_intrinsic_attributes() -> Result<(), Error> {
        let profile = BackendProfile::new("pdf", "html", "pdf", ".pdf").with_htmlsyntax("html");

        let options = profile
            .apply(Options::builder(), Doctype::Book, true)
            .build()?;
        let attributes = options.document_attributes();

        assert_eq!(
            attributes.get("backend").and_then(|value| value.text()),
            Some("pdf")
        );
        assert_eq!(
            attributes.get("basebackend").and_then(|value| value.text()),
            Some("html")
        );
        assert_eq!(
            attributes.get("doctype").and_then(|value| value.text()),
            Some("book")
        );
        assert_eq!(
            attributes.get("filetype").and_then(|value| value.text()),
            Some("pdf")
        );
        assert_eq!(
            attributes
                .get("outfilesuffix")
                .and_then(|value| value.text()),
            Some(".pdf")
        );
        assert_eq!(
            attributes.get("htmlsyntax").and_then(|value| value.text()),
            Some("html")
        );
        for name in [
            "backend-pdf",
            "backend-pdf-doctype-book",
            "basebackend-html",
            "basebackend-html-doctype-book",
            "doctype-book",
            "filetype-pdf",
            "embedded",
        ] {
            assert!(attributes.contains_key(name), "{name}");
        }
        Ok(())
    }

    #[test]
    fn replaces_stale_backend_attributes_but_preserves_explicit_output_suffix() -> Result<(), Error>
    {
        let builder = BackendProfile::new("html5", "html", "html", ".html")
            .with_htmlsyntax("html")
            .apply(Options::builder(), Doctype::Article, false)
            .build()?
            .into_builder()
            .with_attribute("outfilesuffix", ".custom")
            .with_attribute("doctype", "book");

        let options = BackendProfile::new("pdf", "html", "pdf", ".pdf")
            .with_htmlsyntax("html")
            .apply(builder, Doctype::Book, false)
            .build()?;
        let attributes = options.document_attributes();

        assert!(!attributes.contains_key("backend-html5"));
        assert!(!attributes.contains_key("backend-html5-doctype-article"));
        assert!(!attributes.contains_key("basebackend-html-doctype-article"));
        assert!(!attributes.contains_key("doctype-article"));
        assert!(!attributes.contains_key("filetype-html"));
        assert!(!attributes.contains_key("embedded"));
        assert_eq!(
            attributes
                .get("outfilesuffix")
                .and_then(|value| value.text()),
            Some(".custom")
        );
        Ok(())
    }

    #[test]
    fn selected_backend_replaces_a_conflicting_caller_value_when_maps_merge()
    -> Result<(), Box<dyn std::error::Error>> {
        let parser_options = BackendProfile::new("pdf", "html", "pdf", ".pdf")
            .apply(
                Options::builder().with_attribute("backend", "spoofed"),
                Doctype::Article,
                false,
            )
            .build()?;

        let parsed = acdc_parser::parse(":backend: also-spoofed\n", &parser_options)?;

        assert_eq!(
            parsed
                .document()
                .attributes
                .get("backend")
                .and_then(|value| value.text()),
            Some("pdf")
        );
        assert!(parsed.document().attributes.contains_key("backend-pdf"));
        assert!(!parsed.document().attributes.contains_key("backend-spoofed"));
        Ok(())
    }

    #[test]
    fn switching_to_a_backend_without_htmlsyntax_clears_the_previous_syntax() -> Result<(), Error> {
        let html = BackendProfile::new("html5", "html", "html", ".html")
            .with_htmlsyntax("html")
            .apply(Options::builder(), Doctype::Article, true)
            .build()?;
        let options = BackendProfile::new("manpage", "manpage", "man", ".man")
            .apply(html.into_builder(), Doctype::Manpage, false)
            .build()?;
        let attributes = options.document_attributes();
        assert!(!attributes.contains_key("htmlsyntax"));
        assert!(!attributes.contains_key("embedded"));
        assert!(!attributes.contains_key("backend-html5"));
        assert!(attributes.contains_key("backend-manpage"));
        Ok(())
    }
}
