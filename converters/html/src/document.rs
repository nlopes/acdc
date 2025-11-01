use std::io::Write;

use acdc_converters_common::toc as toc_common;
use acdc_parser::{Author, Block, Document, Footnote, Header};

use crate::{Processor, Render, RenderOptions, toc};

impl Render for Document {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<!DOCTYPE html>")?;
        writeln!(w, "<html>")?;
        render_head(self, w, processor, options)?;
        writeln!(w, "<body class=\"{}\">", processor.options.doctype)?;

        let toc_placement = toc_common::get_placement_from_attributes(&self.attributes);

        render_body_header(self, w, processor, options, toc_placement)?;

        writeln!(w, "<div id=\"content\">")?;
        let mut blocks = self.blocks.clone();
        let preamble = find_preamble(&mut blocks);
        if let Some(preamble) = preamble {
            writeln!(w, "<div id=\"preamble\">")?;
            writeln!(w, "<div class=\"sectionbody\">")?;
            for block in &preamble {
                block.render(w, processor, options)?;
            }
            writeln!(w, "</div>")?;
            writeln!(w, "</div>")?;
            if toc_placement == "preamble" {
                toc::render(w, processor, options)?;
            }
        }
        for block in &blocks {
            block.render(w, processor, options)?;
        }
        writeln!(w, "</div>")?;

        // Render footnotes if any exist
        if !self.footnotes.is_empty() {
            render_footnotes(&self.footnotes, w, processor, options)?;
        }
        render_body_footer(w, options)?;
        writeln!(w, "</body>")?;
        writeln!(w, "</html>")?;
        Ok(())
    }
}

fn render_head<W: Write>(
    document: &Document,
    w: &mut W,
    processor: &Processor,
    options: &RenderOptions,
) -> Result<(), crate::Error> {
    writeln!(w, "<head>")?;
    writeln!(w, "<meta charset=\"utf-8\">")?;
    writeln!(
        w,
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">"
    )?;
    writeln!(
        w,
        "<meta name=\"generator\" content=\"{}\">",
        processor.options.generator_metadata
    )?;
    if let Some(header) = &document.header {
        header.render(
            w,
            processor,
            &RenderOptions {
                inlines_basic: true,
                ..*options
            },
        )?;
    }
    writeln!(
        w,
        r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css?family=Open+Sans:300,300italic,400,400italic,600,600italic%7CNoto+Serif:400,400italic,700,700italic%7CDroid+Sans+Mono:400,700">
<style>
{}
.stemblock .content {{
  text-align: center;
}}
</style>
"#,
        include_str!("../static/asciidoctor.css")
    )?;

    // Add MathJax if stem is enabled
    if processor.document_attributes.get("stem").is_some() {
        writeln!(
            w,
            r#"<script>
MathJax = {{
      loader: {{load: ['input/asciimath']}},
      tex: {{
        processEscapes: false
      }},
      asciimath: {{
        delimiters: {{'[+]': [['\\$','\\$']]}},
        displaystyle: false
      }},
      options: {{
        ignoreHtmlClass: 'tex2jax_ignore|nostem|nolatexmath|noasciimath',
        processHtmlClass: 'tex2jax_process'
      }},
      startup: {{
        ready() {{
          MathJax.startup.defaultReady();
          MathJax.startup.promise.then(() => {{
            const asciimath = MathJax._.input.asciimath.AsciiMath;
            if (asciimath) {{
              const originalCompile = asciimath.compile;
              asciimath.compile = function(math, display) {{
                const node = math.math;
                if (node && node.parentElement && node.parentElement.parentElement &&
                  node.parentElement.parentElement.classList.contains('stemblock')) {{
                  display = true;
                }}
                return originalCompile.call(this, math, display);
              }};
            }}
          }});
        }}
      }}
}};
</script>
<script defer src="https://cdn.jsdelivr.net/npm/mathjax@4/tex-mml-chtml.js"></script>"#
        )?;
    }

    // Add Font Awesome if icons are set to font mode
    if processor
        .document_attributes
        .get("icons")
        .is_some_and(|v| v.to_string() == "font")
    {
        writeln!(
            w,
            r#"<link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/7.0.1/css/all.min.css">
<link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/7.0.1/css/v4-shims.min.css">"#
        )?;
    }

    writeln!(w, "</head>")?;
    Ok(())
}

