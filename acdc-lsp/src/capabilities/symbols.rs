//! Document outlines retain block hierarchy and primary-file source ranges.

use acdc_converters_core::InlineTextTransform;
use acdc_parser::ast::{
    AdmonitionRef, BlockKind, BlockRef, ContentRef, DocumentAttributeRef, DocumentRef,
    ParagraphRef, SectionRef, SourceRef, StructuralRef,
};
use tower_lsp_server::ls_types::{DocumentSymbol, Range, SymbolKind};

use crate::convert::{is_primary_location, location_to_utf16_range};

const PARAGRAPH_PREVIEW_LEN: usize = 50;

#[must_use]
pub(crate) fn document_symbols(doc: DocumentRef<'_>, source: &str) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    if let Some(header) = doc.header()
        && is_primary_location(header.location())
        && let Some(range) = location_to_utf16_range(source, &header.location().into())
    {
        symbols.push(make_symbol(
            title_text(header.title()),
            SymbolKind::FILE,
            range,
            Some("Document title".to_string()),
        ));
    }
    for block in doc.blocks() {
        collect_block_symbols(block, source, &mut symbols);
    }
    symbols
}

fn make_symbol(
    name: String,
    kind: SymbolKind,
    range: Range,
    detail: Option<String>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        kind,
        range,
        selection_range: range,
        detail,
        children: None,
        tags: Some(vec![]),
        deprecated: None,
    }
}

fn truncate(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_len).collect();
        format!("{truncated}...")
    }
}

pub(super) fn title_text(title: Option<ContentRef<'_>>) -> String {
    title.map_or_else(String::new, |content| {
        InlineTextTransform::default().content_to_string(content)
    })
}

fn title_or_default(title: Option<ContentRef<'_>>, default: &str) -> String {
    let text = title_text(title);
    if text.is_empty() {
        default.to_string()
    } else {
        text
    }
}

fn title_or_source(title: Option<ContentRef<'_>>, source: SourceRef<'_>) -> String {
    title.filter(|content| !content.is_empty()).map_or_else(
        || source.to_string(),
        |content| InlineTextTransform::default().content_to_string(content),
    )
}

fn document_attribute_name(attribute: DocumentAttributeRef<'_>) -> String {
    let mut value = String::new();
    if let Some(assigned) = attribute.assignment().value() {
        let _ = assigned.write_text(&mut value);
        format!(":{}: {value}", attribute.name())
    } else {
        format!(":{}!:", attribute.name())
    }
}

fn collect_block_symbols(block: BlockRef<'_>, source: &str, symbols: &mut Vec<DocumentSymbol>) {
    let mut children = Vec::new();
    for_each_symbol_child(block, |child| {
        collect_block_symbols(child, source, &mut children);
    });
    let location = if is_primary_location(block.location()) {
        Some(block.location().into())
    } else if let BlockKind::Section(section) = block.kind() {
        // A section can start here and end in an include. Keep its local heading.
        let heading = super::identity::heading_location(section);
        is_primary_location(&heading).then_some(heading)
    } else {
        None
    };
    if let Some(range) = location.and_then(|location| location_to_utf16_range(source, &location))
        && let Some(mut symbol) = block_to_symbol(block, range)
    {
        for child in &children {
            symbol.range.start = symbol.range.start.min(child.range.start);
            symbol.range.end = symbol.range.end.max(child.range.end);
        }
        symbol.children = (!children.is_empty()).then_some(children);
        symbols.push(symbol);
    } else {
        // Included containers can hold text that resumes in the primary file.
        symbols.extend(children);
    }
}

