use acdc_parser::{InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[cfg(feature = "pre-spec-subs")]
#[test]
fn disabled_attribute_substitutions_do_not_warn_about_counters() -> Result<(), Error> {
    for subs in ["quotes", "none", "normal,-attributes"] {
        let source = format!(
            "[subs=\"{subs}\"]\nLiteral {{counter:seq}}, {{counter:seq:4}}, and {{counter2:seq}} with *bold*.\n"
        );
        let parsed = parse(&source, &Options::default())?;
        assert!(
            parsed
                .warnings()
                .iter()
                .all(|warning| !warning.kind.to_string().contains("Counters")),
            "{:?}",
            parsed.warnings()
        );
    }
    Ok(())
}

#[test]
fn bibliography_footnotes_keep_their_source_and_catalog_entry() -> Result<(), Error> {
    let source = "[bibliography]\n* [[[note,footnote:[A note.] +]]] Footnote label.\n";
    let parsed = parse(source, &Options::default())?;
    let [footnote] = parsed.document().footnotes.as_slice() else {
        return Err("expected one bibliography footnote".into());
    };
    assert_eq!(footnote.number, 1);
    assert!(
        matches!(footnote.content.as_slice(), [InlineNode::PlainText(text)] if text.content == "A note.")
    );
    assert_eq!(
        source.get(footnote.location.absolute_start..=footnote.location.absolute_end),
        Some("footnote:[A note.]")
    );
    Ok(())
}

#[test]
fn bibliography_counter_citations_remain_literal() -> Result<(), Error> {
    let parsed = parse(
        "Cite <<literal-counter>>.\n\n[bibliography]\n* [[[literal-counter,{counter:seq} +]]] Entry.\n",
        &Options::default(),
    )?;
    let label = parsed
        .document()
        .references
        .get("literal-counter")
        .and_then(|reference| reference.xreflabel.as_ref())
        .ok_or("missing citation")?;
    let text = label
        .iter()
        .map(|node| {
            if let InlineNode::PlainText(text) = node {
                Ok(text.content)
            } else if let InlineNode::RawText(raw) = node {
                Ok(raw.content)
            } else {
                Err(Error::from("expected literal citation text"))
            }
        })
        .collect::<Result<String, Error>>()?;
    assert_eq!(text, "[{counter:seq} +]");
    Ok(())
}
