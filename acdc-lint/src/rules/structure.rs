use std::path::Path;

use acdc_parser::{
    Block, DelimitedBlock, DelimitedBlockType, Image, InlineMacro, InlineNode, ListItem, Source,
    Table, TableRow, UnorderedList,
};

use crate::LintId;

use super::{
    LintEmitter, SourceLine, is_list_continuation, is_skipped_line, root_list_family,
    split_first_char,
};

pub(crate) fn lint_document_extension(emitter: &mut LintEmitter<'_>, path: &Path) {
    let extension = path.extension().and_then(std::ffi::OsStr::to_str);
    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("adoc")) {
        return;
    }

    let message = if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("asc")) {
        "prefer the .adoc extension over .asc"
    } else {
        "AsciiDoc files should use the .adoc extension"
    };
    emitter.emit(
        LintId::DocumentExtension,
        message,
        Some("rename the file to use the .adoc extension".to_string()),
        None,
    );
}

pub(crate) fn lint_adjacent_list_separator(
    emitter: &mut LintEmitter<'_>,
    lines: &[SourceLine<'_>],
    skipped_lines: &[bool],
) {
    let mut last_list_family = None;
    let mut blank_since_last_list = false;
    let mut comment_separator_since_last_list = false;

    for line in lines {
        if is_skipped_line(line.number, skipped_lines) {
            last_list_family = None;
            blank_since_last_list = false;
            comment_separator_since_last_list = false;
            continue;
        }

        let trimmed = line.text.trim();
        if trimmed.is_empty() {
            blank_since_last_list = last_list_family.is_some();
            continue;
        }
        if trimmed.starts_with("//") {
            if blank_since_last_list {
                comment_separator_since_last_list = true;
            }
            continue;
        }

        if let Some(family) = root_list_family(line.text) {
            if last_list_family == Some(family)
                && blank_since_last_list
                && !comment_separator_since_last_list
            {
                emitter.emit(
                    LintId::AdjacentListSeparator,
                    "adjacent lists should be separated with an empty line comment",
                    Some("insert a line comment such as `//-` between the lists".to_string()),
                    Some(emitter.point_location(line.number, 1)),
                );
            }
            last_list_family = Some(family);
            blank_since_last_list = false;
            comment_separator_since_last_list = false;
        } else if !is_list_continuation(trimmed) {
            last_list_family = None;
            blank_since_last_list = false;
            comment_separator_since_last_list = false;
        }
    }
}

pub(crate) fn lint_blocks(emitter: &mut LintEmitter<'_>, blocks: &[Block<'_>], list_depth: usize) {
    for block in blocks {
        match block {
            Block::Admonition(block) => lint_blocks(emitter, &block.blocks, list_depth),
            Block::CalloutList(list) => {
                for item in &list.items {
                    lint_blocks(emitter, &item.blocks, list_depth.saturating_add(1));
                }
            }
            Block::DescriptionList(list) => {
                for item in &list.items {
                    lint_inlines(emitter, &item.term);
                    lint_inlines(emitter, &item.principal_text);
                    lint_blocks(emitter, &item.description, list_depth.saturating_add(1));
                }
            }
            Block::DelimitedBlock(block) => lint_delimited_block(emitter, block, list_depth),
            Block::DiscreteHeader(header) => lint_inlines(emitter, header.title.as_ref()),
            Block::Image(image) => lint_image(emitter, image),
            Block::OrderedList(list) => {
                lint_inlines(emitter, list.title.as_ref());
                for item in &list.items {
                    lint_list_item_inlines(emitter, item);
                    lint_blocks(emitter, &item.blocks, list_depth.saturating_add(1));
                }
            }
            Block::Paragraph(paragraph) => {
                lint_inlines(emitter, paragraph.title.as_ref());
                lint_inlines(emitter, &paragraph.content);
            }
            Block::Section(section) => {
                lint_inlines(emitter, section.title.as_ref());
                lint_blocks(emitter, &section.content, list_depth);
            }
            Block::UnorderedList(list) => lint_unordered_list(emitter, list, list_depth),
            Block::Audio(_)
            | Block::Comment(_)
            | Block::DocumentAttribute(_)
            | Block::PageBreak(_)
            | Block::TableOfContents(_)
            | Block::ThematicBreak(_)
            | Block::Video(_)
            | _ => {}
        }
    }
}

