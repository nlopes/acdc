use crate::{Block, CompatibilityMode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn named_gaps_preserve_slots_and_draft_empty_slots_preserve_values() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "[custom,old,tail]\n[custom,name=value,,last]\nText.",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let metadata = parsed
            .document()
            .blocks
            .first()
            .and_then(Block::metadata)
            .ok_or("missing metadata")?;
        assert_eq!(
            metadata.positional_attributes().collect::<Vec<_>>(),
            [
                Some("old"),
                (mode == CompatibilityMode::Draft).then_some("tail"),
                Some("last")
            ]
        );
    }
    Ok(())
}
