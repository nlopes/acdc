use std::io::Write;

use acdc_parser::{DelimitedBlock, DelimitedBlockType, StemContent, StemNotation};

use crate::{Processor, Render, RenderOptions};

// Common programming languages and markup languages
const KNOWN_LANGUAGES: &[&str] = &[
    "bash",
    "shell",
    "sh",
    "zsh",
    "fish",
    "python",
    "py",
    "ruby",
    "rb",
    "javascript",
    "js",
    "typescript",
    "ts",
    "java",
    "c",
    "cpp",
    "c++",
    "csharp",
    "cs",
    "go",
    "rust",
    "rs",
    "php",
    "perl",
    "lua",
    "swift",
    "kotlin",
    "scala",
    "clojure",
    "html",
    "xml",
    "css",
    "json",
    "yaml",
    "yml",
    "toml",
    "ini",
    "sql",
    "dockerfile",
    "makefile",
    "cmake",
    "groovy",
];

impl Render for DelimitedBlock {
    type Error = crate::Error;

    #[allow(clippy::too_many_lines)]
    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        match &self.inner {
            DelimitedBlockType::DelimitedTable(t) => t.render(w, processor, options)?,
            DelimitedBlockType::DelimitedPass(inlines) => {
                crate::inlines::render_inlines(inlines, w, processor, options)?;
            }
            DelimitedBlockType::DelimitedListing(inlines) => {
                writeln!(w, "<div class=\"listingblock\">")?;
                writeln!(w, "<div class=\"content\">")?;
                crate::inlines::render_title(&self.title, w, processor, options)?;
                // Check if this is a source block with a language
                // The language is the first positional attribute (after style), which gets moved to attributes map
                let is_source = self.metadata.style.as_deref() == Some("source");
                let language = if is_source {
                    // Check if there's a "bash" (or other language) key in attributes
                    // Positional attributes get moved to attributes map, so we need to find the language
                    self.metadata.attributes.iter().find_map(|(key, _)| {
                        if KNOWN_LANGUAGES.contains(&key.as_str()) {
                            Some(key.as_str())
                        } else {
                            None
                        }
                    })
                } else {
                    None
                };

                if let Some(lang) = language {
                    write!(
                        w,
                        "<pre class=\"highlight\"><code class=\"language-{lang}\" data-lang=\"{lang}\">"
                    )?;
                } else {
                    writeln!(w, "<pre>")?;
                }

                crate::inlines::render_inlines(inlines, w, processor, options)?;

                if language.is_some() {
                    writeln!(w, "</code></pre>")?;
                } else {
                    writeln!(w, "</pre>")?;
                }

                writeln!(w, "</div>")?;
                writeln!(w, "</div>")?;
            }
            DelimitedBlockType::DelimitedLiteral(inlines) => {
                // Check for custom style other than "source" - I've done this because
                // `asciidoctor` seems to always use "literalblock" for source blocks or
                // so I think!
                if let Some(style) = &self.metadata.style
                    && style != "source"
                {
                    writeln!(w, "<div class=\"{style}block\">")?;
                } else {
                    writeln!(w, "<div class=\"literalblock\">")?;
                }

                crate::inlines::render_title(&self.title, w, processor, options)?;

                writeln!(w, "<div class=\"content\">")?;
                writeln!(w, "<pre>")?;
                crate::inlines::render_inlines(inlines, w, processor, options)?;
                writeln!(w, "</pre>")?;
                writeln!(w, "</div>")?;
                writeln!(w, "</div>")?;
            }
            DelimitedBlockType::DelimitedQuote(blocks) => {
                if let Some(style) = &self.metadata.style {
                    writeln!(w, "<div class=\"{style}block\">")?;
                } else {
                    writeln!(w, "<div class=\"quoteblock\">")?;
                }
                writeln!(w, "<blockquote>")?;
                for block in blocks {
                    block.render(w, processor, options)?;
                }
                writeln!(w, "</blockquote>")?;
                writeln!(w, "</div>")?;
            }
            DelimitedBlockType::DelimitedOpen(blocks) => {
                writeln!(w, "<div class=\"openblock\">")?;
                crate::inlines::render_title(&self.title, w, processor, options)?;
                writeln!(w, "<div class=\"content\">")?;
                for block in blocks {
                    block.render(w, processor, options)?;
                }
                writeln!(w, "</div>")?;
                writeln!(w, "</div>")?;
            }
            DelimitedBlockType::DelimitedStem(stem) => {
                writeln!(w, "<div class=\"stemblock\">")?;
                crate::inlines::render_title(&self.title, w, processor, options)?;
                stem.render(w, processor, options)?;
                writeln!(w, "</div>")?;
            }
            DelimitedBlockType::DelimitedComment(_) => {
                // Comment blocks produce no output
            }
            DelimitedBlockType::DelimitedExample(blocks) => {
                writeln!(w, "<div class=\"exampleblock\">")?;

                // Render title with "Example N." prefix if title exists
                if !self.title.is_empty() {
                    let count = processor.example_counter.get() + 1;
                    processor.example_counter.set(count);
                    write!(w, "<div class=\"title\">Example {count}. ")?;
                    crate::inlines::render_inlines(&self.title, w, processor, options)?;
                    writeln!(w, "</div>")?;
                }

                writeln!(w, "<div class=\"content\">")?;
                for block in blocks {
                    block.render(w, processor, options)?;
                }
                writeln!(w, "</div>")?;
                writeln!(w, "</div>")?;
            }
            DelimitedBlockType::DelimitedSidebar(blocks) => {
                writeln!(w, "<div class=\"sidebarblock\">")?;
                writeln!(w, "<div class=\"content\">")?;
                crate::inlines::render_title(&self.title, w, processor, options)?;
                for block in blocks {
                    block.render(w, processor, options)?;
                }
                writeln!(w, "</div>")?;
                writeln!(w, "</div>")?;
            }
            DelimitedBlockType::DelimitedVerse(inlines) => {
                writeln!(w, "<div class=\"verseblock\">")?;
                crate::inlines::render_title(&self.title, w, processor, options)?;
                write!(w, "<pre class=\"content\">")?;
                crate::inlines::render_inlines(inlines, w, processor, options)?;
                writeln!(w, "</pre>")?;
                writeln!(w, "<div class=\"attribution\">")?;

                // Extract author and cite from positional attributes
                // [verse, author, cite] -> positional_attributes[0] = author, [1] = cite
                let author = self.metadata.positional_attributes.first();
                let cite = self.metadata.positional_attributes.get(1);

                if let Some(author) = author {
                    writeln!(w, "&#8212; {author}<br>")?;
                }
                if let Some(cite) = cite {
                    writeln!(w, "<cite>{cite}</cite>")?;
                }

                writeln!(w, "</div>")?;
                writeln!(w, "</div>")?;
            }
            _ => {
                let inner = &self.inner;
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    format!("Unsupported delimited block type: {inner:?}"),
                )
                .into());
            }
        }
        Ok(())
    }
}