/// Symbols include ordinary list attachments, but not table cells or callout bodies.
pub(super) fn for_each_symbol_child<'doc>(
    block: BlockRef<'doc>,
    mut visit: impl FnMut(BlockRef<'doc>),
) {
    if matches!(
        block.kind(),
        BlockKind::Table(_) | BlockKind::CalloutList(_)
    ) {
        return;
    }
    for child in block.children() {
        match child {
            StructuralRef::Block(block) => visit(block),
            StructuralRef::ListItem(item) => item.blocks().for_each(&mut visit),
            StructuralRef::DescriptionListItem(item) => item.blocks().for_each(&mut visit),
            StructuralRef::Document(_)
            | StructuralRef::Header(_)
            | StructuralRef::CalloutListItem(_)
            | StructuralRef::TableRow(_)
            | StructuralRef::TableCell(_)
            | _ => {}
        }
    }
}

fn block_to_symbol(block: BlockRef<'_>, range: Range) -> Option<DocumentSymbol> {
    match block.kind() {
        BlockKind::Section(section) => Some(section_to_symbol(section, range)),
        BlockKind::Paragraph(para) => Some(paragraph_to_symbol(para, range)),
        BlockKind::Admonition(adm) => Some(admonition_to_symbol(adm, range)),
        BlockKind::UnorderedList(list) => Some(list_to_symbol(
            list.title(),
            "Unordered list",
            list.items().count(),
            range,
        )),
        BlockKind::OrderedList(list) => Some(list_to_symbol(
            list.title(),
            "Ordered list",
            list.items().count(),
            range,
        )),
        BlockKind::DescriptionList(list) => Some(list_to_symbol(
            list.title(),
            "Description list",
            list.items().count(),
            range,
        )),
        BlockKind::CalloutList(list) => Some(list_to_symbol(
            list.title(),
            "Callout list",
            list.items().count(),
            range,
        )),
        BlockKind::Image(image) => Some(make_symbol(
            title_or_source(image.title(), image.source()),
            SymbolKind::FILE,
            range,
            Some("Image".to_string()),
        )),
        BlockKind::Audio(audio) => Some(make_symbol(
            title_or_source(audio.title(), audio.source()),
            SymbolKind::FILE,
            range,
            Some("Audio".to_string()),
        )),
        BlockKind::Video(video) => Some(make_symbol(
            title_or_source(video.title(), video.source()),
            SymbolKind::FILE,
            range,
            Some("Video".to_string()),
        )),
        BlockKind::DiscreteHeader(header) => Some(make_symbol(
            title_text(header.title()),
            SymbolKind::KEY,
            range,
            Some(format!("Discrete heading (level {})", header.level())),
        )),
        BlockKind::DocumentAttribute(attr) => Some(make_symbol(
            document_attribute_name(attr),
            SymbolKind::PROPERTY,
            range,
            Some("Attribute".to_string()),
        )),
        BlockKind::TableOfContents(_) => Some(make_symbol(
            "Table of contents".to_string(),
            SymbolKind::STRUCT,
            range,
            None,
        )),
        BlockKind::ThematicBreak(_) => Some(make_symbol(
            "---".to_string(),
            SymbolKind::NULL,
            range,
            Some("Thematic break".to_string()),
        )),
        BlockKind::PageBreak(_) => Some(make_symbol(
            "<<<".to_string(),
            SymbolKind::NULL,
            range,
            Some("Page break".to_string()),
        )),
        BlockKind::Example(_)
        | BlockKind::Sidebar(_)
        | BlockKind::Open(_)
        | BlockKind::Quote(_)
        | BlockKind::Listing(_)
        | BlockKind::Literal(_)
        | BlockKind::Table(_)
        | BlockKind::Passthrough(_)
        | BlockKind::Verse(_)
        | BlockKind::Stem(_)
        | BlockKind::DelimitedComment(_) => Some(delimited_block_to_symbol(block, range)),
        BlockKind::Comment(_) | _ => None,
    }
}

fn paragraph_to_symbol(para: ParagraphRef<'_>, range: Range) -> DocumentSymbol {
    let name = para.title().filter(|title| !title.is_empty()).map_or_else(
        || {
            truncate(
                &InlineTextTransform::default().content_to_string(para.content()),
                PARAGRAPH_PREVIEW_LEN,
            )
        },
        |title| InlineTextTransform::default().content_to_string(title),
    );
    make_symbol(
        name,
        SymbolKind::STRING,
        range,
        Some("Paragraph".to_string()),
    )
}