fn render_body_header<W: Write>(
    document: &Document,
    w: &mut W,
    processor: &Processor,
    options: &RenderOptions,
    toc_placement: &str,
) -> Result<(), crate::Error> {
    writeln!(w, "<div id=\"header\">")?;
    if let Some(header) = &document.header
        && !header.title.is_empty()
    {
        write!(w, "<h1>")?;
        crate::inlines::render_inlines(&header.title, w, processor, options)?;
        if let Some(subtitle) = &header.subtitle {
            write!(w, ": ")?;
            crate::inlines::render_inlines(subtitle, w, processor, options)?;
        }
        writeln!(w, "</h1>")?;
        writeln!(w, "<div class=\"details\">")?;
        if !header.authors.is_empty() {
            for (i, author) in header.authors.iter().enumerate() {
                writeln!(
                    w,
                    "<span id=\"author{}\" class=\"author\">",
                    if i > 0 {
                        format!("{}", i + 1)
                    } else {
                        String::new()
                    }
                )?;
                write!(w, "{} ", author.first_name)?;
                if let Some(middle_name) = &author.middle_name {
                    write!(w, "{middle_name} ")?;
                }
                write!(w, "{}", author.last_name)?;
                writeln!(w, "</span>")?;
                writeln!(w, "<br>")?;
                if let Some(email) = &author.email {
                    writeln!(
                        w,
                        "<span id=\"email{}\" class=\"email\">",
                        if i > 0 {
                            format!("{}", i + 1)
                        } else {
                            String::new()
                        }
                    )?;

                    writeln!(w, "<a href=\"mailto:{email}\">{email}</a>")?;
                    writeln!(w, "</span>")?;
                    writeln!(w, "<br>")?;
                }
            }
        }
        writeln!(w, "</div>")?;
    }
    // Render TOC after header if toc="auto"
    if toc_placement == "auto" {
        toc::render(w, processor, options)?;
    }
    writeln!(w, "</div>")?;
    Ok(())
}

fn render_body_footer<W: Write>(w: &mut W, options: &RenderOptions) -> Result<(), crate::Error> {
    writeln!(w, "<div id=\"footer\">")?;
    writeln!(w, "<div id=\"footer-text\">")?;
    if let Some(last_updated) = options.last_updated {
        writeln!(w, "Last updated {}", last_updated.format("%F %T %Z"))?;
    }
    writeln!(w, "</div>")?;
    writeln!(w, "</div>")?;
    Ok(())
}

fn find_preamble(blocks: &mut Vec<Block>) -> Option<Vec<Block>> {
    let mut first_section_index = 0;
    for (index, block) in blocks.iter().enumerate() {
        if let Block::Section(_) = block {
            first_section_index = index;
            break;
        }
    }
    if first_section_index > 0 {
        Some(blocks.drain(..first_section_index).collect::<Vec<_>>())
    } else {
        None
    }
}

impl Render for Header {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        self.authors.iter().try_for_each(|author| {
            author.render(w, processor, options)?;
            Ok::<(), Self::Error>(())
        })?;
        write!(w, "<title>")?;
        crate::inlines::render_inlines(&self.title, w, processor, options)?;
        if let Some(subtitle) = &self.subtitle {
            write!(w, ": ")?;
            crate::inlines::render_inlines(subtitle, w, processor, options)?;
        }
        writeln!(w, "</title>")?;
        Ok(())
    }
}

impl Render for Author {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        _processor: &Processor,
        _options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        write!(w, "<meta name=\"author\" content=\"")?;
        write!(w, "{} ", self.first_name)?;
        if let Some(middle_name) = &self.middle_name {
            write!(w, "{middle_name} ")?;
        }
        write!(w, "{}", self.last_name)?;
        if let Some(email) = &self.email {
            write!(w, " <{email}>")?;
        }
        writeln!(w, "\">")?;
        Ok(())
    }
}

fn render_footnotes<W: Write>(
    footnotes: &[Footnote],
    w: &mut W,
    processor: &Processor,
    options: &RenderOptions,
) -> Result<(), crate::Error> {
    writeln!(w, "<div id=\"footnotes\">")?;
    writeln!(w, "<hr>")?;
    for footnote in footnotes {
        let number = footnote.number;
        writeln!(w, "<div class=\"footnote\" id=\"_footnotedef_{number}\">")?;
        write!(w, "<a href=\"#_footnoteref_{number}\">{number}</a>. ")?;
        crate::inlines::render_inlines(&footnote.content, w, processor, options)?;
        writeln!(w, "</div>")?;
    }
    writeln!(w, "</div>")?;
    Ok(())
}
