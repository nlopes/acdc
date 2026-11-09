use acdc_parser::{
    CompatibilityMode, Options,
    ast::{AttributeValueRef, AttributesRef, BlockKind, BlockRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn tokens<'a>(attributes: AttributesRef<'a>, name: &str) -> Result<Vec<&'a str>, Error> {
    match attributes.get(name) {
        Some(AttributeValueRef::Tokens(tokens)) => Ok(tokens.iter().collect()),
        None => Ok(Vec::new()),
        _ => Err("expected tokens".into()),
    }
}

#[test]
fn named_roles_replace_in_compatible_mode_and_accumulate_in_draft() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for (source, compatible, draft) in [
            (
                "[role=old]\n[role=new]\nText.",
                vec!["new"],
                vec!["old", "new"],
            ),
            ("[role=old]\n[role=\"\"]\nText.", vec![], vec!["old"]),
            (
                "[.short,role=named]\nText.",
                vec!["named", "short"],
                vec!["short", "named"],
            ),
            (
                "[role=old]\nimage::photo.png[Photo,role=\"\"]",
                vec![],
                vec!["old"],
            ),
        ] {
            let parsed = parse(
                source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let metadata = parsed
                .document()
                .blocks()
                .next()
                .and_then(BlockRef::attributes)
                .ok_or("missing metadata")?;
            assert_eq!(
                tokens(metadata, "role")?,
                if mode == CompatibilityMode::Draft {
                    draft
                } else {
                    compatible
                },
                "{source}"
            );
        }
    }
    Ok(())
}

#[test]
fn media_options_keep_preceding_options_in_both_modes() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for source in [
            "image::photo.png[Photo,opts=two]",
            "audio::audio.ogg[opts=two]",
            "video::movie.mp4[Poster,opts=two]",
        ] {
            let source = format!("[opts=one]\n{source}");
            let parsed = parse(
                &source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let metadata = parsed
                .document()
                .blocks()
                .next()
                .and_then(BlockRef::attributes)
                .ok_or("missing metadata")?;
            assert_eq!(tokens(metadata, "opts")?, ["one", "two"]);
        }
    }
    Ok(())
}

#[test]
fn ignored_duplicates_do_not_replace_selected_caption_content() -> Result<(), Error> {
    let source = ":ignored: None\n\n.Caption\n[caption='Kept footnote:[Selected.]',caption={ignored}]\nimage::photo.png[Photo]";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let expected = usize::from(mode == CompatibilityMode::Asciidoctor);
        assert_eq!(parsed.document().footnotes().count(), expected);
        if let Some(note) = parsed.document().footnotes().next() {
            assert_eq!(
                source.get(note.location().absolute_start()..=note.location().absolute_end()),
                Some("footnote:[Selected.]")
            );
        }
    }
    Ok(())
}

#[test]
fn selected_style_controls_heading_recognition() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "[discrete,style=normal]\n== Heading\n\nText.",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        assert_eq!(
            matches!(
                parsed.document().blocks().next().map(BlockRef::kind),
                Some(BlockKind::DiscreteHeading(_))
            ),
            mode == CompatibilityMode::Asciidoctor
        );
    }
    Ok(())
}

#[test]
fn option_splitting_follows_the_selected_mode() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for (value, compatible, draft) in [
            (
                " wide, compact extra ",
                vec!["wide", "compactextra"],
                vec!["wide", "compact", "extra"],
            ),
            (
                "wide compact",
                vec!["wide compact"],
                vec!["wide", "compact"],
            ),
            (" ready ", vec![" ready "], vec!["ready"]),
        ] {
            let source = format!("[opts=\"{value}\"]\nText.");
            let parsed = parse(
                &source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let metadata = parsed
                .document()
                .blocks()
                .next()
                .and_then(BlockRef::attributes)
                .ok_or("missing metadata")?;
            assert_eq!(
                tokens(metadata, "opts")?,
                if mode == CompatibilityMode::Draft {
                    draft
                } else {
                    compatible
                }
            );
        }
    }
    Ok(())
}

#[test]
fn only_selected_content_registers_footnotes() -> Result<(), Error> {
    let source = include_str!("../../fixtures/tests/compatibility_selected_attributes.adoc");
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let selected: Vec<_> = parsed
            .document()
            .footnotes()
            .map(|note| {
                source.get(note.location().absolute_start()..=note.location().absolute_end())
            })
            .collect();
        let mut expected = vec![Some("footnote:[Selected title.]")];
        if mode == CompatibilityMode::Asciidoctor {
            expected.push(Some("footnote:[Selected caption.]"));
        }
        expected.extend([
            Some("footnote:[Macro title.]"),
            Some("footnote:[New caption.]"),
        ]);
        assert_eq!(selected, expected);
    }
    Ok(())
}