fn lint_delimited_block(
    emitter: &mut LintEmitter<'_>,
    block: &DelimitedBlock<'_>,
    list_depth: usize,
) {
    if let Some(minimum) = minimum_delimiter_len(block.delimiter) {
        let actual = block.delimiter.chars().count();
        if actual > minimum {
            let location = block
                .open_delimiter_location
                .as_ref()
                .unwrap_or(&block.location);
            emitter.emit(
                LintId::DelimitedBlockMinimalDelimiter,
                format!(
                    "delimited block uses `{}` but only {minimum} delimiter characters are needed",
                    block.delimiter
                ),
                Some("shorten the opening and closing block delimiters".to_string()),
                Some(emitter.source_location(location)),
            );
        }
    }

    match &block.inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks) => lint_blocks(emitter, blocks, list_depth),
        DelimitedBlockType::DelimitedTable(table) => lint_table(emitter, table, list_depth),
        DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedStem(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | _ => {}
    }
}

fn lint_table(emitter: &mut LintEmitter<'_>, table: &Table<'_>, list_depth: usize) {
    if let Some(header) = &table.header {
        lint_table_row(emitter, header, list_depth);
    }
    for row in &table.rows {
        lint_table_row(emitter, row, list_depth);
    }
    if let Some(footer) = &table.footer {
        lint_table_row(emitter, footer, list_depth);
    }
}

fn lint_table_row(emitter: &mut LintEmitter<'_>, row: &TableRow<'_>, list_depth: usize) {
    for column in &row.columns {
        lint_blocks(emitter, &column.content, list_depth);
    }
}

fn lint_unordered_list(emitter: &mut LintEmitter<'_>, list: &UnorderedList<'_>, list_depth: usize) {
    lint_inlines(emitter, list.title.as_ref());
    for item in &list.items {
        lint_list_item_inlines(emitter, item);
        if list_depth > 0 && item.marker.trim_start().starts_with('-') {
            emitter.emit(
                LintId::NestedUnorderedListMarker,
                "nested unordered list item uses a hyphen marker",
                Some("use asterisk markers for nested unordered lists".to_string()),
                Some(emitter.source_location(&item.location)),
            );
        }
        lint_blocks(emitter, &item.blocks, list_depth.saturating_add(1));
    }
}

fn lint_list_item_inlines(emitter: &mut LintEmitter<'_>, item: &ListItem<'_>) {
    lint_inlines(emitter, &item.principal);
}

