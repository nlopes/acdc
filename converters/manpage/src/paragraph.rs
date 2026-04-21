//! Paragraph rendering for manpages.
//!
//! Handles paragraph breaks, titles, and styled paragraphs (quote, verse, literal).

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{Paragraph, inlines_to_string};

use crate::{
    Error, ManpageVisitor,
    document::extract_plain_text,
    escape::{EscapeMode, manify},
};

impl<W: Write> ManpageVisitor<'_, W> {
    /// Visit a paragraph, handling styled paragraphs (quote, verse, literal).
    pub(crate) fn render_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        // Check for styled paragraphs
        if let Some(style) = para.metadata.style {
            match style {
                "quote" => return self.render_quote_paragraph(para),
                "verse" => return self.render_verse_paragraph(para),
                "literal" | "listing" | "source" => return self.render_literal_paragraph(para),
                _ => {}
            }
        }

        // Paragraph break (skip in NAME section per manpage convention)
        let skip_sp = self.in_name_section;
        let w = self.writer_mut();
        if !skip_sp {
            writeln!(w, ".sp")?;
        }

        // Optional title (rendered as bold)
        if !para.title.is_empty() {
            write!(w, "\\fB")?;
            self.visit_inline_nodes(&para.title)?;
            let w = self.writer_mut();
            writeln!(w, "\\fP")?;
            writeln!(w, ".br")?;
        }

        // Paragraph content
        self.visit_inline_nodes(&para.content)?;

        let w = self.writer_mut();
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
    fn render_quote_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        let w = self.writer_mut();

        // Quote block structure
        writeln!(w, ".RS 3")?;
        writeln!(w, ".ll -.6i")?;
        writeln!(w, ".sp")?;

        // Render content
        self.visit_inline_nodes(&para.content)?;

        let w = self.writer_mut();
        writeln!(w)?;
        writeln!(w, ".br")?;
        writeln!(w, ".RE")?;
        writeln!(w, ".ll")?;

        // Render attribution if present
        self.render_para_attribution(para, &[".RS 5", ".ll -.10i"], &[".RE", ".ll"])?;

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
    fn render_verse_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        let w = self.writer_mut();

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
        self.render_para_attribution(para, &[".br", ".in +.5i", ".ll -.5i"], &[".in", ".ll"])?;

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
    fn render_literal_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        let w = self.writer_mut();

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

    /// Render attribution with configurable roff preamble/postamble.
    ///
    /// Format: preamble lines, then `Citation \(em Author`, then postamble lines.
    fn render_para_attribution(
        &mut self,
        para: &Paragraph,
        preamble: &[&str],
        postamble: &[&str],
    ) -> Result<(), Error> {
        let attribution = para
            .metadata
            .attribution
            .as_ref()
            .map(|a| inlines_to_string(a));
        let citation = para
            .metadata
            .citetitle
            .as_ref()
            .map(|c| inlines_to_string(c));

        if attribution.is_some() || citation.is_some() {
            let w = self.writer_mut();

            for line in preamble {
                writeln!(w, "{line}")?;
            }

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
            for line in postamble {
                writeln!(w, "{line}")?;
            }
        }

        Ok(())
    }
}
