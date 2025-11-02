use clap::ValueEnum;

/// Document type to use when converting document.
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum Doctype {
    /// The default doctype. In `DocBook`, this includes the appendix, abstract,
    /// bibliography, glossary, and index sections. Unless you are making a book or a man
    /// page, you don’t need to worry about the doctype. The default will suffice.
    #[default]
    Article,

    /// Builds on the article doctype with the additional ability to use a top-level title
    /// as part titles, includes the appendix, dedication, preface, bibliography,
    /// glossary, index, and colophon. There’s also the concept of a multi-part book, but
    /// the distinction from a regular book is determined by the content. A book only has
    /// chapters and special sections, whereas a multi-part book is divided by parts that
    /// each contain one or more chapters or special sections.
    Book,

    /// Used for producing a roff or HTML-formatted manual page (man page) for Unix and
    /// Unix-like operating systems. This doctype instructs the parser to recognize a
    /// special document header and section naming conventions for organizing the
    /// `AsciiDoc` content as a man page. See Generate Manual Pages from `AsciiDoc` for
    /// details on how structure a man page using `AsciiDoc` and generate it using
    /// Asciidoctor.
    Manpage,

    /// There may be cases when you only want to apply inline `AsciiDoc` formatting to input
    /// text without wrapping it in a block element. For example, in the Asciidoclet
    /// project (`AsciiDoc` in Javadoc), only the inline formatting is needed for the text
    /// in Javadoc tags.
    Inline,
}

impl std::fmt::Display for Doctype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Doctype::Article => write!(f, "article"),
            Doctype::Book => write!(f, "book"),
            Doctype::Manpage => write!(f, "manpage"),
            Doctype::Inline => write!(f, "inline"),
        }
    }
}

/// Safe mode to use when processing the document. This follows from what is described in
/// <https://docs.asciidoctor.org/asciidoctor/latest/safe-modes/> and is intended to
/// provide similar functionality as Asciidoctor.
#[derive(Debug, Clone, ValueEnum, Default, PartialOrd, PartialEq)]
pub enum SafeMode {
    /// The `UNSAFE` safe mode level disables all security measures.
    #[default]
    Unsafe = 0,

    /// The `SAFE` safe mode level prevents access to files which reside outside of the
    /// parent directory of the source file. Include directives (`include::[]`) are
    /// enabled, but paths to include files must be within the parent directory. This mode
    /// allows assets (such as the stylesheet) to be embedded in the document.
    Safe,

    /// The `SERVER` safe mode level disallows the document from setting attributes that
    /// would affect conversion of the document. This level trims docfile to its relative
    /// path and prevents the document from:
    ///
    /// - setting source-highlighter, doctype, docinfo and backend
    /// - seeing docdir (as it can reveal information about the host filesystem)
    ///
    /// It allows icons and linkcss. No includes from a url are allowed unless the
    /// `allow-uri-read` attribute is set.
    Server,

    /// The `SECURE` safe mode level disallows the document from attempting to read files
    /// from the file system and including their contents into the document. Additionally,
    /// it:
    ///
    /// - disables icons
    /// - disables include directives (`include::[]`)
    /// - data can not be retrieved from URIs
    /// - prevents access to stylesheets and JavaScript files
    /// - sets the backend to html5
    /// - disables docinfo files
    /// - disables data-uri
    /// - disables interactive (opts=interactive) and inline (opts=inline) modes for SVGs
    /// - disables docdir and docfile (as these can reveal information about the host
    ///   filesystem)
    /// - disables source highlighting
    ///
    /// Note: `GitHub` processes `AsciiDoc` files using the `SECURE` mode.
    Secure,
}