fn lint_inlines(emitter: &mut LintEmitter<'_>, nodes: &[InlineNode<'_>]) {
    for node in nodes {
        match node {
            InlineNode::BoldText(text) => lint_inlines(emitter, &text.content),
            InlineNode::CurvedApostropheText(text) => lint_inlines(emitter, &text.content),
            InlineNode::CurvedQuotationText(text) => lint_inlines(emitter, &text.content),
            InlineNode::HighlightText(text) => lint_inlines(emitter, &text.content),
            InlineNode::ItalicText(text) => lint_inlines(emitter, &text.content),
            InlineNode::MonospaceText(text) => lint_inlines(emitter, &text.content),
            InlineNode::SubscriptText(text) => lint_inlines(emitter, &text.content),
            InlineNode::SuperscriptText(text) => lint_inlines(emitter, &text.content),
            InlineNode::Macro(macro_node) => lint_inline_macro(emitter, macro_node),
            InlineNode::CalloutRef(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::LineBreak(_)
            | InlineNode::PlainText(_)
            | InlineNode::RawText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::VerbatimText(_)
            | _ => {}
        }
    }
}

fn lint_inline_macro(emitter: &mut LintEmitter<'_>, macro_node: &InlineMacro<'_>) {
    match macro_node {
        InlineMacro::CrossReference(reference) => lint_inlines(emitter, &reference.text),
        InlineMacro::Footnote(footnote) => lint_inlines(emitter, &footnote.content),
        InlineMacro::Image(image) => lint_image(emitter, image),
        InlineMacro::Link(link) => lint_inlines(emitter, &link.text),
        InlineMacro::Mailto(mailto) => lint_inlines(emitter, &mailto.text),
        InlineMacro::Url(url) => lint_inlines(emitter, &url.text),
        InlineMacro::Autolink(_)
        | InlineMacro::Button(_)
        | InlineMacro::Icon(_)
        | InlineMacro::IndexTerm(_)
        | InlineMacro::Keyboard(_)
        | InlineMacro::Menu(_)
        | InlineMacro::Pass(_)
        | InlineMacro::Stem(_)
        | _ => {}
    }
}

fn lint_image(emitter: &mut LintEmitter<'_>, image: &Image<'_>) {
    let Some(target) = image_target_with_directory(&image.source) else {
        return;
    };
    let help = image_target_basename(&image.source).map_or_else(
        || "set :imagesdir: and use a filename-only image target".to_string(),
        |name| format!("set :imagesdir: and use `{name}` as the image target"),
    );
    emitter.emit(
        LintId::Imagesdir,
        format!("image target `{target}` repeats a directory path"),
        Some(help),
        Some(emitter.source_location(&image.location)),
    );
}

fn minimum_delimiter_len(delimiter: &str) -> Option<usize> {
    let (first, rest) = split_first_char(delimiter)?;
    if first == '`' {
        return None;
    }
    if matches!(first, '|' | '!' | ',' | ':') && rest.chars().all(|ch| ch == '=') {
        return Some(4);
    }
    if delimiter == "--" {
        return Some(2);
    }
    if matches!(first, '/' | '=' | '-' | '.' | '*' | '+' | '_' | '~')
        && delimiter.chars().all(|ch| ch == first)
    {
        return Some(4);
    }
    None
}

fn image_target_with_directory(source: &Source<'_>) -> Option<String> {
    match source {
        Source::Path(path) if path_has_directory(path) => Some(path.display().to_string()),
        Source::Name(name) if name.contains('/') || name.contains('\\') => {
            Some((*name).to_string())
        }
        Source::Name(_) | Source::Path(_) | Source::Url(_) => None,
    }
}

fn image_target_basename(source: &Source<'_>) -> Option<String> {
    match source {
        Source::Path(path) => path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .map(ToString::to_string),
        Source::Name(name) => name
            .rsplit(['/', '\\'])
            .next()
            .filter(|name| !name.is_empty())
            .map(ToString::to_string),
        Source::Url(_) => None,
    }
}

fn path_has_directory(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use crate::{Error, LintId, LintOptions, Lintable};

    use super::super::test_support::{has_lint, report_for};

    struct TempDoc {
        path: PathBuf,
    }

    impl TempDoc {
        fn new(name: &str, source: &str) -> Result<Self, Error> {
            let path =
                std::env::temp_dir().join(format!("acdc-lint-{}-{name}", std::process::id()));
            fs::write(&path, source)?;
            Ok(Self { path })
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDoc {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    #[test]
    fn document_extension_warns_for_non_adoc_names() -> Result<(), Error> {
        let doc = TempDoc::new("document-extension.asc", "= Title\n\nContent.\n")?;
        let report = doc.path().lint(&LintOptions::default())?;
        assert!(has_lint(&report, LintId::DocumentExtension));

        let report = "= Title\n\nContent.\n".lint(&LintOptions::default())?;
        assert!(!has_lint(&report, LintId::DocumentExtension));

        Ok(())
    }

    #[test]
    fn delimited_block_minimal_delimiter_flags_long_fences() -> Result<(), Error> {
        let report = report_for("= Title\n\n=====\nExample.\n=====\n")?;

        assert!(has_lint(&report, LintId::DelimitedBlockMinimalDelimiter));
        Ok(())
    }

    #[test]
    fn imagesdir_flags_directory_targets() -> Result<(), Error> {
        let report = report_for("= Title\n\nimage::images/photo.png[]\n")?;

        assert!(has_lint(&report, LintId::Imagesdir));
        Ok(())
    }

    #[test]
    fn nested_unordered_list_marker_flags_nested_hyphen() -> Result<(), Error> {
        let report = report_for("= Title\n\n* Parent\n+\n- Child\n")?;

        assert!(has_lint(&report, LintId::NestedUnorderedListMarker));
        Ok(())
    }

    #[test]
    fn adjacent_list_separator_flags_same_family_lists() -> Result<(), Error> {
        let report = report_for("= Title\n\n* First\n\n* Second\n")?;

        assert!(has_lint(&report, LintId::AdjacentListSeparator));
        Ok(())
    }
}
