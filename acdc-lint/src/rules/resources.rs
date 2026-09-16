use std::{
    convert::Infallible,
    path::{Path, PathBuf},
};

use acdc_converters_core::{TraversalContext, visitor::Visitor};
use acdc_parser::{Document, Image, InlineMacro, InlineNode, Source};

use crate::LintId;

use super::LintEmitter;

pub(crate) fn lint_resources(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    source_path: Option<&Path>,
) {
    let mut traversal = TraversalContext::new(&document.attributes);
    let mut visitor = ResourceVisitor {
        emitter,
        source_path,
    };
    let Ok(()) = traversal.visit_blocks(&mut visitor, &document.blocks);
}

struct ResourceVisitor<'emitter, 'source, 'path> {
    emitter: &'emitter mut LintEmitter<'source>,
    source_path: Option<&'path Path>,
}

impl<'doc> Visitor<'doc> for ResourceVisitor<'_, '_, '_> {
    type Error = Infallible;

    fn visit_image(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        image: &Image<'_>,
    ) -> Result<(), Self::Error> {
        lint_image(self.emitter, traversal, image, self.source_path);
        Ok(())
    }

    fn visit_inline_nodes(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        nodes: &[InlineNode<'_>],
    ) -> Result<(), Self::Error> {
        lint_resource_inlines(self.emitter, traversal, nodes, self.source_path);
        Ok(())
    }
}

fn lint_resource_inlines(
    emitter: &mut LintEmitter<'_>,
    attributes: &mut TraversalContext<'_>,
    nodes: &[InlineNode<'_>],
    source_path: Option<&Path>,
) {
    for node in nodes {
        match node {
            InlineNode::BoldText(text) => {
                lint_resource_inlines(emitter, attributes, &text.content, source_path);
            }
            InlineNode::CurvedApostropheText(text) => {
                lint_resource_inlines(emitter, attributes, &text.content, source_path);
            }
            InlineNode::CurvedQuotationText(text) => {
                lint_resource_inlines(emitter, attributes, &text.content, source_path);
            }
            InlineNode::HighlightText(text) => {
                lint_resource_inlines(emitter, attributes, &text.content, source_path);
            }
            InlineNode::ItalicText(text) => {
                lint_resource_inlines(emitter, attributes, &text.content, source_path);
            }
            InlineNode::MonospaceText(text) => {
                lint_resource_inlines(emitter, attributes, &text.content, source_path);
            }
            InlineNode::SubscriptText(text) => {
                lint_resource_inlines(emitter, attributes, &text.content, source_path);
            }
            InlineNode::SuperscriptText(text) => {
                lint_resource_inlines(emitter, attributes, &text.content, source_path);
            }
            InlineNode::Macro(macro_node) => match macro_node {
                InlineMacro::CrossReference(reference) => {
                    lint_resource_inlines(emitter, attributes, &reference.text, source_path);
                }
                InlineMacro::Footnote(footnote) => {
                    lint_resource_inlines(emitter, attributes, &footnote.content, source_path);
                }
                InlineMacro::Image(image) => {
                    lint_image(emitter, attributes, image, source_path);
                }
                InlineMacro::Link(link) => {
                    lint_resource_inlines(emitter, attributes, &link.text, source_path);
                }
                InlineMacro::Mailto(mailto) => {
                    lint_resource_inlines(emitter, attributes, &mailto.text, source_path);
                }
                InlineMacro::Url(url) => {
                    lint_resource_inlines(emitter, attributes, &url.text, source_path);
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
    attributes: &mut TraversalContext<'_>,
    image: &Image<'_>,
    source_path: Option<&Path>,
) {
    lint_imagesdir(emitter, image);
    lint_image_alt_text(emitter, image);
    lint_image_target_exists(emitter, attributes, image, source_path);
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
    attributes: &mut TraversalContext<'_>,
    image: &Image<'_>,
    source_path: Option<&Path>,
) {
    let Some(path) = image_target_path(attributes, image, source_path) else {
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
    attributes: &TraversalContext<'_>,
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
    let imagesdir = attributes
        .get("imagesdir")
        .and_then(|value| value.text())
        .map(acdc_parser::strip_quotes)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_default();
    if imagesdir.contains("://") || imagesdir.starts_with("//") {
        return None;
    }
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

    use super::{
        super::test_support::{has_lint, report_for},
        TraversalContext, image_target_path,
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
            .document()
            .blocks
            .iter()
            .find_map(|block| {
                if let acdc_parser::Block::Image(image) = block {
                    Some(image)
                } else {
                    None
                }
            })
            .ok_or("missing image")?;
        let source = Path::new("/tmp/attributes.adoc");
        let attributes = TraversalContext::new(&parsed.document().attributes);

        assert_eq!(
            image_target_path(&attributes, image, Some(source)),
            Some(PathBuf::from("/tmp/media files/photo.png"))
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
