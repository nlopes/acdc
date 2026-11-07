//! Command discovery reads authored identities and retains owned source data.

use acdc_execute::{CommandGraph, CommandId, DiscoveryError};
use acdc_parser::{CompatibilityMode, Options, SafeMode, parse, parse_file};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case::anchor("[[run]]\n[source,bash,role=command]")]
#[case::shorthand("[#run.command]\n[source,bash]")]
#[case::named("[source,bash,role=command,id=run]")]
#[case::primary_before_alias("[[alias]]\n[source,bash,role=command,id=run]")]
fn authored_ids_use_the_primary_target_and_keep_literal_source(
    #[case] attributes: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(
        "= Commands\n\n== *Build* λ 😀\n\n{attributes}\n----\nprintf '{{literal}}' <1>\n----\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let graph = CommandGraph::try_from(&parsed)?;
    drop(parsed);
    assert_eq!(
        graph.ids().map(CommandId::as_str).collect::<Vec<_>>(),
        ["run"]
    );
    let plan = graph.plan_all();
    let command = plan.commands().next().ok_or("command")?;
    assert_eq!(command.script, "printf '{literal}' <1>\n");
    assert_eq!(command.metadata.interpreter, "bash");
    assert_eq!(
        command.metadata.section_title.as_deref(),
        Some("Build λ 😀")
    );
    assert!(command.location.file.is_none());
    assert_eq!(command.location.location.start.line, 5);
    Ok(())
}

#[rstest::rstest]
fn generated_section_ids_do_not_supply_a_command_id(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Commands\n\n[.command]\n== Heading\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(matches!(
        CommandGraph::try_from(&parsed),
        Err(DiscoveryError::MissingId { .. })
    ));
    Ok(())
}

#[rstest::rstest]
fn selected_include_anchors_keep_the_original_source_location(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let included = directory.path().join("included.adoc");
    std::fs::write(
        &main,
        "= Commands\n\ninclude::included.adoc[lines=3..8,indent=0]\n",
    )?;
    std::fs::write(
        &included,
        "Ignored.\n\n    [[included]]\n    [source,bash,role=command]\n    ----\n    printf 'λ 😀'\n    ----\n\n",
    )?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_safe_mode(SafeMode::Safe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let graph = CommandGraph::try_from(&parsed)?;
    drop(parsed);
    let plan = graph.plan_all();
    let command = plan.commands().next().ok_or("included command")?;
    assert_eq!(command.metadata.id.as_str(), "included");
    assert_eq!(command.script, "printf 'λ 😀'\n");
    assert_eq!(command.location.file.as_deref(), Some(included.as_path()));
    assert_eq!(command.location.location.start.line, 3);
    assert_eq!(command.location.location.start.column, 5);
    Ok(())
}
