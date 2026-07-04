use acdc_parser::{
    AttributeValue, Block, DelimitedBlock, DelimitedBlockType, Document, DocumentAttribute,
    SourceLocation, Table, TableRow, strip_quotes,
};

use crate::LintId;

use super::{LintEmitter, SourceLine, line_range_for_location, source_lines_for_range};

pub(crate) fn lint_attribute_url_prefix(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    lines: &[SourceLine<'_>],
) {
    lint_header_attribute_url_prefix(emitter, document, lines);
    lint_attribute_url_prefix_blocks(emitter, &document.blocks);
}

fn lint_header_attribute_url_prefix(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    lines: &[SourceLine<'_>],
) {
    let Some(header) = &document.header else {
        return;
    };
    let Some(range) = line_range_for_location(&header.location) else {
        return;
    };
    for line in source_lines_for_range(lines, range) {
        let Some(attribute) = parse_attribute_line(line.text.trim()) else {
            continue;
        };
        lint_url_attribute(
            emitter,
            attribute.name,
            attribute.value,
            emitter.point_location(line.number, 1),
        );
    }
}

fn lint_attribute_url_prefix_blocks(emitter: &mut LintEmitter<'_>, blocks: &[Block<'_>]) {
    for block in blocks {
        match block {
            Block::Admonition(block) => lint_attribute_url_prefix_blocks(emitter, &block.blocks),
            Block::CalloutList(list) => {
                for item in &list.items {
                    lint_attribute_url_prefix_blocks(emitter, &item.blocks);
                }
            }
            Block::DescriptionList(list) => {
                for item in &list.items {
                    lint_attribute_url_prefix_blocks(emitter, &item.description);
                }
            }
            Block::DelimitedBlock(block) => {
                lint_attribute_url_prefix_delimited_block(emitter, block);
            }
            Block::DocumentAttribute(attribute) => {
                lint_document_attribute_url_prefix(emitter, attribute);
            }
            Block::OrderedList(list) => {
                for item in &list.items {
                    lint_attribute_url_prefix_blocks(emitter, &item.blocks);
                }
            }
            Block::Section(section) => lint_attribute_url_prefix_blocks(emitter, &section.content),
            Block::UnorderedList(list) => {
                for item in &list.items {
                    lint_attribute_url_prefix_blocks(emitter, &item.blocks);
                }
            }
            Block::Audio(_)
            | Block::Comment(_)
            | Block::DiscreteHeader(_)
            | Block::Image(_)
            | Block::PageBreak(_)
            | Block::Paragraph(_)
            | Block::TableOfContents(_)
            | Block::ThematicBreak(_)
            | Block::Video(_)
            | _ => {}
        }
    }
}

fn lint_attribute_url_prefix_delimited_block(
    emitter: &mut LintEmitter<'_>,
    block: &DelimitedBlock<'_>,
) {
    match &block.inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks) => {
            lint_attribute_url_prefix_blocks(emitter, blocks);
        }
        DelimitedBlockType::DelimitedTable(table) => {
            lint_attribute_url_prefix_table(emitter, table);
        }
        DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedStem(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | _ => {}
    }
}

fn lint_attribute_url_prefix_table(emitter: &mut LintEmitter<'_>, table: &Table<'_>) {
    if let Some(header) = &table.header {
        lint_attribute_url_prefix_table_row(emitter, header);
    }
    for row in &table.rows {
        lint_attribute_url_prefix_table_row(emitter, row);
    }
    if let Some(footer) = &table.footer {
        lint_attribute_url_prefix_table_row(emitter, footer);
    }
}

fn lint_attribute_url_prefix_table_row(emitter: &mut LintEmitter<'_>, row: &TableRow<'_>) {
    for column in &row.columns {
        lint_attribute_url_prefix_blocks(emitter, &column.content);
    }
}

fn lint_document_attribute_url_prefix(
    emitter: &mut LintEmitter<'_>,
    attribute: &DocumentAttribute<'_>,
) {
    let AttributeValue::String(value) = &attribute.value else {
        return;
    };
    lint_url_attribute(
        emitter,
        attribute.name.as_ref(),
        value.as_ref(),
        emitter.source_location(&attribute.location),
    );
}

fn lint_url_attribute(
    emitter: &mut LintEmitter<'_>,
    name: &str,
    value: &str,
    location: SourceLocation,
) {
    let value = strip_quotes(value.trim());
    if !is_url_value(value) || name.starts_with("url-") || name.starts_with("uri-") {
        return;
    }
    emitter.emit(
        LintId::AttributeUrlPrefix,
        format!("URL-valued attribute `{name}` should use a url- or uri- prefix"),
        Some(format!("rename `{name}` to `url-{name}`")),
        Some(location),
    );
}

struct AttributeLine<'a> {
    name: &'a str,
    value: &'a str,
}

fn parse_attribute_line(trimmed: &str) -> Option<AttributeLine<'_>> {
    let rest = trimmed.strip_prefix(':')?;
    let (name, value) = rest.split_once(':')?;
    if name.is_empty() || name.starts_with('!') || name.ends_with('!') {
        return None;
    }
    let value = value.strip_prefix(' ').unwrap_or(value).trim();
    (!value.is_empty()).then_some(AttributeLine { name, value })
}

fn is_url_value(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && scheme
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
        && scheme
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
        && (rest.starts_with("//") || matches!(scheme, "mailto" | "urn"))
}

#[cfg(test)]
mod tests {
    use crate::{Error, LintId};

    use super::super::test_support::{has_lint, report_for};

    #[test]
    fn attribute_url_prefix_flags_url_values_without_prefix() -> Result<(), Error> {
        let report = report_for("= Title\n\n:docs: https://example.com\n\nContent.\n")?;

        assert!(has_lint(&report, LintId::AttributeUrlPrefix));
        Ok(())
    }

    #[test]
    fn attribute_url_prefix_flags_header_attributes() -> Result<(), Error> {
        let report = report_for("= Title\n:docs: https://example.com\n\nContent.\n")?;

        assert!(has_lint(&report, LintId::AttributeUrlPrefix));
        Ok(())
    }

    #[test]
    fn attribute_url_prefix_allows_url_prefixed_names() -> Result<(), Error> {
        let report = report_for("= Title\n\n:url-docs: https://example.com\n\nContent.\n")?;

        assert!(!has_lint(&report, LintId::AttributeUrlPrefix));
        Ok(())
    }

    #[test]
    fn attribute_url_prefix_ignores_attributes_inside_listing() -> Result<(), Error> {
        let report = report_for("= Title\n\n----\n:docs: https://example.com\n----\n")?;

        assert!(!has_lint(&report, LintId::AttributeUrlPrefix));
        Ok(())
    }
}
