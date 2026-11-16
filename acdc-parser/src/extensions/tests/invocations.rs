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
