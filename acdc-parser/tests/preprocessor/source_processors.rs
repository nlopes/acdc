use acdc_parser::{
    Error, Options,
    extensions::{Registry, SourceEdit, SourceProcessorContext},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn preprocessors_compose_and_retain_authored_positions() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_preprocessor("first", || {
        |_: &str, _: &SourceProcessorContext<'_>| {
            Ok(vec![
                SourceEdit::Replace {
                    line: 1,
                    text: "Generated one.\n\nGenerated two.".into(),
                },
                SourceEdit::Remove(3),
            ])
        }
    })?;
    builder.register_preprocessor("second", || {
        |source: &str, _: &SourceProcessorContext<'_>| {
            assert!(source.starts_with("Generated one.\n\nGenerated two."));
            Ok(vec![SourceEdit::Remove(1)])
        }
    })?;
    let result = builder
        .build()
        .parse("x\n\nomit\nTail.", &Options::default())?
        .finish()?;
    let positions: Vec<_> = result
        .document()
        .blocks()
        .map(acdc_parser::ast::BlockRef::location)
        .collect();
    let [generated, tail] = positions.as_slice() else {
        return Err("expected two paragraphs".into());
    };
    assert_eq!(generated.start().line(), 1);
    assert_eq!(generated.end().column(), 1);
    assert_eq!(generated.absolute_end(), 0);
    assert_eq!(tail.start().line(), 4);
    assert_eq!(tail.absolute_start(), 8);
    Ok(())
}

#[test]
fn invalid_source_edits_are_located() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_preprocessor("bad", || {
        |_: &str, _: &SourceProcessorContext<'_>| Ok(vec![SourceEdit::Remove(4)])
    })?;
    let Err(Error::Extension(location, message)) =
        builder.build().parse("One.", &Options::default())
    else {
        return Err("expected source edit error".into());
    };
    assert_eq!(location.location.start.line, 1);
    assert!(message.contains("line 4"));
    Ok(())
}
