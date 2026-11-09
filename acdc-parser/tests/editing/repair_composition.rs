use acdc_parser::{
    CompatibilityMode, Document, Options,
    ast::{CaptionRef, DocumentRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn footnotes_then_references_then_captions_restore_targets_and_refresh_snapshots(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[[removed]]\n.First footnote:retained[anchor:restored[]*Retained* body.]\nimage::first.svg[]\n\n[[kept]]\n.Second footnote:retained[]\nimage::second.svg[]\n\nSee <<restored>>.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let original = parsed.document();
    let definition = original.footnotes().next().ok_or("definition")?;
    let anchor = original.references().get("restored").ok_or("anchor")?;
    let mut edited = Document::from(original);
    let blocks = edited.take_blocks();
    edited.set_blocks(blocks.skip(1));

    // Reference repair cannot discover an anchor retained only in the note catalog.
    edited.rebuild_references_and_toc()?;
    assert!(!DocumentRef::from(&edited).references().contains("restored"));

    edited.rebuild_footnotes()?;
    assert!(!DocumentRef::from(&edited).references().contains("restored"));
    edited.rebuild_references_and_toc()?;
    edited.rebuild_captions()?;

    let read = DocumentRef::from(&edited);
    assert!(!read.references().contains("removed"));
    let restored = read.references().get("restored").ok_or("restored anchor")?;
    assert_eq!(restored.target(), anchor.target());
    assert_eq!(restored.location(), anchor.location());
    assert_eq!(read.footnotes().count(), 1);
    let retained = read.footnotes().next().ok_or("retained definition")?;
    assert_eq!(retained.content(), definition.content());
    assert_eq!(retained.location(), definition.location());
    let figure = read.references().get("kept").ok_or("surviving figure")?;
    assert!(matches!(
        figure.caption(),
        Some(CaptionRef::Numbered { number: Some(number), .. }) if number.get() == 1
    ));

    let before = serde_json::to_value(read)?;
    edited.rebuild_footnotes()?;
    edited.rebuild_references_and_toc()?;
    edited.rebuild_captions()?;
    assert_eq!(serde_json::to_value(DocumentRef::from(&edited))?, before);
    Ok(())
}
