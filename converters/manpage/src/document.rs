//! Document-level rendering for manpages.
//!
//! Handles the `.TH` title header macro and document preamble.

use std::{borrow::Cow, io::Write};

use acdc_converters_core::visitor::WritableVisitor;
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
#[allow(clippy::too_many_lines)]
pub(crate) fn visit_document_start<W: Write>(
    doc: &Document,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    // In embedded mode, skip the entire preamble (comment block, .TH, macros, settings)
    // This matches asciidoctor's --embedded behavior for manpages
    if visitor.processor.options.embedded() {
        return Ok(());
    }

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
    // Support both forms: :mansource: and :man source: (asciidoctor accepts both)
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

    // Get date - use revdate from document, then processor (source file mtime), then current date
    let date = doc
        .attributes
        .get_string("revdate")
        .or_else(|| visitor.processor.document_attributes.get_string("revdate"))
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());

    let w = visitor.writer_mut();

    // Write comment header (enables tbl preprocessor)
    writeln!(w, r#"'\" t"#)?;

    // Get the document title for the header comment
    // Use the original document title without volume number (matching asciidoctor)
    let title_for_comment = doc.header.as_ref().map_or_else(
        || mantitle.clone(),
        |h| {
            let full_title = extract_plain_text(&h.title);
            // Strip trailing (N) volume number if present
            full_title
                .rsplit_once('(')
                .filter(|(_, vol)| vol.ends_with(')') && vol.len() <= 3)
                .map_or(full_title.clone(), |(name, _)| name.to_string())
        },
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
    writeln!(w, r#".\""#)?;

    // Write .TH macro
    // Format: .TH "NAME" "VOLUME" "DATE" "SOURCE" "MANUAL"
    // Use `\ \&` for empty fields (roff idiom for "intentionally blank")
    // Note: Don't escape the special `\ \&` value - backslashes are intentional roff escapes
    let th_source = if mansource.is_empty() {
        Cow::Borrowed(r"\ \&")
    } else {
        escape_quoted(&mansource)
    };
    let th_manual = if manmanual.is_empty() {
        Cow::Borrowed(r"\ \&")
    } else {
        escape_quoted(&manmanual)
    };
    // Escape hyphens in the title to prevent line breaking (roff convention)
    // Note: Apply escape_quoted first, then replace hyphens to avoid double-escaping
    let uppercase_title = mantitle.to_uppercase();
    let quoted_title = escape_quoted(&uppercase_title);
    let escaped_title = quoted_title.replace('-', r"\-");
    writeln!(
        w,
        ".TH \"{}\" \"{}\" \"{}\" \"{}\" \"{}\"",
        escaped_title,
        escape_quoted(&manvolnum),
        escape_quoted(&date),
        th_source,
        th_manual
    )?;

    // Define portable apostrophe string:
    // - GNU troff (.g register set): use proper typographic apostrophe \(aq
    // - Other implementations: fall back to ASCII apostrophe '
    writeln!(w, r".ie \n(.g .ds Aq \(aq")?;
    writeln!(w, r".el       .ds Aq '")?;

    // Disable extra space after sentence-ending punctuation (modern convention)
    writeln!(w, r".ss \n[.ss] 0")?;

    // Disable hyphenation and left-align only
    writeln!(w, ".nh")?;
    writeln!(w, ".ad l")?;

    // Get linkstyle from document attributes (default: "blue R < >")
    let linkstyle = doc
        .attributes
        .get_string("man-linkstyle")
        .unwrap_or_else(|| "blue R < >".to_string());

    write_url_macros(w, &linkstyle)?;

    Ok(())
}
