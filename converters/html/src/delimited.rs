use std::io::Write;

use acdc_converters_common::{
    code::detect_language,
    visitor::{WritableVisitor, WritableVisitorExt},
};
use acdc_parser::{
    BlockMetadata, DelimitedBlock, DelimitedBlockType, InlineNode, StemContent, StemNotation,
};

use crate::{Error, Processor, RenderOptions};

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
            let mut writer = visitor.writer_mut();
            writeln!(writer, "<div class=\"exampleblock\">")?;
            let _ = writer;

            // Render title with "Example N." prefix if title exists
            if !block.title.is_empty() {
                let count = processor.example_counter.get() + 1;
                processor.example_counter.set(count);
                let prefix = format!("<div class=\"title\">Example {count}. ");
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
        _ => {
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
    _processor: &Processor,
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
            visitor.render_title_with_wrapper(title, "<div class=\"title\">", "</div>\n")?;
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
            writeln!(w, "<div class=\"attribution\">")?;

            // Extract author and cite from positional attributes
            // [verse, author, cite] -> positional_attributes[0] = author, [1] = cite
            let author = metadata.positional_attributes.first();
            let cite = metadata.positional_attributes.get(1);

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

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block).unwrap();
        let html = String::from_utf8(visitor.into_writer()).unwrap();

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

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block).unwrap();
        let html = String::from_utf8(visitor.into_writer()).unwrap();

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

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block).unwrap();
        let html = String::from_utf8(visitor.into_writer()).unwrap();

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

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block).unwrap();
        let html = String::from_utf8(visitor.into_writer()).unwrap();

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
