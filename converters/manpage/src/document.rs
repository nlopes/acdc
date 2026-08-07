//! Document-level rendering for manpages.
//!
//! Handles the `.TH` title header macro and document preamble.

use std::{borrow::Cow, collections::HashMap, io::Write, rc::Rc};

use acdc_converters_core::{InlineTextTransform, visitor::WritableVisitor};
use acdc_parser::{Author, Document, InlineNode, Reference};

use crate::{Error, ManpageVisitor, escape::escape_quoted};

/// The version of the acdc package (from Cargo.toml).
const VERSION: &str = env!("CARGO_PKG_VERSION");

const SEE_THE_AUTHOR_SECTION: &str = r#"[see the "AUTHOR(S)" section]"#;

/// Format an author's full name for display.
pub(crate) fn format_author_name(author: &Author) -> String {
    match &author.middle_name {
        Some(middle) => format!("{} {middle} {}", author.first_name, author.last_name),
        None => format!("{} {}", author.first_name, author.last_name),
    }
}

/// Format the author line for the comment header.
///
/// Returns all author names comma-separated if available, otherwise a
/// reference to the AUTHOR(S) section.
fn format_author_line(authors: &[Author]) -> String {
    if authors.is_empty() {
        SEE_THE_AUTHOR_SECTION.to_string()
    } else {
        authors
            .iter()
            .map(format_author_name)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Write a comment line with a right-aligned label (12 chars total width).
fn write_comment_line<W: Write + ?Sized>(
    w: &mut W,
    label: &str,
    value: &str,
) -> std::io::Result<()> {
    // Labels are right-aligned to 11 characters (including the colon)
    // This matches asciidoctor's comment header alignment
    writeln!(w, r#".\"{:>11} {value}"#, format!("{label}:"))
}

/// Write URL and MTO macro definitions for link handling.
fn write_url_macros<W: Write + ?Sized>(w: &mut W, linkstyle: &str) -> std::io::Result<()> {
    writeln!(w, ".de URL")?;
    writeln!(w, r"\fI\\$2\fP <\\$1>\\$3")?;
    writeln!(w, "..")?;
    writeln!(w, ".als MTO URL")?;
    writeln!(w, ".if \\n[.g] \\{{\\")?;
    writeln!(w, ".  mso www.tmac")?;
    writeln!(w, ".  am URL")?;
    writeln!(w, ".    ad l")?;
    writeln!(w, ".  .")?;
    writeln!(w, ".  am MTO")?;
    writeln!(w, ".    ad l")?;
    writeln!(w, ".  .")?;
    writeln!(w, ".  LINKSTYLE {linkstyle}")?;
    writeln!(w, r".\}}")?;
    Ok(())
}

/// Plain text for verbatim block content.
///
/// Hard line breaks are newlines, and numeric character references decode to
/// characters because roff output carries no HTML entities.
pub(crate) fn extract_verbatim_text(nodes: &[InlineNode]) -> String {
    InlineTextTransform::default()
        .line_break("\n")
        .decode_char_refs(true)
        .to_string(nodes)
}

/// Plain text for a roff line that cannot carry markup, such as `.SH` or `.TH`.
///
/// Every node contributes its text: a link contributes its link text, and a
/// cross-reference contributes its target's reference text, matching
/// `asciidoctor`.
pub(crate) fn extract_heading_text(
    nodes: &[InlineNode],
    references: &HashMap<&str, Reference<'_>>,
) -> String {
    InlineTextTransform::default()
        .decode_char_refs(true)
        .references(references)
        .to_string(nodes)
}

impl<W: Write> ManpageVisitor<'_, '_, W> {
    /// Visit document start - generates the .TH header and preamble.
    ///
    /// Reads manpage attributes that were derived by the parser:
    /// - `mantitle`: The program name from the document title
    /// - `manvolnum`: The volume number from the document title
    /// - `manname`: From NAME section (or falls back to mantitle)
    /// - `manpurpose`: From NAME section (after ` - `)
    /// - `_manpage_title_conforming`: Whether the title conforms to name(volume) format
    pub(crate) fn render_document_start(&mut self, doc: &Document) -> Result<(), Error> {
        // In embedded mode, skip the entire preamble (comment block, .TH, macros, settings)
        // This matches asciidoctor's --embedded behavior for manpages
        if self.processor.options.embedded() {
            return Ok(());
        }

        // Ensure we have a header
        if doc.header.is_none() {
            return Err(Error::MissingHeader);
        }

        let mantitle = doc
            .attributes
            .get_string("mantitle")
            .ok_or_else(|| Error::InvalidManpageTitle("missing mantitle attribute".to_string()))?;
        let manvolnum = doc
            .attributes
            .get_string("manvolnum")
            .unwrap_or(Cow::Borrowed("1"));

        let mansource = doc
            .attributes
            .get_string("mansource")
            .or_else(|| doc.attributes.get_string("man source"))
            .or_else(|| doc.attributes.get_string("man-source"))
            .unwrap_or_default();
        let manmanual = doc
            .attributes
            .get_string("manmanual")
            .or_else(|| doc.attributes.get_string("man manual"))
            .or_else(|| doc.attributes.get_string("man-manual"))
            .unwrap_or_default();
        let date = doc
            .attributes
            .get_string("revdate")
            .or_else(|| self.processor.document_attributes.get_string("revdate"))
            .unwrap_or_else(|| Cow::Owned(chrono::Local::now().format("%Y-%m-%d").to_string()));

        self.write_preamble_header(doc, &mantitle, &manvolnum, &date, &mansource, &manmanual)?;

        Ok(())
    }

    /// Write the full roff preamble: comment block, .TH, settings, and URL macros.
    fn write_preamble_header(
        &mut self,
        doc: &Document,
        mantitle: &str,
        manvolnum: &str,
        date: &str,
        mansource: &str,
        manmanual: &str,
    ) -> Result<(), Error> {
        let references = Rc::clone(&self.processor.references);
        let w = self.writer_mut();
        writeln!(w, r#"'\" t"#)?;

        let title_for_comment = doc.header.as_ref().map_or_else(
            || mantitle.to_string(),
            |h| {
                let full_title = extract_heading_text(&h.title, &references);
                full_title
                    .rsplit_once('(')
                    .filter(|(_, vol)| vol.ends_with(')') && vol.len() <= 3)
                    .map_or(full_title.clone(), |(name, _)| name.to_string())
            },
        );
        let author_line = doc.header.as_ref().map_or_else(
            || SEE_THE_AUTHOR_SECTION.to_string(),
            |h| format_author_line(&h.authors),
        );
        let manual_display = if manmanual.is_empty() {
            r"\ \&".to_string()
        } else {
            manmanual.to_string()
        };
        let source_display = if mansource.is_empty() {
            r"\ \&".to_string()
        } else {
            mansource.to_string()
        };

        write_comment_line(w, "Title", &title_for_comment)?;
        write_comment_line(w, "Author", &author_line)?;
        write_comment_line(w, "Generator", &format!("acdc {VERSION}"))?;
        write_comment_line(w, "Date", date)?;
        write_comment_line(w, "Manual", &manual_display)?;
        write_comment_line(w, "Source", &source_display)?;
        write_comment_line(w, "Language", "English")?;
        writeln!(w, r#".\""#)?;

        let th_source = if mansource.is_empty() {
            Cow::Borrowed(r"\ \&")
        } else {
            escape_quoted(mansource)
        };
        let th_manual = if manmanual.is_empty() {
            Cow::Borrowed(r"\ \&")
        } else {
            escape_quoted(manmanual)
        };
        let uppercase_title = mantitle.to_uppercase();
        let quoted_title = escape_quoted(&uppercase_title);
        let escaped_title = quoted_title.replace('-', r"\-");
        writeln!(
            w,
            ".TH \"{}\" \"{}\" \"{}\" \"{}\" \"{}\"",
            escaped_title,
            escape_quoted(manvolnum),
            escape_quoted(date),
            th_source,
            th_manual
        )?;

        writeln!(w, r".ie \n(.g .ds Aq \(aq")?;
        writeln!(w, r".el       .ds Aq '")?;
        writeln!(w, r".ss \n[.ss] 0")?;
        writeln!(w, ".nh")?;
        writeln!(w, ".ad l")?;

        let linkstyle = doc
            .attributes
            .get_string("man-linkstyle")
            .unwrap_or(Cow::Borrowed("blue R < >"));
        write_url_macros(w, &linkstyle)?;

        Ok(())
    }
}
