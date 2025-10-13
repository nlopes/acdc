use std::io::Write;

use acdc_parser::{DelimitedBlock, DelimitedBlockType};

use crate::{Processor, Render, RenderOptions};

impl Render for DelimitedBlock {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<div>")?;
        match &self.inner {
            DelimitedBlockType::DelimitedTable(t) => t.render(w, processor, options)?,
            DelimitedBlockType::DelimitedPass(inlines) => {
                crate::inlines::render_inlines(inlines, w, processor, options)?;
            }
            DelimitedBlockType::DelimitedListing(inlines) => {
                if let Some(style) = &self.metadata.style {
                    writeln!(w, "<div class=\"{style}block\">")?;
                } else {
                    writeln!(w, "<div class=\"listingblock\">")?;
                }
                write!(w, "<div class=\"title\">")?;
                crate::inlines::render_inlines(&self.title, w, processor, options)?;
                writeln!(w, "</div>")?;
                writeln!(w, "<div class=\"content\">")?;
                writeln!(w, "<pre>")?;
                crate::inlines::render_inlines(
                    inlines,
                    w,
                    processor,
                    &RenderOptions {
                        inlines_substitutions: true,
                        ..*options
                    },
                )?;
                writeln!(w, "</pre>")?;
                writeln!(w, "</div>")?;
                writeln!(w, "</div>")?;
            }
            DelimitedBlockType::DelimitedLiteral(inlines) => {
                if let Some(style) = &self.metadata.style {
                    writeln!(w, "<div class=\"{style}block\">")?;
                } else {
                    writeln!(w, "<div class=\"literalblock\">")?;
                }
                write!(w, "<div class=\"title\">")?;
                crate::inlines::render_inlines(&self.title, w, processor, options)?;
                writeln!(w, "</div>")?;
                writeln!(w, "<div class=\"content\">")?;
                writeln!(w, "<pre>")?;
                crate::inlines::render_inlines(
                    inlines,
                    w,
                    processor,
                    &RenderOptions {
                        inlines_substitutions: true,
                        ..*options
                    },
                )?;
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
            unknown => todo!("Unknown delimited block type: {:?}", unknown),
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
    fn test_listing_block_with_style_attribute() {
        let metadata = BlockMetadata {
            style: Some("source".to_string()),
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
            html.contains("<div class=\"sourceblock\">"),
            "Expected listing block with style to render with custom class, got: {html}",
        );
        assert!(
            !html.contains("<div class=\"listingblock\">"),
            "Listing block with style should not have default 'listingblock' class"
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
