use std::path::{Path, PathBuf};

use acdc_parser::{
    Block, CalloutList, DelimitedBlock, DelimitedBlockType, DescriptionList, Document, Image,
    InlineMacro, InlineNode, OrderedList, Paragraph, Section, Source, UnorderedList,
};

use crate::LintId;

use super::LintEmitter;

pub(crate) fn lint_resources(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    source_path: Option<&Path>,
) {
    lint_resource_blocks(emitter, document, &document.blocks, source_path);
}

fn lint_resource_blocks(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    blocks: &[Block<'_>],
    source_path: Option<&Path>,
) {
    for block in blocks {
        match block {
            Block::Admonition(block) => {
                lint_resource_blocks(emitter, document, &block.blocks, source_path);
            }
            Block::CalloutList(list) => {
                lint_resource_callout_list(emitter, document, list, source_path);
            }
            Block::DescriptionList(list) => {
                lint_resource_description_list(emitter, document, list, source_path);
            }
            Block::DelimitedBlock(block) => {
                lint_resource_delimited_block(emitter, document, block, source_path);
            }
            Block::DiscreteHeader(header) => {
                lint_resource_inlines(emitter, document, header.title.as_ref(), source_path);
            }
            Block::Image(image) => lint_image(emitter, document, image, source_path),
            Block::OrderedList(list) => {
                lint_resource_ordered_list(emitter, document, list, source_path);
            }
            Block::Paragraph(paragraph) => {
                lint_resource_paragraph(emitter, document, paragraph, source_path);
            }
            Block::Section(section) => {
                lint_resource_section(emitter, document, section, source_path);
            }
            Block::UnorderedList(list) => {
                lint_resource_unordered_list(emitter, document, list, source_path);
            }
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

fn lint_resource_callout_list(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    list: &CalloutList<'_>,
    source_path: Option<&Path>,
) {
    for item in &list.items {
        lint_resource_blocks(emitter, document, &item.blocks, source_path);
    }
}

fn lint_resource_description_list(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    list: &DescriptionList<'_>,
    source_path: Option<&Path>,
) {
    for item in &list.items {
        lint_resource_inlines(emitter, document, &item.term, source_path);
        lint_resource_inlines(emitter, document, &item.principal_text, source_path);
        lint_resource_blocks(emitter, document, &item.description, source_path);
    }
}

fn lint_resource_ordered_list(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    list: &OrderedList<'_>,
    source_path: Option<&Path>,
) {
    lint_resource_inlines(emitter, document, list.title.as_ref(), source_path);
    for item in &list.items {
        lint_resource_inlines(emitter, document, &item.principal, source_path);
        lint_resource_blocks(emitter, document, &item.blocks, source_path);
    }
}

fn lint_resource_unordered_list(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    list: &UnorderedList<'_>,
    source_path: Option<&Path>,
) {
    lint_resource_inlines(emitter, document, list.title.as_ref(), source_path);
    for item in &list.items {
        lint_resource_inlines(emitter, document, &item.principal, source_path);
        lint_resource_blocks(emitter, document, &item.blocks, source_path);
    }
}

fn lint_resource_paragraph(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    paragraph: &Paragraph<'_>,
    source_path: Option<&Path>,
) {
    lint_resource_inlines(emitter, document, paragraph.title.as_ref(), source_path);
    lint_resource_inlines(emitter, document, &paragraph.content, source_path);
}

fn lint_resource_section(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    section: &Section<'_>,
    source_path: Option<&Path>,
) {
    lint_resource_inlines(emitter, document, section.title.as_ref(), source_path);
    lint_resource_blocks(emitter, document, &section.content, source_path);
}

fn lint_resource_delimited_block(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    block: &DelimitedBlock<'_>,
    source_path: Option<&Path>,
) {
    match &block.inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks) => {
            lint_resource_blocks(emitter, document, blocks, source_path);
        }
        DelimitedBlockType::DelimitedTable(table) => {
            for row in table
                .header
                .iter()
                .chain(table.rows.iter())
                .chain(table.footer.iter())
            {
                for column in &row.columns {
                    lint_resource_blocks(emitter, document, &column.content, source_path);
                }
            }
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

fn lint_resource_inlines(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    nodes: &[InlineNode<'_>],
    source_path: Option<&Path>,
) {
    for node in nodes {
        match node {
            InlineNode::BoldText(text) => {
                lint_resource_inlines(emitter, document, &text.content, source_path);
            }
            InlineNode::CurvedApostropheText(text) => {
                lint_resource_inlines(emitter, document, &text.content, source_path);
            }
            InlineNode::CurvedQuotationText(text) => {
                lint_resource_inlines(emitter, document, &text.content, source_path);
            }
            InlineNode::HighlightText(text) => {
                lint_resource_inlines(emitter, document, &text.content, source_path);
            }
            InlineNode::ItalicText(text) => {
                lint_resource_inlines(emitter, document, &text.content, source_path);
            }
            InlineNode::MonospaceText(text) => {
                lint_resource_inlines(emitter, document, &text.content, source_path);
            }
            InlineNode::SubscriptText(text) => {
                lint_resource_inlines(emitter, document, &text.content, source_path);
            }
            InlineNode::SuperscriptText(text) => {
                lint_resource_inlines(emitter, document, &text.content, source_path);
            }
            InlineNode::Macro(macro_node) => match macro_node {
                InlineMacro::CrossReference(reference) => {
                    lint_resource_inlines(emitter, document, &reference.text, source_path);
                }
                InlineMacro::Footnote(footnote) => {
                    lint_resource_inlines(emitter, document, &footnote.content, source_path);
                }
                InlineMacro::Image(image) => {
                    lint_image(emitter, document, image, source_path);
                }
                InlineMacro::Link(link) => {
                    lint_resource_inlines(emitter, document, &link.text, source_path);
                }
                InlineMacro::Mailto(mailto) => {
                    lint_resource_inlines(emitter, document, &mailto.text, source_path);
                }
                InlineMacro::Url(url) => {
                    lint_resource_inlines(emitter, document, &url.text, source_path);
                }
                InlineMacro::Autolink(_)
                | InlineMacro::Button(_)
                | InlineMacro::Icon(_)
                | InlineMacro::IndexTerm(_)
                | InlineMacro::Keyboard(_)
                | InlineMacro::Menu(_)
                | InlineMacro::Pass(_)
                | InlineMacro::Stem(_)
                | _ => {}
            },
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

fn lint_image(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    image: &Image<'_>,
    source_path: Option<&Path>,
) {
    lint_imagesdir(emitter, image);
    lint_image_alt_text(emitter, image);
    lint_image_target_exists(emitter, document, image, source_path);
}

fn lint_imagesdir(emitter: &mut LintEmitter<'_>, image: &Image<'_>) {
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

fn lint_image_alt_text(emitter: &mut LintEmitter<'_>, image: &Image<'_>) {
    let has_alt = image
        .metadata
        .attributes
        .get_string("alt")
        .is_some_and(|alt| !alt.trim().is_empty());
    if has_alt {
        return;
    }

    emitter.emit(
        LintId::ImageAltText,
        "image is missing alt text",
        None,
        Some(emitter.source_location(&image.location)),
    );
}

fn lint_image_target_exists(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    image: &Image<'_>,
    source_path: Option<&Path>,
) {
    let Some(path) = image_target_path(document, image, source_path) else {
        return;
    };
    if path.exists() {
        return;
    }

    emitter.emit(
        LintId::ImageTargetExists,
        format!("image target `{}` does not exist", image.source),
        None,
        Some(emitter.source_location(&image.location)),
    );
}

fn image_target_path(
    document: &Document<'_>,
    image: &Image<'_>,
    source_path: Option<&Path>,
) -> Option<PathBuf> {
    let source_path = source_path?;
    let target = match &image.source {
        Source::Url(_) => return None,
        Source::Path(path) => path.clone(),
        Source::Name(name) => PathBuf::from(name),
    };
    if target.is_absolute() {
        return Some(target);
    }

    let base = source_path.parent().unwrap_or_else(|| Path::new("."));
    let has_dir = target
        .parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty());
    if has_dir {
        return Some(base.join(target));
    }

    let imagesdir = document
        .attributes
        .get_string("imagesdir")
        .filter(|value| !value.trim().is_empty())
        .map_or_else(PathBuf::new, |value| PathBuf::from(value.as_ref()));
    Some(base.join(imagesdir).join(target))
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
    fn imagesdir_flags_directory_targets() -> Result<(), Error> {
        let report = report_for("= Title\n\nimage::images/photo.png[Photo]\n")?;

        assert!(has_lint(&report, LintId::Imagesdir));
        Ok(())
    }

    #[test]
    fn image_alt_text_flags_empty_alt() -> Result<(), Error> {
        let report = report_for("= Title\n\nimage::photo.png[]\n")?;

        assert!(has_lint(&report, LintId::ImageAltText));
        Ok(())
    }

    #[test]
    fn image_target_exists_flags_missing_file() -> Result<(), Error> {
        let doc = TempDoc::new("missing-image.adoc", "= Title\n\nimage::photo.png[Photo]\n")?;
        let report = doc.path().lint(&LintOptions::default())?;

        assert!(has_lint(&report, LintId::ImageTargetExists));
        Ok(())
    }
}
