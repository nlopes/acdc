//! Visitor pattern for traversing `AsciiDoc` AST.
//!
//! This module provides a Visitor trait that defines methods for visiting each type of AST node.
//! Converters implement this trait to define how to process each node type.
//!
//! # Naming Conventions
//!
//! The codebase follows a two-tier naming pattern for converter functions:
//!
//! - **`visit_*` functions**: High-level orchestration functions that accept a visitor reference
//!   and traverse the AST structure. These functions call `visitor.visit_block()`,
//!   `visitor.visit_inline_nodes()`, or other visitor methods to handle nested content.
//!   They are entry points for processing complete AST nodes.
//!
//! - **`render_*` functions**: Low-level helper functions that generate specific markup output.
//!   These functions typically write directly to a `Write` trait object (not a full visitor)
//!   and handle specific formatting concerns. They are implementation details focused purely
//!   on output generation.
//!
//! This distinction keeps the visitor pattern semantics clear: `visit_*` functions orchestrate
//! traversal and structure, while `render_*` functions focus on output formatting.

use std::io::Write;

use acdc_parser::{
    Admonition, Audio, Block, CalloutList, CalloutListItem, CalloutRef, DelimitedBlock,
    DescriptionList, DiscreteHeader, Document, Header, Image, InlineNode, ListItem, OrderedList,
    PageBreak, Paragraph, Section, TableOfContents, ThematicBreak, UnorderedList, Video,
};

/// The Visitor trait defines methods for visiting each type of AST node.
///
/// Converters implement this trait to define how to process each node type.
///
/// The module provides default traversal logic that calls these methods following the
/// `AsciiDoc` document structure.
///
/// # Document Structure
///
/// The `visit_document` method calls visitors in this order (per `AsciiDoc` spec):
///
/// 1. `visit_document_start()` - document setup
/// 2. `visit_header()` - if header present
/// 3. Walk preamble blocks (blocks before first section)
/// 4. `visit_preamble_end()` - after preamble blocks walked
/// 5. Walk remaining blocks (sections and top-level blocks)
/// 6. `visit_document_supplements()` - footnotes, bibliography, etc.
/// 7. `visit_document_end()` - document cleanup
///
/// All structural hooks (`visit_document_*`, `visit_preamble_end`) have default
/// no-op implementations. Simple converters only implement block/inline visitors.
pub trait Visitor {
    /// The error type that can be returned during visiting
    type Error;