impl Render for StemContent {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        _processor: &Processor,
        _options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<div class=\"content\">")?;
        match self.notation {
            StemNotation::Latexmath => {
                write!(w, "\\[{}\\]", self.content)?;
            }
            StemNotation::Asciimath => {
                write!(w, "\\${}\\$", self.content)?;
            }
        }
        writeln!(w, "</div>")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_converters_common::{GeneratorMetadata, Options};
    use acdc_core::{Doctype, SafeMode, Source};
    use acdc_parser::{BlockMetadata, DocumentAttributes, InlineNode, Location, Plain};
    use std::cell::Cell;

    fn create_test_inlines(content: &str) -> Vec<InlineNode> {
        vec![InlineNode::PlainText(Plain {
            content: content.to_string(),
            location: Location::default(),
        })]
    }

    fn create_test_processor() -> Processor {
        let options = Options {
            generator_metadata: GeneratorMetadata::default(),
            doctype: Doctype::Article,
            source: Source::String("test".to_string()),
            safe_mode: SafeMode::Unsafe,
            timings: false,
        };
        let document_attributes = DocumentAttributes::default();
        Processor {
            options,
            document_attributes,
            toc_entries: Vec::new(),
            example_counter: Cell::new(0),
        }
    }

    #[test]
    fn test_listing_block_renders_as_listingblock() {
        let block = DelimitedBlock {
            metadata: BlockMetadata::default(),
            inner: DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            delimiter: "----".to_string(),
            title: Vec::new(),
            location: Location::default(),
        };

        let mut output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();

        block.render(&mut output, &processor, &options).unwrap();
        let html = String::from_utf8(output).unwrap();

        assert!(
            html.contains("<div class=\"listingblock\">"),
            "Expected listing block to render with 'listingblock' class, got: {html}",
        );
    }

    #[test]
    fn test_literal_block_renders_as_literalblock() {
        let block = DelimitedBlock {
            metadata: BlockMetadata::default(),
            inner: DelimitedBlockType::DelimitedLiteral(create_test_inlines("literal text")),
            delimiter: "....".to_string(),
            title: Vec::new(),
            location: Location::default(),
        };

        let mut output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();

        block.render(&mut output, &processor, &options).unwrap();
        let html = String::from_utf8(output).unwrap();

        assert!(
            html.contains("<div class=\"literalblock\">"),
            "Expected literal block to render with 'literalblock' class, got: {html}",
        );
    }

    #[test]
    fn test_listing_block_with_source_style_and_language() {
        use acdc_parser::{AttributeValue, ElementAttributes};

        let mut attributes = ElementAttributes::default();
        attributes.insert("bash".to_string(), AttributeValue::None);

        let metadata = BlockMetadata {
            style: Some("source".to_string()),
            attributes,
            ..Default::default()
        };

        let block = DelimitedBlock {
            metadata,
            inner: DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            delimiter: "----".to_string(),
            title: Vec::new(),
            location: Location::default(),
        };

        let mut output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();

        block.render(&mut output, &processor, &options).unwrap();
        let html = String::from_utf8(output).unwrap();

        assert!(
            html.contains("<div class=\"listingblock\">"),
            "Expected listing block to always use 'listingblock' class, got: {html}",
        );
        assert!(
            html.contains("<pre class=\"highlight\">"),
            "Expected source block to have 'highlight' class on pre tag, got: {html}",
        );
        assert!(
            html.contains("<code class=\"language-bash\" data-lang=\"bash\">"),
            "Expected source block to have language attributes, got: {html}",
        );
    }

    #[test]
    fn test_literal_block_with_style_attribute() {
        let metadata = BlockMetadata {
            style: Some("verse".to_string()),
            ..Default::default()
        };

        let block = DelimitedBlock {
            metadata,
            inner: DelimitedBlockType::DelimitedLiteral(create_test_inlines("literal text")),
            delimiter: "....".to_string(),
            title: Vec::new(),
            location: Location::default(),
        };

        let mut output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();

        block.render(&mut output, &processor, &options).unwrap();
        let html = String::from_utf8(output).unwrap();

        assert!(
            html.contains("<div class=\"verseblock\">"),
            "Expected literal block with style to render with custom class, got: {html}",
        );
        assert!(
            !html.contains("<div class=\"literalblock\">"),
            "Literal block with style should not have default 'literalblock' class"
        );
    }
}
