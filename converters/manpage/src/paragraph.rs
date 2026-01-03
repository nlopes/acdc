//! Paragraph rendering for manpages.
//!
//! Handles `.PP` paragraph macro, paragraph titles, and styled paragraphs.

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::Paragraph;

use crate::{
    Error, ManpageVisitor,
    document::extract_plain_text,
    escape::{EscapeMode, manify},
};

/// Visit a paragraph, handling styled paragraphs (quote, verse, literal).
pub fn visit_paragraph<W: Write>(
    para: &Paragraph,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    // Check for styled paragraphs
    if let Some(style) = &para.metadata.style {
        match style.as_str() {
            "quote" => return render_quote_paragraph(para, visitor),
            "verse" => return render_verse_paragraph(para, visitor),
            "literal" => return render_literal_paragraph(para, visitor),
            _ => {}
        }
    }

    // Paragraph break (skip in NAME section per manpage convention)
    let skip_pp = visitor.in_name_section;
    let w = visitor.writer_mut();
    if !skip_pp {
        writeln!(w, ".PP")?;
    }

    // Optional title (rendered as bold)
    if !para.title.is_empty() {
        write!(w, "\\fB")?;
        visitor.visit_inline_nodes(&para.title)?;
        let w = visitor.writer_mut();
        writeln!(w, "\\fP")?;
        writeln!(w, ".br")?;
    }

    // Paragraph content
    visitor.visit_inline_nodes(&para.content)?;

    let w = visitor.writer_mut();
    writeln!(w)?;

    Ok(())
}

/// Render a quote-styled paragraph (asciidoctor-compatible).
///
/// Output format:
/// ```roff
/// .RS 3
/// .ll -.6i
/// .sp
/// Content here
/// .br
/// .RE
/// .ll
/// .RS 5
/// .ll -.10i
/// \(em Author Name
/// .RE
/// .ll
/// ```
fn render_quote_paragraph<W: Write>(
    para: &Paragraph,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    // Quote block structure
    writeln!(w, ".RS 3")?;
    writeln!(w, ".ll -.6i")?;
    writeln!(w, ".sp")?;

    // Render content
    visitor.visit_inline_nodes(&para.content)?;

    let w = visitor.writer_mut();
    writeln!(w)?;
    writeln!(w, ".br")?;
    writeln!(w, ".RE")?;
    writeln!(w, ".ll")?;

    // Render attribution if present
    render_attribution(visitor, para)?;

    Ok(())
}

/// Render a verse-styled paragraph (asciidoctor-compatible).
///
/// Output format:
/// ```roff
/// .sp
/// .nf
/// Line one
/// Line two
/// .fi
/// .br
/// .in +.5i
/// .ll -.5i
/// Citation \(em Author
/// .in
/// .ll
/// ```
fn render_verse_paragraph<W: Write>(
    para: &Paragraph,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    // Verse block - preserve line breaks
    writeln!(w, ".sp")?;
    writeln!(w, ".nf")?;

    // Extract and write content preserving whitespace
    let content = extract_plain_text(&para.content);
    let escaped = manify(&content, EscapeMode::Preserve);
    for line in escaped.lines() {
        writeln!(w, "{line}")?;
    }

    writeln!(w, ".fi")?;

    // Render attribution if present
    render_verse_attribution(visitor, para)?;

    Ok(())
}

/// Render a literal-styled paragraph (asciidoctor-compatible).
///
/// Output format:
/// ```roff
/// .sp
/// .if n .RS 4
/// .nf
/// .fam C
/// Content here
/// .fam
/// .fi
/// .if n .RE
/// ```
fn render_literal_paragraph<W: Write>(
    para: &Paragraph,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    writeln!(w, ".sp")?;
    writeln!(w, ".if n .RS 4")?;
    writeln!(w, ".nf")?;
    writeln!(w, ".fam C")?;

    // Extract and write content preserving whitespace
    let content = extract_plain_text(&para.content);
    let escaped = manify(&content, EscapeMode::Preserve);
    for line in escaped.lines() {
        writeln!(w, "{line}")?;
    }

    writeln!(w, ".fam")?;
    writeln!(w, ".fi")?;
    writeln!(w, ".if n .RE")?;

    Ok(())
}

/// Render attribution for quote paragraphs (asciidoctor format).
///
/// Format: `.RS 5` block with `\(em Author` or `Citation \(em Author`
fn render_attribution<W: Write>(
    visitor: &mut ManpageVisitor<W>,
    para: &Paragraph,
) -> Result<(), Error> {
    let attribution = para.metadata.attributes.get_string("attribution");
    let citation = para.metadata.attributes.get_string("citation");

    if attribution.is_some() || citation.is_some() {
        let w = visitor.writer_mut();

        writeln!(w, ".RS 5")?;
        writeln!(w, ".ll -.10i")?;

        // Format: "Citation \(em Author" or just "\(em Author" or just "Citation"
        if let Some(cite) = citation {
            let escaped = manify(&cite, EscapeMode::Normalize);
            write!(w, "{escaped}")?;
            if attribution.is_some() {
                write!(w, " ")?;
            }
        }

        if let Some(author) = attribution {
            let escaped = manify(&author, EscapeMode::Normalize);
            write!(w, "\\(em {escaped}")?;
        }

        writeln!(w)?;
        writeln!(w, ".RE")?;
        writeln!(w, ".ll")?;
    }

    Ok(())
}

/// Render attribution for verse paragraphs (asciidoctor format).
///
/// Format: `.in +.5i` block with `Citation \(em Author`
fn render_verse_attribution<W: Write>(
    visitor: &mut ManpageVisitor<W>,
    para: &Paragraph,
) -> Result<(), Error> {
    let attribution = para.metadata.attributes.get_string("attribution");
    let citation = para.metadata.attributes.get_string("citation");

    if attribution.is_some() || citation.is_some() {
        let w = visitor.writer_mut();

        writeln!(w, ".br")?;
        writeln!(w, ".in +.5i")?;
        writeln!(w, ".ll -.5i")?;

        // Format: "Citation \(em Author" or just "\(em Author" or just "Citation"
        if let Some(cite) = citation {
            let escaped = manify(&cite, EscapeMode::Normalize);
            write!(w, "{escaped}")?;
            if attribution.is_some() {
                write!(w, " ")?;
            }
        }

        if let Some(author) = attribution {
            let escaped = manify(&author, EscapeMode::Normalize);
            write!(w, "\\(em {escaped}")?;
        }

        writeln!(w)?;
        writeln!(w, ".in")?;
        writeln!(w, ".ll")?;
    }

    Ok(())
}