fn admonition_to_symbol(adm: AdmonitionRef<'_>, range: Range) -> DocumentSymbol {
    let label = adm.variant().to_string().to_uppercase();
    let name = adm.title().filter(|title| !title.is_empty()).map_or_else(
        || label.clone(),
        |title| {
            format!(
                "{label}: {}",
                InlineTextTransform::default().content_to_string(title)
            )
        },
    );
    make_symbol(
        name,
        SymbolKind::EVENT,
        range,
        Some("Admonition".to_string()),
    )
}

fn list_to_symbol(
    title: Option<ContentRef<'_>>,
    default: &str,
    count: usize,
    range: Range,
) -> DocumentSymbol {
    make_symbol(
        title_or_default(title, default),
        SymbolKind::ARRAY,
        range,
        Some(format!("{count} items")),
    )
}

fn delimited_block_to_symbol(block: BlockRef<'_>, range: Range) -> DocumentSymbol {
    let (default_name, kind) = match block.kind() {
        BlockKind::Example(_) => ("Example", SymbolKind::STRUCT),
        BlockKind::Sidebar(_) => ("Sidebar", SymbolKind::STRUCT),
        BlockKind::Open(_) => ("Open block", SymbolKind::STRUCT),
        BlockKind::Quote(_) => ("Quote", SymbolKind::STRUCT),
        BlockKind::Listing(_) => ("Listing", SymbolKind::STRING),
        BlockKind::Literal(_) => ("Literal", SymbolKind::STRING),
        BlockKind::Table(_) => ("Table", SymbolKind::STRUCT),
        BlockKind::Passthrough(_) => ("Passthrough", SymbolKind::STRING),
        BlockKind::Verse(_) => ("Verse", SymbolKind::STRING),
        BlockKind::Stem(_) => ("Stem", SymbolKind::STRING),
        BlockKind::DelimitedComment(_)
        | BlockKind::TableOfContents(_)
        | BlockKind::Admonition(_)
        | BlockKind::DiscreteHeader(_)
        | BlockKind::DocumentAttribute(_)
        | BlockKind::ThematicBreak(_)
        | BlockKind::PageBreak(_)
        | BlockKind::UnorderedList(_)
        | BlockKind::OrderedList(_)
        | BlockKind::CalloutList(_)
        | BlockKind::DescriptionList(_)
        | BlockKind::Section(_)
        | BlockKind::Paragraph(_)
        | BlockKind::Image(_)
        | BlockKind::Audio(_)
        | BlockKind::Video(_)
        | BlockKind::Comment(_)
        | _ => ("Block", SymbolKind::STRING),
    };
    make_symbol(
        title_or_default(block.title(), default_name),
        kind,
        range,
        Some(default_name.to_string()),
    )
}

fn section_to_symbol(section: SectionRef<'_>, range: Range) -> DocumentSymbol {
    make_symbol(
        title_text(section.title()),
        section_level_to_symbol_kind(section.level()),
        range,
        Some(format!("Level {}", section.level())),
    )
}

