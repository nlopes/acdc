//! Document type configuration for `AsciiDoc` conversion.
//!
//! The doctype determines how the document structure is interpreted and rendered.
//! Most documents use the default `Article` doctype.

use std::str::FromStr;

/// Document type to use when converting document.
///
/// The doctype affects how the document structure is interpreted:
/// - Section numbering and hierarchy
/// - Special sections (appendix, bibliography, etc.)
/// - Output format (e.g., manpage requires specific structure)
#[derive(Debug, Clone, Default, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Doctype {
    /// The default doctype. In `DocBook`, this includes the appendix, abstract,
    /// bibliography, glossary, and index sections. Unless you are making a book or a man
    /// page, you don't need to worry about the doctype. The default will suffice.
    #[default]
    Article,

    /// Builds on the article doctype with the additional ability to use a top-level title
    /// as part titles, includes the appendix, dedication, preface, bibliography,
    /// glossary, index, and colophon. There's also the concept of a multi-part book, but
    /// the distinction from a regular book is determined by the content. A book only has
    /// chapters and special sections, whereas a multi-part book is divided by parts that
    /// each contain one or more chapters or special sections.
    Book,

    /// Used for producing a groff manpage for Unix and Unix-like operating systems. This
    /// doctype instructs the parser to recognize a special document header and section
    /// naming conventions for organizing the `AsciiDoc` content as a man page. See
    /// Generate Manual Pages from `AsciiDoc` for details on how structure a man page
    /// using `AsciiDoc` and generate it using Asciidoctor.
    Manpage,

    /// There may be cases when you only want to apply inline `AsciiDoc` formatting to input
    /// text without wrapping it in a block element. For example, in the Asciidoclet
    /// project (`AsciiDoc` in Javadoc), only the inline formatting is needed for the text
    /// in Javadoc tags.
    Inline,
}

impl FromStr for Doctype {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "article" => Ok(Self::Article),
            "book" => Ok(Self::Book),
            "manpage" => Ok(Self::Manpage),
            "inline" => Ok(Self::Inline),
            _ => Err(format!(
                "invalid doctype: '{s}', expected: article, book, manpage, inline"
            )),
        }
    }
}

impl std::fmt::Display for Doctype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Article => write!(f, "article"),
            Self::Book => write!(f, "book"),
            Self::Manpage => write!(f, "manpage"),
            Self::Inline => write!(f, "inline"),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str() {
        assert_eq!(Doctype::from_str("article").unwrap(), Doctype::Article);
        assert_eq!(Doctype::from_str("ARTICLE").unwrap(), Doctype::Article);
        assert_eq!(Doctype::from_str("book").unwrap(), Doctype::Book);
        assert_eq!(Doctype::from_str("manpage").unwrap(), Doctype::Manpage);
        assert_eq!(Doctype::from_str("inline").unwrap(), Doctype::Inline);
        assert!(Doctype::from_str("invalid").is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(Doctype::Article.to_string(), "article");
        assert_eq!(Doctype::Book.to_string(), "book");
        assert_eq!(Doctype::Manpage.to_string(), "manpage");
        assert_eq!(Doctype::Inline.to_string(), "inline");
    }

    #[test]
    fn test_default() {
        assert_eq!(Doctype::default(), Doctype::Article);
    }
}
