//! Document-level rendering for manpages.
//!
//! Handles the `.TH` title header macro and document preamble.

use std::io::Write;

use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{AttributeValue, Author, Document, InlineNode};

use crate::{Error, ManpageVisitor, escape::escape_quoted};

/// The version of the acdc package (from Cargo.toml).
const VERSION: &str = env!("CARGO_PKG_VERSION");

const SEE_THE_AUTHOR_SECTION: &str = r#"[see the "AUTHOR(S)" section]"#;

/// Format an author's full name for display.
fn format_author_name(author: &Author) -> String {
    match &author.middle_name {
        Some(middle) => format!("{} {middle} {}", author.first_name, author.last_name),
        None => format!("{} {}", author.first_name, author.last_name),
    }
}

/// Format the author line for the comment header.
///
/// Returns the first author's name if available, otherwise a reference to
/// the AUTHOR(S) section.
fn format_author_line(authors: &[Author]) -> String {
    authors
        .first()
        .map_or_else(|| SEE_THE_AUTHOR_SECTION.to_string(), format_author_name)
}

/// Write a comment line with a right-aligned label (12 chars total width).
fn write_comment_line<W: Write + ?Sized>(
    w: &mut W,
    label: &str,
    value: &str,
) -> std::io::Result<()> {
    // Labels are right-aligned to 11 characters (including the colon)
    // This matches asciidoctor's comment header alignment
    writeln!(w, r#"."{:>11} {value}"#, format!("{label}:"))
}

/// Extract plain text from inline nodes (for code blocks, title parsing, etc.).
pub fn extract_plain_text(nodes: &[InlineNode]) -> String {
    let mut result = String::new();
    for node in nodes {
        match node {
            InlineNode::PlainText(text) => result.push_str(&text.content),
            InlineNode::RawText(text) => result.push_str(&text.content),
            InlineNode::VerbatimText(text) => result.push_str(&text.content),
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

/// Visit document start - generates the .TH header and preamble.
///
/// Reads manpage attributes that were derived by the parser:
/// - `mantitle`: The program name from the document title
/// - `manvolnum`: The volume number from the document title
/// - `manname`: From NAME section (or falls back to mantitle)
/// - `manpurpose`: From NAME section (after ` - `)
/// - `_manpage_title_conforming`: Whether the title conforms to name(volume) format
pub fn visit_document_start<W: Write>(
    doc: &Document,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    // Ensure we have a header
    if doc.header.is_none() {
        return Err(Error::MissingHeader);
    }

    // Get mantitle and manvolnum from document attributes (set by parser)
    // The parser always sets these now (either from conforming title or fallbacks)
    let mantitle = doc
        .attributes
        .get_string("mantitle")
        .ok_or_else(|| Error::InvalidManpageTitle("missing mantitle attribute".to_string()))?;

    let manvolnum = doc
        .attributes
        .get_string("manvolnum")
        .unwrap_or(String::from("1"));

    // Copy parser-derived attributes to visitor for use during conversion.
    // These are already set by the parser (from NAME section), but we need
    // to ensure they're in the visitor's attribute map for rendering.
    let attrs = &mut visitor.processor.document_attributes;

    // manname: from parser (NAME section) or fall back to mantitle
    if let Some(manname) = doc.attributes.get_string("manname") {
        attrs.insert("manname".to_string(), AttributeValue::String(manname));
    } else {
        attrs.insert(
            "manname".to_string(),
            AttributeValue::String(mantitle.clone()),
        );
    }

    // manpurpose: from parser (NAME section), if available
    if let Some(manpurpose) = doc.attributes.get_string("manpurpose") {
        attrs.insert("manpurpose".to_string(), AttributeValue::String(manpurpose));
    }

    // Get optional attributes (user-provided or defaults)
    let mansource = doc.attributes.get_string("mansource").unwrap_or_default();
    let manmanual = doc.attributes.get_string("manmanual").unwrap_or_default();

    // Get date - use revdate attribute or current date
    let date = doc
        .attributes
        .get_string("revdate")
        .unwrap_or(chrono::Local::now().format("%Y-%m-%d").to_string());

    let w = visitor.writer_mut();

    // Write comment header (enables tbl preprocessor)
    writeln!(w, r#"'\" t"#)?;

    // Get the document title for the header comment
    // Use the original document title (not mantitle) to match asciidoctor behavior
    let title_for_comment = doc.header.as_ref().map_or_else(
        || mantitle.clone(),
        |h| extract_plain_text(&h.title),
    );

    // Get author information from the header
    let author_line = doc.header.as_ref().map_or_else(
        || SEE_THE_AUTHOR_SECTION.to_string(),
        |h| format_author_line(&h.authors),
    );

    // Format manual and source - use `\ \&` for empty values (roff escape for nothing)
    let manual_display = if manmanual.is_empty() {
        r"\ \&".to_string()
    } else {
        manmanual.clone()
    };
    let source_display = if mansource.is_empty() {
        r"\ \&".to_string()
    } else {
        mansource.clone()
    };

    // Write metadata comment block (matches asciidoctor format)
    write_comment_line(w, "Title", &title_for_comment)?;
    write_comment_line(w, "Author", &author_line)?;
    write_comment_line(w, "Generator", &format!("acdc {VERSION}"))?;
    write_comment_line(w, "Date", &date)?;
    write_comment_line(w, "Manual", &manual_display)?;
    write_comment_line(w, "Source", &source_display)?;
    write_comment_line(w, "Language", "English")?;
    writeln!(w, r#".""#)?;

    // Write .TH macro
    // Format: .TH "NAME" "VOLUME" "DATE" "SOURCE" "MANUAL"
    writeln!(
        w,
        ".TH \"{}\" \"{}\" \"{}\" \"{}\" \"{}\"",
        escape_quoted(&mantitle.to_uppercase()),
        escape_quoted(&manvolnum),
        escape_quoted(&date),
        escape_quoted(&mansource),
        escape_quoted(&manmanual)
    )?;

    // Write preamble settings (targeting modern groff)
    writeln!(w, r#".\" Disable hyphenation"#)?;
    writeln!(w, ".nh")?;
    writeln!(w, r#".\" Left-align only"#)?;
    writeln!(w, ".ad l")?;

    // Get linkstyle from document attributes (default: "blue R < >")
    let linkstyle = doc
        .attributes
        .get_string("man-linkstyle")
        .unwrap_or_else(|| "blue R < >".to_string());

    // Define URL and MTO macros for link handling (matches asciidoctor)
    writeln!(w, r#".\" URL/email macros"#)?;
    writeln!(w, ".de URL")?;
    writeln!(w, r"\fI\\$2\fP <\\$1>\\$3")?;
    writeln!(w, "..")?;
    writeln!(w, ".als MTO URL")?;
    writeln!(w, r".if \n[.g] \{{")?;
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
