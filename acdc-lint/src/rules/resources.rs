use std::{
    convert::Infallible,
    path::{Path, PathBuf},
};

use acdc_converters_core::{InlineTextTransform, TraversalContext, visitor::BlockVisitor};
use acdc_parser::{
    ColumnStyle,
    ast::{
        AttributeValueRef, BlockKind, BlockRef, ColumnFormatRef, ContentRef, ContentRole,
        DocumentRef, InlineKind, LocationRef, SourceKind, SourceRef, StructuralRef, TableRef,
    },
};

use crate::LintId;

use super::LintEmitter;

pub(crate) fn lint_resources(
    emitter: &mut LintEmitter<'_>,
    document: DocumentRef<'_>,
    source_path: Option<&Path>,
) {
    let mut traversal = TraversalContext::new(document.attributes());
    let mut visitor = ResourceVisitor {
        emitter,
        source_path,
    };
    let Ok(()) = traversal.visit_blocks(&mut visitor, document.blocks());
}

struct ResourceVisitor<'emitter, 'source, 'path> {
    emitter: &'emitter mut LintEmitter<'source>,
    source_path: Option<&'path Path>,
}

impl<'doc> BlockVisitor<'doc> for ResourceVisitor<'_, '_, '_> {
    type Error = Infallible;

    fn before_block(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        block: BlockRef<'doc>,
    ) -> Result<(), Self::Error> {
        if let Some(AttributeValueRef::Content(content)) = block
            .attributes()
            .and_then(|attributes| attributes.get("caption"))
        {
            lint_resource_inlines(self.emitter, traversal, content, self.source_path);
        }
        Ok(())
    }

    fn visit_block(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        block: BlockRef<'doc>,
    ) -> Result<(), Self::Error> {
        if let BlockKind::Image(image) = block.kind() {
            if let Some(alt) = image.alt() {
                lint_resource_inlines(self.emitter, traversal, alt, self.source_path);
            }
            lint_image(
                self.emitter,
                traversal,
                image.source(),
                image.location(),
                image.alt(),
                self.source_path,
            );
            return Ok(());
        }
        let visit_title = matches!(
            block.kind(),
            BlockKind::Paragraph(_)
                | BlockKind::Section(_)
                | BlockKind::DiscreteHeader(_)
                | BlockKind::OrderedList(_)
                | BlockKind::UnorderedList(_)
        );
        for field in StructuralRef::Block(block).content_fields() {
            if (visit_title && field.role() == ContentRole::Title)
                || (matches!(block.kind(), BlockKind::Paragraph(_))
                    && field.role() == ContentRole::Body)
            {
                lint_resource_inlines(self.emitter, traversal, field.content(), self.source_path);
            }
        }
        if let BlockKind::Table(table) = block.kind() {
            self.visit_table(traversal, table)
        } else {
            for child in block.children() {
                self.visit_structure(traversal, child)?;
            }
            Ok(())
        }
    }
}

impl ResourceVisitor<'_, '_, '_> {
    fn visit_structure<'doc>(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        node: StructuralRef<'doc>,
    ) -> Result<(), Infallible> {
        if let StructuralRef::Block(block) = node {
            return traversal.visit_block(self, block);
        }
        for field in node.content_fields() {
            if matches!(
                field.role(),
                ContentRole::ListPrincipal | ContentRole::DescriptionTerm
            ) {
                lint_resource_inlines(self.emitter, traversal, field.content(), self.source_path);
            }
        }
        for child in node.children() {
            self.visit_structure(traversal, child)?;
        }
        Ok(())
    }

    fn visit_table<'doc>(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        table: TableRef<'doc>,
    ) -> Result<(), Infallible> {
        for row in table
            .header()
            .into_iter()
            .chain(table.rows())
            .chain(table.footer())
        {
            let mut formats = table.columns();
            for cell in row.cells() {
                let default_style = formats
                    .next()
                    .map_or(ColumnStyle::Default, ColumnFormatRef::style);
                if cell.style().unwrap_or(default_style) == ColumnStyle::AsciiDoc {
                    traversal.with_table_cell(cell, |context| {
                        context.visit_blocks(self, cell.blocks())
                    })?;
                } else {
                    traversal.visit_blocks(self, cell.blocks())?;
                }
            }
        }
        Ok(())
    }
}

