//! Image-resource diagnostics keep source scopes and content-field ordering.

use acdc_lint::{
    LintDiagnostic, LintGroup, LintId, LintLevel, LintOptions, LintOverride, LintSelector, Lintable,
};
use acdc_parser::{CompatibilityMode, SafeMode};

type Error = Box<dyn std::error::Error>;

fn options(mode: CompatibilityMode) -> LintOptions {
    LintOptions::new(vec![
        LintOverride::new(LintLevel::Allow, LintSelector::Group(LintGroup::All)),
        LintOverride::new(
            LintLevel::Deny,
            LintSelector::Lint(LintId::ImageTargetExists),
        ),
    ])
    .with_compatibility_mode(mode)
    .with_safe_mode(SafeMode::Safe)
}

#[test]
fn resource_checks_keep_body_and_table_cell_attribute_scopes() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    for folder in ["outer", "inner"] {
        std::fs::create_dir(directory.path().join(folder))?;
        std::fs::write(directory.path().join(folder).join("image.svg"), "<svg/>")?;
    }
    std::fs::write(directory.path().join("root.svg"), "<svg/>")?;
    let path = directory.path().join("document.adoc");
    let source = "= Images\n:imagesdir: outer\n\nimage::image.svg[Outer]\n\n[cols=a]\n|===\n|\n:imagesdir: inner\n\nimage::image.svg[Inner]\n|===\n\nimage::image.svg[Outer again]\n\n:imagesdir!:\n\nimage::root.svg[Root]\n\nimage::missing.svg[Missing]\n";
    std::fs::write(&path, source)?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let report = path.lint(&options(mode))?;
        let [diagnostic] = report.diagnostics() else {
            return Err(format!("expected only the missing image: {report:?}").into());
        };
        assert_eq!(
            diagnostic.message(),
            "image target `missing.svg` does not exist"
        );
        let location = diagnostic.location().ok_or("missing image location")?;
        assert_eq!(location.file.as_deref(), Some(path.as_path()));
        assert_eq!(location.location.start.line, 20);
    }
    Ok(())
}

#[test]
fn resource_fields_keep_tooltip_before_link_label_order() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("document.adoc");
    let source = "link:https://example.test['image:label.svg[Label]',title='image:tip.svg[Tip]']";
    std::fs::write(&path, source)?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let report = path.lint(&options(mode))?;
        assert_eq!(
            report
                .diagnostics()
                .iter()
                .map(LintDiagnostic::message)
                .collect::<Vec<_>>(),
            [
                "image target `tip.svg` does not exist",
                "image target `label.svg` does not exist",
            ]
        );
    }
    Ok(())
}
