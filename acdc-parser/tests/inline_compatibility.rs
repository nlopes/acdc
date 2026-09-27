use acdc_parser::{InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[cfg(feature = "pre-spec-subs")]
#[test]
fn index_registration_boundaries_survive_later_substitutions() -> Result<(), Error> {
    for subs in [
        "macros,attributes",
        "macros,quotes",
        "macros,attributes,quotes",
    ] {
        for label in [
            "((Start {close} End))",
            "indexterm2:[Start {bracket} End]",
            "({empty}(Joined))",
            "(({empty})) indexterm2:[{empty}]",
            "((Outer ((Inner))))",
        ] {
            for code in [false, true] {
                for index in ["", "\n[index]\n== Index\n"] {
                    let block = if code {
                        format!("[source,text,subs=\"{subs}\"]\n----\n{label}\n----")
                    } else {
                        format!("[subs=\"{subs}\"]\n{label}")
                    };
                    let source = format!(
                        "= Boundaries\n:close: ))\n:bracket: ]\n:empty:\n\n{block}\n{index}"
                    );
                    let parsed = parse(&source, &Options::default())?;
                    assert!(!parsed.document().blocks.is_empty());
                }
            }
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn index_catalog_snapshots_do_not_register_footnotes_twice() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/subs_index_registration_order.adoc");
    let parsed = parse(source, &Options::default())?;
    let notes = &parsed.document().footnotes;
    assert_eq!(notes.len(), 2);
    for (note, expected) in notes
        .iter()
        .zip(["footnote:[Only once]", "footnote:[Second note]"])
    {
        assert_eq!(
            source.get(note.location.absolute_start..=note.location.absolute_end),
            Some(expected)
        );
    }
    assert_eq!(
        notes.iter().map(|note| note.number).collect::<Vec<_>>(),
        [1, 2]
    );
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn ordered_index_footnotes_keep_one_definition_and_original_locations() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/subs_index_stage_labels.adoc");
    let parsed = parse(source, &Options::default())?;
    let notes = &parsed.document().footnotes;
    assert_eq!(notes.len(), 4);
    for (note, expected) in notes.iter().zip([
        "footnote:[One]",
        "footnote:[Two]",
        "footnote:[((Note {name})) and *literal* (C)]",
        "footnote:[*bold* ((Early {name}))]",
    ]) {
        assert_eq!(
            source.get(note.location.absolute_start..=note.location.absolute_end),
            Some(expected)
        );
    }
    Ok(())
}

#[test]
fn passthrough_index_attribute_values_point_to_their_reference() -> Result<(), Error> {
    use acdc_parser::{Block, InlineMacro};

    for value in ["Expanded", "é", "🦀", "foobar", "a much longer replacement"] {
        for stages in ["attributes,macros", "macros,attributes"] {
            let source = format!(":name: {value}\n\npass:{stages}[((Term {{name}}))]\n");
            let parsed = parse(&source, &Options::default())?;
            let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
                return Err("missing paragraph".into());
            };
            let Some(InlineNode::Macro(InlineMacro::IndexTerm(term))) = paragraph.content.first()
            else {
                return Err("missing index term".into());
            };
            let raw = term
                .term()
                .iter()
                .find_map(|node| {
                    if let InlineNode::RawText(raw) = node {
                        (raw.content == value).then_some(raw)
                    } else {
                        None
                    }
                })
                .ok_or("missing expanded label")?;
            assert_eq!(
                source.get(raw.location.absolute_start..=raw.location.absolute_end),
                Some("{name}"),
                "{stages}: {value} at {:?}",
                raw.location,
            );
        }
    }
    Ok(())
}

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
