use acdc_parser::{Options, parse};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error>;

fn fixture(source: &str) -> Result<Value, Error> {
    Ok(serde_json::to_value(
        parse(source, &Options::default())?.document(),
    )?)
}

#[test]
fn media_content_has_one_semantic_owner() -> Result<(), Error> {
    let doc = fixture(include_str!(
        "../fixtures/tests/media_content_attributes.adoc"
    ))?;
    assert_eq!(
        doc.pointer("/blocks/0/alt/0/variant"),
        Some(&json!("strong"))
    );
    assert!(doc.pointer("/blocks/0/metadata/attributes/alt").is_none());
    assert_eq!(
        doc.pointer("/blocks/1/inlines/0/alt/0/variant"),
        Some(&json!("strong"))
    );
    assert_eq!(
        doc.pointer("/blocks/1/inlines/0/tooltip/0/variant"),
        Some(&json!("emphasis"))
    );
    assert_eq!(doc.pointer("/blocks/2/inlines/0/alt"), Some(&json!([])));
    assert_eq!(doc.pointer("/blocks/2/inlines/0/tooltip"), Some(&json!([])));
    assert_eq!(
        doc.pointer("/blocks/3/inlines/0/alt/0/value"),
        Some(&json!("*Inserted*"))
    );
    assert_eq!(
        doc.pointer("/blocks/4/inlines/0/alt/0/variant"),
        Some(&json!("strong"))
    );
    assert_eq!(
        doc.pointer("/blocks/4/inlines/0/attributes/size"),
        Some(&json!("Other"))
    );
    for index in 5..=7 {
        assert!(
            doc.pointer(&format!("/blocks/{index}/inlines/0/tooltip/0/variant"))
                .is_some()
        );
        assert!(
            doc.pointer(&format!("/blocks/{index}/inlines/0/attributes/title"))
                .is_none()
        );
    }
    Ok(())
}

#[test]
fn video_targets_are_preserved_before_provider_interpretation() -> Result<(), Error> {
    let doc = fixture(include_str!(
        "../fixtures/tests/video_target_preservation.adoc"
    ))?;
    assert_eq!(
        doc.pointer("/blocks/0/source/value"),
        Some(&json!("one.mp4,two.mp4"))
    );
    assert_eq!(doc.pointer("/blocks/1/name"), Some(&json!("video")));
    assert_eq!(
        doc.pointer("/blocks/1/source/value"),
        Some(&json!("one,,three"))
    );
    assert_eq!(
        doc.pointer("/blocks/1/metadata/attributes/poster"),
        Some(&json!("youtube"))
    );
    assert!(
        doc.pointer("/blocks/1/metadata/attributes/youtube")
            .is_none()
    );
    assert_eq!(
        doc.pointer("/blocks/4/source/value"),
        Some(&json!("local clip.mp4"))
    );
    Ok(())
}

#[test]
fn ui_content_retains_inline_formatting() -> Result<(), Error> {
    let doc = fixture(include_str!("../fixtures/tests/ui_content_attributes.adoc"))?;
    assert_eq!(
        doc.pointer("/blocks/0/inlines/0/label/0/variant"),
        Some(&json!("strong"))
    );
    Ok(())
}

#[test]
fn selected_media_content_registers_notes_once_at_authored_locations() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/media_content_escapes.adoc");
    let parsed = parse(source, &Options::default())?;
    let notes = &parsed.document().footnotes;
    assert_eq!(notes.len(), 3, "{notes:?}");
    for (note, expected) in notes.iter().zip([
        "footnote:[Selected.]",
        "footnote:[Selected block.]",
        "footnote:[Escaped.\\]",
    ]) {
        assert_eq!(
            source.get(note.location.absolute_start..=note.location.absolute_end),
            Some(expected)
        );
    }
    Ok(())
}

#[test]
fn media_content_keeps_documents_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<acdc_parser::Document<'static>>();
}

#[test]
fn block_image_alt_preserves_positional_fallback_and_named_precedence() -> Result<(), Error> {
    for (attributes, expected) in [
        ("[custom]", "custom"),
        ("[custom.role]", "custom"),
        ("[alt=Named]\n[custom]", "Named"),
        ("[custom,alt=Named]", "Named"),
        (r#"[alt="A \"quote\""]"#, "A \"quote\""),
        (r#"["A \"quote\""]"#, "A \"quote\""),
    ] {
        let doc = fixture(&format!("{attributes}\nimage::photo.png[]"))?;
        assert_eq!(
            doc.pointer("/blocks/0/alt/0/value"),
            Some(&json!(expected)),
            "{attributes}"
        );
    }
    let doc = fixture("[style=custom]\nimage::photo.png[]")?;
    assert!(doc.pointer("/blocks/0/alt").is_none());
    Ok(())
}

#[test]
fn block_content_attributes_register_before_lazy_titles() -> Result<(), Error> {
    for attribute in ["alt", "caption"] {
        let source = format!(
            "[{attribute}='footnote:[Attribute.]']\n.Title footnote:[Title.]\nimage::x.png[]"
        );
        let parsed = parse(&source, &Options::default())?;
        let notes = &parsed.document().footnotes;
        assert_eq!(notes.len(), 2);
        for (note, expected) in notes
            .iter()
            .zip(["footnote:[Attribute.]", "footnote:[Title.]"])
        {
            assert_eq!(
                source.get(note.location.absolute_start..=note.location.absolute_end),
                Some(expected)
            );
        }
    }
    Ok(())
}

#[test]
fn video_target_does_not_consume_later_blocks() -> Result<(), Error> {
    let doc = fixture("video::unfinished\n\nParagraph [text].")?;
    assert_eq!(doc.pointer("/blocks/0/name"), Some(&json!("paragraph")));
    assert_eq!(doc.pointer("/blocks/1/name"), Some(&json!("paragraph")));
    Ok(())
}

#[test]
fn paragraph_admonition_style_keeps_custom_caption_content() -> Result<(), Error> {
    let doc = fixture("[NOTE,caption='*Care*']\nA custom label.")?;
    assert_eq!(doc.pointer("/blocks/0/name"), Some(&json!("admonition")));
    assert_eq!(
        doc.pointer("/blocks/0/metadata/caption/0/variant"),
        Some(&json!("strong"))
    );
    Ok(())
}