    /// Called before any document processing begins.
    ///
    /// Use for: document setup, inspecting structure, opening document wrappers.
    /// Example: HTML converter writes `<!DOCTYPE html><html><head>` here.
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error, but custom implementations
    /// may return errors during document processing.
    fn visit_document_start(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called before any body content blocks are walked.
    ///
    /// Called after header (if present), before any blocks (preamble or sections).
    /// Use for: opening content wrappers that contain all body blocks.
    /// Example: HTML converter opens `<div id="content">`.
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error, but custom implementations
    /// may return errors during document processing.
    fn visit_body_content_start(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called before preamble blocks are walked (if preamble exists).
    ///
    /// Use for: opening preamble wrappers, special preamble setup.
    /// Example: HTML converter opens preamble divs.
    /// Note: Only called if preamble blocks exist (blocks before first section).
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error, but custom implementations
    /// may return errors during document processing.
    fn visit_preamble_start(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called after preamble blocks are walked (if preamble existed).
    ///
    /// Use for: closing preamble wrappers, adding content after preamble.
    /// Example: HTML converter closes preamble divs and adds TOC if configured.
    /// Note: Only called if preamble blocks existed.
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error, but custom implementations
    /// may return errors during document processing.
    fn visit_preamble_end(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called after all blocks processed, before document end.
    ///
    /// Use for: document supplements like footnotes, bibliography, appendices.
    /// Example: HTML converter renders footnotes and footer.
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error, but custom implementations
    /// may return errors during document processing.
    fn visit_document_supplements(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called at document end, after all processing complete.
    ///
    /// Use for: closing document wrappers, final cleanup.
    /// Example: HTML converter writes `</body></html>`.
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error, but custom implementations
    /// may return errors during document processing.
    fn visit_document_end(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Visit a complete document.
    ///
    /// This is the main entry point for traversing an `AsciiDoc` document.
    /// It orchestrates the document structure per `AsciiDoc` spec:
    /// - Document start (setup)
    /// - Header (optional)
    /// - Preamble (blocks before first section)
    /// - Sections and top-level blocks
    /// - Document supplements (footnotes, etc.)
    /// - Document end (cleanup)
    ///
    /// The default implementation calls visitor hooks at appropriate structural points,
    /// allowing converters to handle document framing, metadata, and supplements.
    ///
    /// # Errors
    ///
    /// Returns an error if any visitor method fails during document traversal.
    fn visit_document(&mut self, doc: &Document) -> Result<(), Self::Error> {
        // 1. Document start - setup, document wrappers
        self.visit_document_start(doc)?;

        // 2. Header - title, authors, metadata (if present)
        if let Some(header) = &doc.header {
            self.visit_header(header)?;
        }

        // 3. Body content start - before any blocks
        self.visit_body_content_start(doc)?;

        // 4. Identify preamble (blocks before first section) per AsciiDoc spec
        let first_section_idx = doc
            .blocks
            .iter()
            .position(|b| matches!(b, Block::Section(_)));

        let (preamble, remaining) = match first_section_idx {
            Some(idx) => doc.blocks.split_at(idx),
            None => (doc.blocks.as_slice(), &[][..]),
        };

        // Check if preamble has substantive content (not just comments/attributes)
        let has_substantive_preamble = preamble
            .iter()
            .any(|b| !matches!(b, Block::Comment(_) | Block::DocumentAttribute(_)));

        // Preamble wrapper is only emitted when ALL conditions are met:
        // 1. Document has a header (title)
        // 2. There is at least one section
        // 3. There's substantive content before that section
        let emit_preamble =
            doc.header.is_some() && first_section_idx.is_some() && has_substantive_preamble;

        // 5. Walk preamble blocks
        if emit_preamble {
            self.visit_preamble_start(doc)?;
        }

        for block in preamble {
            self.visit_block(block)?;
        }

        if emit_preamble {
            self.visit_preamble_end(doc)?;
        }

        // 6. Walk remaining blocks (sections and other top-level blocks)
        for block in remaining {
            self.visit_block(block)?;
        }

        // 7. Document supplements - footnotes, bibliography, etc.
        self.visit_document_supplements(doc)?;

        // 8. Document end - close wrappers, final cleanup
        self.visit_document_end(doc)?;

        Ok(())
    }

    /// Visit a generic block (delegates to specific block visitors)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this block fails.
    fn visit_block(&mut self, block: &Block) -> Result<(), Self::Error> {
        match block {
            Block::Section(section) => self.visit_section(section),
            Block::Paragraph(para) => self.visit_paragraph(para),
            Block::DelimitedBlock(delimited) => self.visit_delimited_block(delimited),
            Block::OrderedList(list) => self.visit_ordered_list(list),
            Block::UnorderedList(list) => self.visit_unordered_list(list),
            Block::DescriptionList(list) => self.visit_description_list(list),
            Block::CalloutList(list) => self.visit_callout_list(list),
            Block::Admonition(admon) => self.visit_admonition(admon),
            Block::Image(img) => self.visit_image(img),
            Block::Video(video) => self.visit_video(video),
            Block::Audio(audio) => self.visit_audio(audio),
            Block::ThematicBreak(br) => self.visit_thematic_break(br),
            Block::PageBreak(br) => self.visit_page_break(br),
            Block::TableOfContents(toc) => self.visit_table_of_contents(toc),
            Block::DiscreteHeader(header) => self.visit_discrete_header(header),
            // DocumentAttribute blocks are metadata and comments produce no output
            Block::DocumentAttribute(_) | Block::Comment(_) => Ok(()),
            // Handle any future block types (Block is marked non-exhaustive)
            _ => {
                // Default behavior: ignore unknown blocks
                tracing::warn!(?block, "Unexpected block");
                Ok(())
            }
        }
    }

    /// Visit the document header (title, authors, metadata).
    ///
    /// Called after `visit_document_start()`, before any blocks.
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of the header fails.
    fn visit_header(&mut self, header: &Header) -> Result<(), Self::Error>;

    /// Visit a section (heading with nested content)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this section fails.
    fn visit_section(&mut self, section: &Section) -> Result<(), Self::Error>;

    /// Visit a paragraph
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this paragraph fails.
    fn visit_paragraph(&mut self, para: &Paragraph) -> Result<(), Self::Error>;

    /// Visit a delimited block (listing, example, sidebar, table, etc.)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this delimited block fails.
    fn visit_delimited_block(&mut self, block: &DelimitedBlock) -> Result<(), Self::Error>;

    /// Visit an ordered (numbered) list
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this list fails.
    fn visit_ordered_list(&mut self, list: &OrderedList) -> Result<(), Self::Error>;

    /// Visit an unordered (bulleted) list
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this list fails.
    fn visit_unordered_list(&mut self, list: &UnorderedList) -> Result<(), Self::Error>;

    /// Visit a description list (term/definition pairs)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this list fails.
    fn visit_description_list(&mut self, list: &DescriptionList) -> Result<(), Self::Error>;

    /// Visit a callout list
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this list fails.
    fn visit_callout_list(&mut self, list: &CalloutList) -> Result<(), Self::Error>;

    /// Visit a list item
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this list item fails.
    fn visit_list_item(&mut self, item: &ListItem) -> Result<(), Self::Error>;

    /// Visit an admonition (NOTE, TIP, WARNING, etc.)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this admonition fails.
    fn visit_admonition(&mut self, admon: &Admonition) -> Result<(), Self::Error>;

    /// Visit an image
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this image fails.
    fn visit_image(&mut self, img: &Image) -> Result<(), Self::Error>;

    /// Visit a video
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this video fails.
    fn visit_video(&mut self, video: &Video) -> Result<(), Self::Error>;

    /// Visit an audio element
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this audio element fails.
    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error>;

    /// Visit a thematic break (horizontal rule)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this thematic break fails.
    fn visit_thematic_break(&mut self, br: &ThematicBreak) -> Result<(), Self::Error>;

    /// Visit a page break
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this page break fails.
    fn visit_page_break(&mut self, br: &PageBreak) -> Result<(), Self::Error>;

    /// Visit a table of contents
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this table of contents fails.
    fn visit_table_of_contents(&mut self, toc: &TableOfContents) -> Result<(), Self::Error>;

    /// Visit a discrete header (not part of document structure)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this discrete header fails.
    fn visit_discrete_header(&mut self, header: &DiscreteHeader) -> Result<(), Self::Error>;

    /// Visit a sequence of inline nodes
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of any inline node fails.
    fn visit_inline_nodes(&mut self, nodes: &[InlineNode]) -> Result<(), Self::Error>;

    /// Visit a single inline node
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this inline node fails.
    fn visit_inline_node(&mut self, node: &InlineNode) -> Result<(), Self::Error>;

    /// Visit plain text
    ///
    /// # Errors
    ///
    /// Returns an error if writing the text fails.
    fn visit_text(&mut self, text: &str) -> Result<(), Self::Error>;

    /// Visit a callout reference inline node.
    ///
    /// Default implementation does nothing. Override to render callout markers.
    ///
    /// # Errors
    ///
    /// Returns an error if writing the callout reference fails.
    fn visit_callout_ref(&mut self, _callout: &CalloutRef) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Visit a callout list item.
    ///
    /// Default implementation does nothing. Override to render callout list items.
    ///
    /// # Errors
    ///
    /// Returns an error if writing the callout list item fails.
    fn visit_callout_list_item(&mut self, _item: &CalloutListItem) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// A writable visitor that outputs to a writer.
///
/// This is a common pattern for converters that generate text output (HTML, terminal, etc.)
pub trait WritableVisitor: Visitor {
    /// Get a mutable reference to the writer
    fn writer_mut(&mut self) -> &mut dyn Write;
}

/// Extension trait for `WritableVisitor` that provides common rendering helpers.
///
/// This trait provides utility methods that handle common patterns across converters,
/// such as rendering titles with wrapper markup.
pub trait WritableVisitorExt: WritableVisitor {
    /// Render a title with wrapper markup (prefix and suffix).
    ///
    /// This helper handles the common pattern of:
    /// 1. Write opening markup
    /// 2. Drop the writer borrow
    /// 3. Visit inline nodes (which needs mutable visitor)
    /// 4. Get writer again
    /// 5. Write closing markup
    ///
    /// Does nothing if title is empty.
    ///
    /// # Errors
    ///
    /// Returns an error if writing or visiting fails.
    fn render_title_with_wrapper(
        &mut self,
        title: &[InlineNode],
        prefix: &str,
        suffix: &str,
    ) -> Result<(), Self::Error>
    where
        Self::Error: From<std::io::Error>,
    {
        if !title.is_empty() {
            let w = self.writer_mut();
            write!(w, "{prefix}")?;
            let _ = w;
            self.visit_inline_nodes(title)?;
            let w = self.writer_mut();
            write!(w, "{suffix}")?;
        }
        Ok(())
    }
}

// Blanket implementation for all WritableVisitor types
impl<T: WritableVisitor> WritableVisitorExt for T {}