pub(super) const fn section_level_to_symbol_kind(level: u8) -> SymbolKind {
    match level {
        0 => SymbolKind::NAMESPACE,
        1 => SymbolKind::MODULE,
        2 => SymbolKind::CLASS,
        3 => SymbolKind::METHOD,
        4 => SymbolKind::FUNCTION,
        _ => SymbolKind::VARIABLE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::{Error, Options, parse};

    #[test]
    fn symbol_kinds_cover_every_block_family() -> Result<(), Error> {
        for (source, name, kind) in [
            ("toc::[]", "Table of contents", SymbolKind::STRUCT),
            ("NOTE: Body.", "NOTE", SymbolKind::EVENT),
            ("[discrete]\n== Heading", "Heading", SymbolKind::KEY),
            (":custom: value", ":custom: value", SymbolKind::PROPERTY),
            ("'''", "---", SymbolKind::NULL),
            ("<<<", "<<<", SymbolKind::NULL),
            ("* Item", "Unordered list", SymbolKind::ARRAY),
            (". Item", "Ordered list", SymbolKind::ARRAY),
            ("Term:: Description", "Description list", SymbolKind::ARRAY),
            ("<1> Explanation", "Callout list", SymbolKind::ARRAY),
            ("== Section", "Section", SymbolKind::MODULE),
            ("Paragraph.", "Paragraph.", SymbolKind::STRING),
            ("image::bird.png[]", "bird.png", SymbolKind::FILE),
            ("audio::song.ogg[]", "song.ogg", SymbolKind::FILE),
            ("video::one,two[youtube]", "one,two", SymbolKind::FILE),
            ("====\nBody.\n====", "Example", SymbolKind::STRUCT),
            ("****\nBody.\n****", "Sidebar", SymbolKind::STRUCT),
            ("--\nBody.\n--", "Open block", SymbolKind::STRUCT),
            ("____\nBody.\n____", "Quote", SymbolKind::STRUCT),
            ("----\nBody.\n----", "Listing", SymbolKind::STRING),
            ("....\nBody.\n....", "Literal", SymbolKind::STRING),
            ("|===\n|Cell\n|===", "Table", SymbolKind::STRUCT),
            ("++++\nBody.\n++++", "Passthrough", SymbolKind::STRING),
            ("[verse]\n____\nBody.\n____", "Verse", SymbolKind::STRING),
            ("[stem]\n++++\nx\n++++", "Stem", SymbolKind::STRING),
            ("////\nComment.\n////", "Block", SymbolKind::STRING),
        ] {
            let source = format!("= Document\n\n{source}\n\n");
            let parsed = parse(&source, &Options::default())?;
            let symbols = document_symbols(parsed.document_ref(), &source);
            let symbol = symbols.get(1);
            assert_eq!(
                symbol.map(|symbol| (symbol.name.as_str(), symbol.kind)),
                Some((name, kind)),
                "{source}"
            );
        }
        Ok(())
    }

    #[test]
    fn test_document_symbols_extraction() -> Result<(), Error> {
        let content = r"= Document Title

== Section One

Some content.

== Section Two

=== Subsection

More content.
";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document_ref();
        let symbols = document_symbols(doc, content);

        // Header + 2 top-level sections
        assert_eq!(symbols.len(), 3);

        assert_eq!(
            symbols.first().map(|s| &s.name),
            Some(&"Document Title".to_string())
        );
        assert_eq!(
            symbols.get(1).map(|s| &s.name),
            Some(&"Section One".to_string())
        );
        assert_eq!(
            symbols.get(2).map(|s| &s.name),
            Some(&"Section Two".to_string())
        );

        // Section One has a paragraph child
        let section_one = symbols.get(1);
        let sec_one_children = section_one.and_then(|s| s.children.as_ref());
        assert!(
            sec_one_children.is_some(),
            "Section One should have children (paragraph)"
        );

        // Section Two has a subsection child (which itself has a paragraph)
        let section_two = symbols.get(2);
        let children = section_two.and_then(|s| s.children.as_ref());
        assert!(children.is_some(), "Section Two should have children");
        let children = children.map(Vec::as_slice);
        assert!(
            children.is_some_and(|c| c.iter().any(|s| s.name == "Subsection")),
            "expected Subsection child"
        );
        Ok(())
    }

    #[test]
    fn test_paragraph_symbols() -> Result<(), Error> {
        let content = "= Doc\n\n== Section\n\nA simple paragraph.\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document_ref();
        let symbols = document_symbols(doc, content);

        // Find the paragraph inside Section
        let section = symbols.get(1);
        let children = section.and_then(|s| s.children.as_ref());
        assert!(children.is_some());
        let para =
            children.and_then(|c| c.iter().find(|s| s.detail.as_deref() == Some("Paragraph")));
        assert!(para.is_some(), "expected paragraph symbol");
        assert_eq!(para.map(|p| p.kind), Some(SymbolKind::STRING));
        Ok(())
    }

    #[test]
    fn test_admonition_symbols() -> Result<(), Error> {
        let content = "= Doc\n\n== Section\n\nNOTE: This is a note.\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document_ref();
        let symbols = document_symbols(doc, content);

        let section = symbols.get(1);
        let children = section.and_then(|s| s.children.as_ref());
        assert!(children.is_some());
        let adm =
            children.and_then(|c| c.iter().find(|s| s.detail.as_deref() == Some("Admonition")));
        assert!(adm.is_some(), "expected admonition symbol");
        assert!(
            adm.is_some_and(|a| a.name.starts_with("NOTE")),
            "admonition name should start with NOTE"
        );
        Ok(())
    }

    #[test]
    fn test_delimited_block_symbols() -> Result<(), Error> {
        let content = "= Doc\n\n== Section\n\n.My sidebar\n****\nSidebar content.\n****\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document_ref();
        let symbols = document_symbols(doc, content);

        let section = symbols.get(1);
        let children = section.and_then(|s| s.children.as_ref());
        assert!(children.is_some());
        let sidebar =
            children.and_then(|c| c.iter().find(|s| s.detail.as_deref() == Some("Sidebar")));
        assert!(sidebar.is_some(), "expected sidebar symbol");
        assert_eq!(
            sidebar.map(|s| s.name.as_str()),
            Some("My sidebar"),
            "sidebar should use block title"
        );
        // Sidebar should have children (the paragraph inside)
        assert!(
            sidebar.is_some_and(|s| s.children.is_some()),
            "sidebar should have nested children"
        );
        Ok(())
    }

    #[test]
    fn test_list_symbols() -> Result<(), Error> {
        let content = "= Doc\n\n== Section\n\n* Item one\n* Item two\n* Item three\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document_ref();
        let symbols = document_symbols(doc, content);

        let section = symbols.get(1);
        let children = section.and_then(|s| s.children.as_ref());
        assert!(children.is_some());
        let list = children.and_then(|c| c.iter().find(|s| s.kind == SymbolKind::ARRAY));
        assert!(list.is_some(), "expected list symbol");
        assert!(
            list.is_some_and(|l| l.detail.as_deref().is_some_and(|d| d.contains("3 items"))),
            "list should show item count"
        );
        Ok(())
    }

    #[test]
    fn test_image_symbols() -> Result<(), Error> {
        let content = "= Doc\n\n== Section\n\nimage::photo.png[]\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document_ref();
        let symbols = document_symbols(doc, content);

        let section = symbols.get(1);
        let children = section.and_then(|s| s.children.as_ref());
        assert!(children.is_some());
        let img = children.and_then(|c| c.iter().find(|s| s.detail.as_deref() == Some("Image")));
        assert!(img.is_some(), "expected image symbol");
        assert!(
            img.is_some_and(|i| i.name.contains("photo.png")),
            "image name should contain source path"
        );
        Ok(())
    }

    #[test]
    fn test_comment_excluded() -> Result<(), Error> {
        let content = "= Doc\n\n// This is a comment\n\n== Section\n\nContent.\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document_ref();
        let symbols = document_symbols(doc, content);

        // Comments should not appear in symbols
        let has_comment = symbols.iter().any(|s| s.name.contains("comment"));
        assert!(
            !has_comment,
            "comments should not appear in document symbols"
        );
        Ok(())
    }

    #[test]
    fn test_paragraph_preview_truncation() {
        let short = "Hello world";
        assert_eq!(truncate(short, PARAGRAPH_PREVIEW_LEN), "Hello world");

        let long = "a".repeat(60);
        let result = truncate(&long, PARAGRAPH_PREVIEW_LEN);
        assert!(result.ends_with("..."));
        // 50 chars + "..." = 53
        assert_eq!(result.len(), 53);
    }
}
