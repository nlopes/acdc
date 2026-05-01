//! Document-level rendering for manpages.
//!
//! Handles the `.TH` title header macro and document preamble.

use std::{borrow::Cow, io::Write};

use acdc_converters_core::{decode_numeric_char_refs, visitor::WritableVisitor};
use acdc_parser::{AttributeValue, Author, Document, InlineNode};

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

/// Extract plain text from inline nodes (for code blocks, title parsing, etc.).
pub(crate) fn extract_plain_text(nodes: &[InlineNode]) -> String {
    let mut result = String::new();
    for node in nodes {
        match node {
            InlineNode::PlainText(text) => result.push_str(text.content),
            InlineNode::RawText(text) => result.push_str(&decode_numeric_char_refs(text.content)),
            InlineNode::VerbatimText(text) => result.push_str(text.content),
            InlineNode::BoldText(bold) => result.push_str(&extract_plain_text(&bold.content)),
            InlineNode::ItalicText(italic) => result.push_str(&extract_plain_text(&italic.content)),
            InlineNode::MonospaceText(mono) => result.push_str(&extract_plain_text(&mono.content)),
            InlineNode::HighlightText(highlight) => {
                result.push_str(&extract_plain_text(&highlight.content));
            }
            InlineNode::SubscriptText(sub) => result.push_str(&extract_plain_text(&sub.content)),
            InlineNode::SuperscriptText(sup) => result.push_str(&extract_plain_text(&sup.content)),
            InlineNode::CurvedQuotationText(quoted) => {
                result.push_str(&extract_plain_text(&quoted.content));
            }
            InlineNode::CurvedApostropheText(quoted) => {
                result.push_str(&extract_plain_text(&quoted.content));
            }
            // These nodes don't contribute plain text (and future variants via wildcard)
            // InlineNode is #[non_exhaustive], so wildcard arm handles future variants
            #[allow(clippy::match_same_arms, clippy::wildcard_enum_match_arm)]
            InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::Macro(_)
            | _ => {}
        }
    }
    result
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

        self.sync_name_attributes(doc, &mantitle);

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

    /// Copy parser-derived name attributes into the visitor's document attributes.
    fn sync_name_attributes(&mut self, doc: &Document, mantitle: &str) {
        let attrs = &mut self.processor.document_attributes;
        if let Some(manname) = doc.attributes.get_string("manname") {
            attrs.insert(
                Cow::Borrowed("manname"),
                AttributeValue::String(Cow::Owned(manname.into_owned())),
            );
        } else {
            attrs.insert(
                Cow::Borrowed("manname"),
                AttributeValue::String(Cow::Owned(mantitle.to_string())),
            );
        }
        if let Some(manpurpose) = doc.attributes.get_string("manpurpose") {
            attrs.insert(
                Cow::Borrowed("manpurpose"),
                AttributeValue::String(Cow::Owned(manpurpose.into_owned())),
            );
        }
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
        let w = self.writer_mut();
        writeln!(w, r#"'\" t"#)?;

        let title_for_comment = doc.header.as_ref().map_or_else(
            || mantitle.to_string(),
            |h| {
                let full_title = extract_plain_text(&h.title);
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
