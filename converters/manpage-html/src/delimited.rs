use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{DelimitedBlock, DelimitedBlockType};

use crate::{
    Error, ManpageHtmlVisitor,
    escape::{escape_html, extract_plain_text},
};

pub(crate) fn visit_delimited_block<W: Write>(
    block: &DelimitedBlock,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    if !block.title.is_empty() {
        write!(visitor.writer_mut(), "<p class=\"Pp\"><b>")?;
        visitor.visit_inline_nodes(&block.title)?;
        write!(visitor.writer_mut(), "</b></p>")?;
    }

    match &block.inner {
        DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines) => {
            let content = extract_plain_text(inlines);
            let escaped = escape_html(&content);
            write!(visitor.writer_mut(), "<pre class=\"Li\">{escaped}</pre>")?;
            Ok(())
        }

        DelimitedBlockType::DelimitedExample(blocks) => {
            write!(visitor.writer_mut(), "<div class=\"Bd-indent example\">")?;
            for nested_block in blocks {
                visitor.visit_block(nested_block)?;
            }
            write!(visitor.writer_mut(), "</div>")?;
            Ok(())
        }

        DelimitedBlockType::DelimitedSidebar(blocks) => {
            write!(visitor.writer_mut(), "<div class=\"Bd-indent sidebar\">")?;
            for nested_block in blocks {
                visitor.visit_block(nested_block)?;
            }
            write!(visitor.writer_mut(), "</div>")?;
            Ok(())
        }

        DelimitedBlockType::DelimitedOpen(blocks) => {
            for nested_block in blocks {
                visitor.visit_block(nested_block)?;
            }
            Ok(())
        }

        DelimitedBlockType::DelimitedQuote(blocks) => {
            write!(visitor.writer_mut(), "<blockquote class=\"Bd-indent\">")?;
            for nested_block in blocks {
                visitor.visit_block(nested_block)?;
            }
            write!(visitor.writer_mut(), "</blockquote>")?;

            let attribution = block.metadata.attributes.get_string("attribution");
            let citation = block.metadata.attributes.get_string("citation");
            if attribution.is_some() || citation.is_some() {
                write!(visitor.writer_mut(), "<footer class=\"attribution\">")?;
                if let Some(cite) = citation {
                    write!(visitor.writer_mut(), "{} ", escape_html(&cite))?;
                }
                if let Some(author) = attribution {
                    write!(visitor.writer_mut(), "&mdash; {}", escape_html(&author))?;
                }
                write!(visitor.writer_mut(), "</footer>")?;
            }

            Ok(())
        }

        DelimitedBlockType::DelimitedVerse(inlines) => {
            let content = extract_plain_text(inlines);
            let escaped = escape_html(&content);
            write!(visitor.writer_mut(), "<pre class=\"verse\">{escaped}</pre>")?;

            let attribution = block.metadata.attributes.get_string("attribution");
            let citation = block.metadata.attributes.get_string("citation");
            if attribution.is_some() || citation.is_some() {
                write!(visitor.writer_mut(), "<footer class=\"verse-footer\">")?;
                if let Some(cite) = citation {
                    write!(visitor.writer_mut(), "{} ", escape_html(&cite))?;
                }
                if let Some(author) = attribution {
                    write!(visitor.writer_mut(), "&mdash; {}", escape_html(&author))?;
                }
                write!(visitor.writer_mut(), "</footer>")?;
            }

            Ok(())
        }

        DelimitedBlockType::DelimitedPass(inlines) => {
            let content = extract_plain_text(inlines);
            write!(visitor.writer_mut(), "{content}")?;
            Ok(())
        }

        DelimitedBlockType::DelimitedTable(table) => {
            crate::table::visit_table(table, block, visitor)
        }

        DelimitedBlockType::DelimitedStem(stem) => {
            write!(
                visitor.writer_mut(),
                "<pre class=\"stem\">{}</pre>",
                escape_html(&stem.content)
            )?;
            Ok(())
        }

        DelimitedBlockType::DelimitedComment(_) | _ => Ok(()),
    }
}