fn lint_resource_inlines(
    emitter: &mut LintEmitter<'_>,
    attributes: &mut TraversalContext<'_>,
    nodes: ContentRef<'_>,
    source_path: Option<&Path>,
) {
    for node in nodes.inlines() {
        let mut link_text = None;
        for field in node.content_fields() {
            match field.role() {
                // Tooltips are checked before link labels to retain diagnostic order.
                ContentRole::LinkText => link_text = Some(field.content()),
                ContentRole::Alt
                | ContentRole::Tooltip
                | ContentRole::ButtonLabel
                | ContentRole::KeyboardKey
                | ContentRole::MenuTarget
                | ContentRole::MenuItem
                | ContentRole::Span
                | ContentRole::Footnote => {
                    lint_resource_inlines(emitter, attributes, field.content(), source_path);
                }
                ContentRole::Title
                | ContentRole::Subtitle
                | ContentRole::Body
                | ContentRole::Attribution
                | ContentRole::CiteTitle
                | ContentRole::Caption
                | ContentRole::ReferenceText
                | ContentRole::BibliographyLabel
                | ContentRole::ListPrincipal
                | ContentRole::DescriptionTerm
                | ContentRole::IndexPrimary
                | ContentRole::IndexSecondary
                | ContentRole::IndexTertiary
                | ContentRole::IndexSee
                | ContentRole::IndexSeeAlso
                | _ => {}
            }
        }
        if let Some(content) = link_text {
            lint_resource_inlines(emitter, attributes, content, source_path);
        }
        if let InlineKind::Image(image) = node.kind() {
            lint_image(
                emitter,
                attributes,
                image.source(),
                image.location(),
                image.alt(),
                source_path,
            );
        }
    }
}

fn lint_image(
    emitter: &mut LintEmitter<'_>,
    attributes: &mut TraversalContext<'_>,
    source: SourceRef<'_>,
    location: LocationRef<'_>,
    alt: Option<ContentRef<'_>>,
    source_path: Option<&Path>,
) {
    lint_imagesdir(emitter, source, location);
    lint_image_alt_text(emitter, alt, location);
    lint_image_target_exists(emitter, attributes, source, location, source_path);
}

fn lint_imagesdir(emitter: &mut LintEmitter<'_>, source: SourceRef<'_>, location: LocationRef<'_>) {
    let Some(target) = image_target_with_directory(source) else {
        return;
    };
    let help = image_target_basename(source).map_or_else(
        || "set :imagesdir: and use a filename-only image target".to_string(),
        |name| format!("set :imagesdir: and use `{name}` as the image target"),
    );
    emitter.emit(
        LintId::Imagesdir,
        format!("image target `{target}` repeats a directory path"),
        Some(help),
        Some(emitter.source_location(location)),
    );
}

fn lint_image_alt_text(
    emitter: &mut LintEmitter<'_>,
    alt: Option<ContentRef<'_>>,
    location: LocationRef<'_>,
) {
    let has_alt = alt.is_some_and(|alt| {
        !InlineTextTransform::default()
            .content_to_string(alt)
            .trim()
            .is_empty()
    });
    if has_alt {
        return;
    }

    emitter.emit(
        LintId::ImageAltText,
        "image is missing alt text",
        None,
        Some(emitter.source_location(location)),
    );
}

fn lint_image_target_exists(
    emitter: &mut LintEmitter<'_>,
    attributes: &mut TraversalContext<'_>,
    source: SourceRef<'_>,
    location: LocationRef<'_>,
    source_path: Option<&Path>,
) {
    let Some(path) = image_target_path(attributes, source, source_path) else {
        return;
    };
    if path.exists() {
        return;
    }

    emitter.emit(
        LintId::ImageTargetExists,
        format!("image target `{source}` does not exist"),
        None,
        Some(emitter.source_location(location)),
    );
}

