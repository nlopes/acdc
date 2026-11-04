//! Paragraph rendering for manpages.
//!
//! Handles paragraph breaks, titles, and styled paragraphs (quote, verse, literal).

use std::io::Write;

use acdc_converters_core::TraversalContext;
use acdc_parser::ast::{AttributesRef, ContentRef, ParagraphRef};

use crate::{Error, ManpageVisitor, document::extract_verbatim_text, escape::EscapeMode};

impl<'a, W: Write> ManpageVisitor<'a, '_, W> {
    /// Render a paragraph with its style-specific manpage layout.
    pub(crate) fn render_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        if let Some(style) = para.style() {
            match style {
                "quote" => return self.render_quote_paragraph(traversal, para),
                "verse" => return self.render_verse_paragraph(traversal, para),
                "example" | "sidebar" => return self.render_example_paragraph(traversal, para),
                "pass" => {
                    writeln!(
                        self.writer_mut(),
                        "{}",
                        extract_verbatim_text(para.content())
                    )?;
                    return Ok(());
                }
                "literal" | "listing" | "source" => {
                    return self.render_literal_paragraph(traversal, para);
                }
                _ => {}
            }
        }

        // Paragraph break (skip in NAME section per manpage convention)
        let skip_sp = self.in_name_section;
        let w = self.writer_mut();
        if !skip_sp {
            writeln!(w, ".sp")?;
        }

        self.render_captioned_title(traversal, para.title(), para.attributes())?;

        // Paragraph content
        self.write_content(traversal, para.content())?;

        let w = self.writer_mut();
        writeln!(w)?;

        Ok(())
    }

    fn render_example_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        self.write_sp()?;
        self.render_captioned_title(traversal, para.title(), para.attributes())?;

        let w = self.writer_mut();
        writeln!(w, ".RS 4")?;
        writeln!(w, ".sp")?;
        self.write_content(traversal, para.content())?;
        let w = self.writer_mut();
        writeln!(w)?;
        writeln!(w, ".RE")?;
        Ok(())
    }

    /// Render a quote-styled paragraph, placing its title before the indented text.
    ///
    /// Body layout:
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
    fn render_quote_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        if para.title().is_some() {
            self.write_sp()?;
            self.render_captioned_title(traversal, para.title(), para.attributes())?;
        }
        let w = self.writer_mut();

        // Quote block structure
        writeln!(w, ".RS 3")?;
        writeln!(w, ".ll -.6i")?;
        writeln!(w, ".sp")?;

        // Render content
        self.write_content(traversal, para.content())?;

        let w = self.writer_mut();
        writeln!(w)?;
        writeln!(w, ".br")?;
        writeln!(w, ".RE")?;
        writeln!(w, ".ll")?;

        // Render attribution if present
        self.render_attribution(
            traversal,
            para.attributes(),
            &[".RS 5", ".ll -.10i"],
            &[".RE", ".ll"],
        )?;

        Ok(())
    }

    /// Render a verse-styled paragraph, placing its title before the preserved lines.
    ///
    /// Body layout:
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
    fn render_verse_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        self.write_sp()?;
        self.render_captioned_title(traversal, para.title(), para.attributes())?;
        self.render_verse_content(traversal, para.content())?;

        // Render attribution if present
        self.render_attribution(
            traversal,
            para.attributes(),
            &[".br", ".in +.5i", ".ll -.5i"],
            &[".in", ".ll"],
        )?;

        Ok(())
    }

    /// Render verse inlines with their own substitutions and preserved whitespace.
    pub(crate) fn render_verse_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: ContentRef<'_>,
    ) -> Result<(), Error> {
        let mut output = Vec::new();
        let mut visitor = self.nested_visitor(&mut output);
        visitor.text_escape_mode = EscapeMode::Preserve;
        let result = visitor.write_content(traversal, nodes);
        result?;
        let w = self.writer_mut();
        writeln!(w, ".nf")?;
        w.write_all(&output)?;
        // Put the closing request on its own line without adding an empty verse line.
        if !output.is_empty() && !output.ends_with(b"\n") {
            writeln!(w)?;
        }
        writeln!(w, ".fi")?;
        Ok(())
    }

    /// Render a literal, listing, or source paragraph as preformatted text.
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
    fn render_literal_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        self.collect_index_terms_from_inlines(traversal, para.content())?;
        self.write_sp()?;
        self.render_captioned_title(traversal, para.title(), para.attributes())?;
        let content = self.verbatim_content(traversal, para.content(), para.attributes())?;
        let w = self.writer_mut();
        writeln!(w, ".if n .RS 4")?;
        writeln!(w, ".nf")?;
        writeln!(w, ".fam C")?;

        for line in content.lines() {
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
    pub(crate) fn render_attribution(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        attributes: AttributesRef<'_>,
        preamble: &[&str],
        postamble: &[&str],
    ) -> Result<(), Error> {
        let Some(acdc_parser::ast::AttributeValueRef::Content(attribution)) =
            attributes.get("attribution")
        else {
            return Ok(());
        };

        let w = self.writer_mut();
        for line in preamble {
            writeln!(w, "{line}")?;
        }

        if let Some(acdc_parser::ast::AttributeValueRef::Content(citation)) =
            attributes.get("citetitle")
        {
            self.write_content(traversal, citation)?;
            write!(self.writer_mut(), " ")?;
        }
        write!(self.writer_mut(), "\\(em ")?;
        self.write_content(traversal, attribution)?;
        writeln!(self.writer_mut())?;

        let w = self.writer_mut();
        for line in postamble {
            writeln!(w, "{line}")?;
        }

        Ok(())
    }
}
