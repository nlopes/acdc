use super::{TestResult, paragraph_text, support};
use acdc_parser::{
    CompatibilityMode, Options,
    extensions::{BlockMacro, BlockMacroOutput, MacroExecutionContext, MacroInvocation, Registry},
};

#[test]
fn extensions_invocations_preserve_source_before_target_expansion() -> TestResult {
    const CALL: &str = r#"source::{which}[first,,'third,slot',custom="x,y",path='a\b']"#;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let mut builder = Registry::builder();
        builder.register(BlockMacro::new("source", || {
            |invocation: &MacroInvocation<'_>, _: &mut MacroExecutionContext| {
                assert_eq!(invocation.source, CALL);
                assert_eq!(
                    invocation.named_arguments_source(|name| matches!(name, "path" | "custom")),
                    r#"custom="x,y",path='a\b'"#
                );
                assert_eq!(
                    invocation.named_arguments_source(|name| name == "first"),
                    ""
                );
                assert_eq!(invocation.target, "expanded");
                assert_eq!(
                    invocation
                        .positional
                        .iter()
                        .map(|slot| slot.as_deref())
                        .collect::<Vec<_>>(),
                    vec![Some("first"), None, Some("third,slot")]
                );
                Ok(BlockMacroOutput::Paragraph(invocation.source.to_string()))
            }
        }))?;
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let result = builder
            .build()
            .parse(&format!("= Document\n:which: expanded\n\n{CALL}"), &options)?
            .finish()?;
        assert_eq!(result.warnings(), []);
        let block = result.document_storage().blocks.first().ok_or("output")?;
        assert_eq!(paragraph_text(block), CALL);
    }
    Ok(())
}

#[test]
fn extensions_deferred_source_keeps_quotes_slots_and_normal_cell_escapes() -> TestResult {
    let calls = [
        r#"defer::target[first,,'third,slot',custom="x,y"]"#,
        r#"defer::target[custom="a\|b"]"#,
    ];
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for call in calls {
            for input in [
                format!("= Document\n\n{call}"),
                format!("[cols=a]\n|===\n|{call}\n|==="),
                format!("[cols=a]\n|===\n|{call}\n\nifdef::never[]\nIgnored\nendif::[]\n|==="),
            ] {
                let result = support::registry().parse(&input, &options)?;
                let request = result.deferred_blocks().first().ok_or("deferred")?;
                let expected = if input.starts_with("[cols=a]") {
                    call.replace("\\|", "|")
                } else {
                    call.to_owned()
                };
                assert_eq!(request.invocation.source, expected, "{mode:?}: {input}");
            }
        }
    }
    Ok(())
}

#[test]
fn local_macro_target_uses_include_provider_origin_and_shared_budget() -> TestResult {
    use acdc_parser::{IncludeLoader, IncludeSource, IncludeSourceTarget, SafeMode};

    let root = tempfile::tempdir()?;
    let root_path = root.path().to_path_buf();
    let loader = IncludeLoader::custom(move |target: &IncludeSourceTarget| {
        let IncludeSourceTarget::File(path) = target else {
            panic!("expected file")
        };
        let relative = path.strip_prefix(&root_path).expect("configured base");
        Ok(IncludeSource::from_string(match relative.to_str() {
            Some("nested/part.adoc") => "read::data.txt[]",
            Some("nested/data.txt") => "data",
            Some("next.txt") => "again",
            _ => panic!("unexpected target: {relative:?}"),
        }))
    });
    let mut builder = Registry::builder();
    builder.register(BlockMacro::new("read", || {
        |call: &MacroInvocation<'_>, ctx: &mut MacroExecutionContext| match ctx.read_local_target()
        {
            Ok((path, text)) => {
                assert!(path.ends_with(call.target.as_ref()));
                Ok(BlockMacroOutput::Paragraph(text))
            }
            Err(err) => {
                ctx.warn(err.to_string());
                Ok(BlockMacroOutput::None)
            }
        }
    }))?;
    let registry = builder.build();
    let mut options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_include_loader(loader)
        .with_base_dir(root.path())
        .with_max_total_include_bytes("read::data.txt[]\n".len() + 4)
        .build()?;
    let result = registry
        .parse("include::nested/part.adoc[]\n\nread::next.txt[]", &options)?
        .finish()?;
    assert_eq!(paragraph_text(&result.document_storage().blocks[0]), "data");
    let [warning] = result.warnings() else {
        panic!("{:?}", result.warnings());
    };
    assert!(
        warning
            .to_string()
            .contains("remaining include byte budget")
    );
    assert_eq!(
        warning
            .location
            .as_ref()
            .ok_or("missing warning location")?
            .location
            .start
            .line,
        3
    );
    options.include_loader = IncludeLoader::Disabled;
    let result = registry.parse("read::data.txt[]", &options)?.finish()?;
    assert!(
        result.warnings()[0]
            .to_string()
            .contains("loading is disabled")
    );
    options.safe_mode = SafeMode::Secure;
    let result = registry.parse("read::data.txt[]", &options)?.finish()?;
    assert!(
        result.warnings()[0]
            .to_string()
            .contains("require unsafe mode")
    );
    Ok(())
}