fn image_target_path(
    attributes: &TraversalContext<'_>,
    source: SourceRef<'_>,
    source_path: Option<&Path>,
) -> Option<PathBuf> {
    let source_path = source_path?;
    let target = match source.kind() {
        SourceKind::Path(path) => path.to_path_buf(),
        SourceKind::Name(name) => PathBuf::from(name),
        SourceKind::Url(_) | _ => return None,
    };
    if target.is_absolute() {
        return Some(target);
    }

    let base = source_path.parent().unwrap_or_else(|| Path::new("."));
    let imagesdir = attributes
        .get("imagesdir")
        .and_then(|value| value.text())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_default();
    if imagesdir.contains("://") || imagesdir.starts_with("//") {
        return None;
    }
    Some(base.join(imagesdir).join(target))
}

fn image_target_with_directory(source: SourceRef<'_>) -> Option<String> {
    match source.kind() {
        SourceKind::Path(path) if path_has_directory(path) => Some(path.display().to_string()),
        SourceKind::Name(name) if name.contains('/') || name.contains('\\') => {
            Some(name.to_string())
        }
        SourceKind::Name(_) | SourceKind::Path(_) | SourceKind::Url(_) | _ => None,
    }
}

fn image_target_basename(source: SourceRef<'_>) -> Option<String> {
    match source.kind() {
        SourceKind::Path(path) => path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .map(ToString::to_string),
        SourceKind::Name(name) => name
            .rsplit(['/', '\\'])
            .next()
            .filter(|name| !name.is_empty())
            .map(ToString::to_string),
        SourceKind::Url(_) | _ => None,
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

    use super::{
        super::test_support::{has_lint, report_for},
        BlockKind, TraversalContext, image_target_path,
    };

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
    fn inline_image_alt_text_uses_the_explicit_alt_field() -> Result<(), Error> {
        for attributes in ["Photo", "alt=Photo", "Photo,title=Tooltip"] {
            let source = format!("= Title\n\nInline image:photo.png[{attributes}].\n");
            let report = report_for(&source)?;
            assert!(!has_lint(&report, LintId::ImageAltText));
        }
        Ok(())
    }

    #[test]
    fn image_target_exists_flags_missing_file() -> Result<(), Error> {
        let doc = TempDoc::new("missing-image.adoc", "= Title\n\nimage::photo.png[Photo]\n")?;
        let report = doc.path().lint(&LintOptions::default())?;

        assert!(has_lint(&report, LintId::ImageTargetExists));
        Ok(())
    }

    #[test]
    fn image_target_uses_document_text_presentation() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            ":imagesdir: 'media files'\n\nimage::photo.png[Photo]\n",
            &acdc_parser::Options::default(),
        )?;
        let image = parsed
            .document_ref()
            .blocks()
            .find_map(|block| {
                if let BlockKind::Image(image) = block.kind() {
                    Some(image)
                } else {
                    None
                }
            })
            .ok_or("missing image")?;
        let source = Path::new("/tmp/attributes.adoc");
        let attributes = TraversalContext::new(parsed.document_ref().attributes());

        assert_eq!(
            image_target_path(&attributes, image.source(), Some(source)),
            Some(PathBuf::from("/tmp/'media files'/photo.png"))
        );
        Ok(())
    }

    #[test]
    fn image_targets_follow_body_events_and_nested_cell_scope()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        fs::create_dir(directory.path().join("assets"))?;
        fs::write(directory.path().join("assets/inside.svg"), "<svg/>")?;
        fs::write(directory.path().join("after.svg"), "<svg/>")?;
        let path = directory.path().join("document.adoc");
        for source in [
            "= T\n\n[NOTE]\n====\n:imagesdir: assets\n\nimage::inside.svg[Inside]\n====\n\nimage::inside.svg[Inside]\n\n:imagesdir!:\n\nimage::after.svg[After]\n",
            "= T\n\n[cols=a]\n|===\n|\n:imagesdir: assets\n\nimage::inside.svg[Inside]\n|===\n\nimage::after.svg[After]\n",
        ] {
            fs::write(&path, source)?;
            let report = path.lint(&LintOptions::default())?;
            assert!(!has_lint(&report, LintId::ImageTargetExists));
        }
        Ok(())
    }
}
