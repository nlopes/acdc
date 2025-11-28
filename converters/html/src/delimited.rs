use std::io::Write;

use acdc_converters_common::{
    code::detect_language,
    visitor::{WritableVisitor, WritableVisitorExt},
};
use acdc_parser::{
    AttributeValue, Block, BlockMetadata, DelimitedBlock, DelimitedBlockType, InlineNode,
    StemContent, StemNotation,
};

use crate::{Error, Processor, RenderOptions};

fn write_example_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    block: &DelimitedBlock,
    processor: &Processor,
    blocks: &[Block],
) -> Result<(), Error> {
    let mut writer = visitor.writer_mut();
    writeln!(writer, "<div class=\"exampleblock\">")?;
    let _ = writer;

    // Render title with caption prefix if title exists
    if !block.title.is_empty() {
        let count = processor.example_counter.get() + 1;
        processor.example_counter.set(count);
        let caption = processor
            .document_attributes
            .get("example-caption")
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s.as_str()),
                AttributeValue::Bool(_) | AttributeValue::None | AttributeValue::Inlines(_) => None,
            })
            .unwrap_or("Example");
        let prefix = format!("<div class=\"title\">{caption} {count}. ");
        visitor.render_title_with_wrapper(&block.title, &prefix, "</div>\n")?;
    }

    writer = visitor.writer_mut();
    writeln!(writer, "<div class=\"content\">")?;
    let _ = writer;
    for nested_block in blocks {
        visitor.visit_block(nested_block)?;
    }
    writer = visitor.writer_mut();
    writeln!(writer, "</div>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}

/// Visit a delimited block using the visitor pattern with ability to walk nested blocks
pub(crate) fn visit_delimited_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    block: &DelimitedBlock,
    processor: &Processor,
    options: &RenderOptions,
) -> Result<(), Error> {
    match &block.inner {
        DelimitedBlockType::DelimitedQuote(blocks) => {
            let mut writer = visitor.writer_mut();
            if let Some(style) = &block.metadata.style {
                writeln!(writer, "<div class=\"{style}block\">")?;
            } else {
                writeln!(writer, "<div class=\"quoteblock\">")?;
            }
            writeln!(writer, "<blockquote>")?;
            let _ = writer;
            for nested_block in blocks {
                visitor.visit_block(nested_block)?;
            }
            writer = visitor.writer_mut();
            writeln!(writer, "</blockquote>")?;

            // Extract author and cite from named attributes.
            //
            // Parser extracts [quote, attribution(author), citation] into "attribution"
            // and "citation" attributes
            let author = block
                .metadata
                .attributes
                .get("attribution")
                .and_then(|v| match v {
                    AttributeValue::String(s) => Some(s.as_str()),
                    AttributeValue::Bool(_) | AttributeValue::None | AttributeValue::Inlines(_) => {
                        None
                    }
                });
            let cite = block
                .metadata
                .attributes
                .get("citation")
                .and_then(|v| match v {
                    AttributeValue::String(s) => Some(s.as_str()),
                    AttributeValue::Bool(_) | AttributeValue::None | AttributeValue::Inlines(_) => {
                        None
                    }
                });

            if author.is_some() || cite.is_some() {
                writeln!(writer, "<div class=\"attribution\">")?;
                if let Some(author) = author {
                    writeln!(writer, "&#8212; {author}<br>")?;
                }
                if let Some(cite) = cite {
                    writeln!(writer, "<cite>{cite}</cite>")?;
                }
                writeln!(writer, "</div>")?;
            }

            writeln!(writer, "</div>")?;
        }
        DelimitedBlockType::DelimitedOpen(blocks) => {
            let mut writer = visitor.writer_mut();
            writeln!(writer, "<div class=\"openblock\">")?;
            let _ = writer;
            visitor.render_title_with_wrapper(&block.title, "<div class=\"title\">", "</div>\n")?;
            writer = visitor.writer_mut();
            writeln!(writer, "<div class=\"content\">")?;
            let _ = writer;
            for nested_block in blocks {
                visitor.visit_block(nested_block)?;
            }
            writer = visitor.writer_mut();
            writeln!(writer, "</div>")?;
            writeln!(writer, "</div>")?;
        }
        DelimitedBlockType::DelimitedExample(blocks) => {
            write_example_block(visitor, block, processor, blocks)?;
        }
        DelimitedBlockType::DelimitedSidebar(blocks) => {
            let mut writer = visitor.writer_mut();
            writeln!(writer, "<div class=\"sidebarblock\">")?;
            writeln!(writer, "<div class=\"content\">")?;
            let _ = writer;
            visitor.render_title_with_wrapper(&block.title, "<div class=\"title\">", "</div>\n")?;
            writer = visitor.writer_mut();
            let _ = writer;
            for nested_block in blocks {
                visitor.visit_block(nested_block)?;
            }
            writer = visitor.writer_mut();
            writeln!(writer, "</div>")?;
            writeln!(writer, "</div>")?;
        }
        // Handle tables
        DelimitedBlockType::DelimitedTable(t) => {
            crate::table::render_table(
                t,
                visitor,
                processor,
                options,
                &block.metadata,
                &block.title,
            )?;
        }
        // For other block types, use the regular rendering
        DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedStem(_)
        | DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | _ => {
            render_delimited_block_inner(
                &block.inner,
                &block.title,
                &block.metadata,
                visitor,
                processor,
                options,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn render_delimited_block_inner<V: WritableVisitor<Error = Error>>(
    inner: &DelimitedBlockType,
    title: &[InlineNode],
    metadata: &BlockMetadata,
    visitor: &mut V,
    processor: &Processor,
    _options: &RenderOptions,
) -> Result<(), Error> {
    match inner {
        DelimitedBlockType::DelimitedPass(inlines) => {
            visitor.visit_inline_nodes(inlines)?;
        }
        DelimitedBlockType::DelimitedListing(inlines) => {
            let mut w = visitor.writer_mut();
            writeln!(w, "<div class=\"listingblock\">")?;
            let _ = w;

            // Check if listing-caption is set and block has a title
            if !title.is_empty() {
                if let Some(AttributeValue::String(caption)) =
                    processor.document_attributes.get("listing-caption")
                {
                    let count = processor.listing_counter.get() + 1;
                    processor.listing_counter.set(count);
                    visitor.render_title_with_wrapper(
                        title,
                        &format!("<div class=\"title\">{caption} {count}. "),
                        "</div>\n",
                    )?;
                } else {
                    // No listing-caption, render title without numbering
                    visitor.render_title_with_wrapper(
                        title,
                        "<div class=\"title\">",
                        "</div>\n",
                    )?;
                }
            }

            w = visitor.writer_mut();
            writeln!(w, "<div class=\"content\">")?;
            // Check if this is a source block with a language
            // The language is the first positional attribute (after style), which gets moved to attributes map
            let language = detect_language(metadata);
            if let Some(lang) = language {
                write!(
                    w,
                    "<pre class=\"highlight\"><code class=\"language-{lang}\" data-lang=\"{lang}\">"
                )?;
            } else {
                write!(w, "<pre>")?;
            }

            let _ = w;
            visitor.visit_inline_nodes(inlines)?;

            w = visitor.writer_mut();
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
            let mut w = visitor.writer_mut();
            if let Some(style) = &metadata.style
                && style != "source"
            {
                writeln!(w, "<div class=\"{style}block\">")?;
            } else {
                writeln!(w, "<div class=\"literalblock\">")?;
            }
            let _ = w;
            visitor.render_title_with_wrapper(title, "<div class=\"title\">", "</div>\n")?;
            w = visitor.writer_mut();
            writeln!(w, "<div class=\"content\">")?;
            write!(w, "<pre>")?;
            let _ = w;
            visitor.visit_inline_nodes(inlines)?;
            w = visitor.writer_mut();
            writeln!(w, "</pre>")?;
            writeln!(w, "</div>")?;
            writeln!(w, "</div>")?;
        }
        DelimitedBlockType::DelimitedStem(stem) => {
            let mut w = visitor.writer_mut();
            writeln!(w, "<div class=\"stemblock\">")?;
            let _ = w;
            visitor.render_title_with_wrapper(title, "<div class=\"title\">", "</div>\n")?;
            w = visitor.writer_mut();
            render_stem_content(stem, w)?;
            writeln!(w, "</div>")?;
        }
        DelimitedBlockType::DelimitedComment(_) => {
            // Comment blocks produce no output
        }
        DelimitedBlockType::DelimitedVerse(inlines) => {
            let mut w = visitor.writer_mut();
            writeln!(w, "<div class=\"verseblock\">")?;
            let _ = w;
            visitor.render_title_with_wrapper(title, "<div class=\"title\">", "</div>\n")?;
            w = visitor.writer_mut();
            write!(w, "<pre class=\"content\">")?;
            let _ = w;
            visitor.visit_inline_nodes(inlines)?;
            w = visitor.writer_mut();
            writeln!(w, "</pre>")?;

            // Extract author and cite from named attributes
            //
            // Parser extracts [verse, attribution(author), citation] into "attribution"
            // and "citation" attributes
            let author = metadata
                .attributes
                .get("attribution")
                .and_then(|v| match v {
                    AttributeValue::String(s) => Some(s.as_str()),
                    AttributeValue::Bool(_) | AttributeValue::None | AttributeValue::Inlines(_) => {
                        None
                    }
                });
            let citation = metadata.attributes.get("citation").and_then(|v| match v {
                AttributeValue::String(s) => Some(s.as_str()),
                AttributeValue::Bool(_) | AttributeValue::None | AttributeValue::Inlines(_) => None,
            });

            if author.is_some() || citation.is_some() {
                writeln!(w, "<div class=\"attribution\">")?;
                if let Some(author) = author {
                    writeln!(w, "&#8212; {author}<br>")?;
                }
                if let Some(citation) = citation {
                    writeln!(w, "<cite>{citation}</cite>")?;
                }
                writeln!(w, "</div>")?;
            }
            writeln!(w, "</div>")?;
        }
        DelimitedBlockType::DelimitedQuote(_)
        | DelimitedBlockType::DelimitedOpen(_)
        | DelimitedBlockType::DelimitedExample(_)
        | DelimitedBlockType::DelimitedSidebar(_)
        | DelimitedBlockType::DelimitedTable(_)
        | _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Unsupported delimited block type: {inner:?}"),
            )
            .into());
        }
    }
    Ok(())
}

fn render_stem_content<W: Write + ?Sized>(stem: &StemContent, w: &mut W) -> Result<(), Error> {
    writeln!(w, "<div class=\"content\">")?;
    match stem.notation {
        StemNotation::Latexmath => {
            write!(w, "\\[{}\\]", stem.content)?;
        }
        StemNotation::Asciimath => {
            write!(w, "\\${}\\$", stem.content)?;
        }
    }
    writeln!(w, "</div>")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_converters_common::{GeneratorMetadata, Options, visitor::Visitor};
    use acdc_core::{Doctype, SafeMode};
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
            safe_mode: SafeMode::Unsafe,
            timings: false,
        };
        let document_attributes = DocumentAttributes::default();
        Processor {
            options,
            document_attributes,
            toc_entries: Vec::new(),
            example_counter: std::rc::Rc::new(std::cell::Cell::new(0)),
            table_counter: std::rc::Rc::new(std::cell::Cell::new(0)),
            figure_counter: std::rc::Rc::new(std::cell::Cell::new(0)),
            listing_counter: std::rc::Rc::new(std::cell::Cell::new(0)),
        }
    }

    #[test]
    fn test_listing_block_renders_as_listingblock() -> Result<(), Error> {
        let block = DelimitedBlock {
            metadata: BlockMetadata::default(),
            inner: DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            delimiter: "----".to_string(),
            title: Vec::new(),
            location: Location::default(),
        };

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div class=\"listingblock\">"),
            "Expected listing block to render with 'listingblock' class, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_literal_block_renders_as_literalblock() -> Result<(), Error> {
        let block = DelimitedBlock {
            metadata: BlockMetadata::default(),
            inner: DelimitedBlockType::DelimitedLiteral(create_test_inlines("literal text")),
            delimiter: "....".to_string(),
            title: Vec::new(),
            location: Location::default(),
        };

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div class=\"literalblock\">"),
            "Expected literal block to render with 'literalblock' class, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_listing_block_with_source_style_and_language() -> Result<(), Error> {
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

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

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
        Ok(())
    }

    #[test]
    fn test_literal_block_with_style_attribute() -> Result<(), Error> {
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

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div class=\"verseblock\">"),
            "Expected literal block with style to render with custom class, got: {html}",
        );
        assert!(
            !html.contains("<div class=\"literalblock\">"),
            "Literal block with style should not have default 'literalblock' class"
        );
        Ok(())
    }

    #[test]
    fn test_listing_block_without_listing_caption_renders_title_without_number() -> Result<(), Error>
    {
        let title = vec![InlineNode::PlainText(Plain {
            content: "My Code Example".to_string(),
            location: Location::default(),
        })];

        let block = DelimitedBlock {
            metadata: BlockMetadata::default(),
            inner: DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            delimiter: "----".to_string(),
            title,
            location: Location::default(),
        };

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor.clone(), options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div class=\"title\">My Code Example</div>"),
            "Expected title without numbering when listing-caption is not set, got: {html}",
        );
        assert!(
            !html.contains("Listing 1"),
            "Should not contain numbering when listing-caption is not set, got: {html}",
        );
        assert_eq!(
            processor.listing_counter.get(),
            0,
            "Counter should not be incremented when listing-caption is not set"
        );
        Ok(())
    }

    #[test]
    fn test_listing_block_with_listing_caption_renders_title_with_number() -> Result<(), Error> {
        use acdc_parser::AttributeValue;

        let title1 = vec![InlineNode::PlainText(Plain {
            content: "First Example".to_string(),
            location: Location::default(),
        })];

        let title2 = vec![InlineNode::PlainText(Plain {
            content: "Second Example".to_string(),
            location: Location::default(),
        })];

        let block1 = DelimitedBlock {
            metadata: BlockMetadata::default(),
            inner: DelimitedBlockType::DelimitedListing(create_test_inlines("code 1")),
            delimiter: "----".to_string(),
            title: title1,
            location: Location::default(),
        };

        let block2 = DelimitedBlock {
            metadata: BlockMetadata::default(),
            inner: DelimitedBlockType::DelimitedListing(create_test_inlines("code 2")),
            delimiter: "----".to_string(),
            title: title2,
            location: Location::default(),
        };

        let output = Vec::new();
        let mut processor = create_test_processor();
        // Set listing-caption attribute
        processor.document_attributes.set(
            "listing-caption".to_string(),
            AttributeValue::String("Listing".to_string()),
        );

        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor.clone(), options.clone());

        // Render first block
        visitor.visit_delimited_block(&block1)?;
        let html1 = String::from_utf8(visitor.into_writer())?;

        assert!(
            html1.contains("<div class=\"title\">Listing 1. First Example</div>"),
            "Expected numbered title for first listing block, got: {html1}",
        );
        assert_eq!(
            processor.listing_counter.get(),
            1,
            "Counter should be incremented to 1"
        );

        // Render second block
        let output2 = Vec::new();
        let mut visitor2 = crate::HtmlVisitor::new(output2, processor.clone(), options);
        visitor2.visit_delimited_block(&block2)?;
        let html2 = String::from_utf8(visitor2.into_writer())?;

        assert!(
            html2.contains("<div class=\"title\">Listing 2. Second Example</div>"),
            "Expected numbered title for second listing block, got: {html2}",
        );
        assert_eq!(
            processor.listing_counter.get(),
            2,
            "Counter should be incremented to 2"
        );
        Ok(())
    }
}
