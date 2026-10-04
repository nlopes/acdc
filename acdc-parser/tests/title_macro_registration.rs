use acdc_parser::{Block, InlineMacro, InlineNode, Options, WarningKind, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn titled_paragraph_macros_register_once() -> Result<(), Error> {
    for metadata in [
        "",
        "[#target]\n",
        "[[target]]\n",
        "[quote]\n",
        "[example]\n",
        "[discrete]\n",
    ] {
        let source = format!(
            "= T\n\n{metadata}.Named footnote:named[First note.]\nParagraph.\n\n.Anonymous footnote:[Second note.]\nParagraph.\n\nBody footnote:[Third note.] and footnote:named[].\n"
        );
        let parsed = parse(&source, &Options::default())?;
        let notes = &parsed.document().footnotes;
        assert_eq!(notes.len(), 3, "{metadata}: {notes:?}");
        for (index, (note, expected)) in notes
            .iter()
            .zip([
                "footnote:named[First note.]",
                "footnote:[Second note.]",
                "footnote:[Third note.]",
            ])
            .enumerate()
        {
            assert_eq!(note.number as usize, index + 1);
            assert!(!note.content.is_empty());
            assert_eq!(
                source.get(note.location.absolute_start..=note.location.absolute_end),
                Some(expected)
            );
        }
        let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
            return Err("expected titled paragraph".into());
        };
        let note = paragraph
            .title
            .iter()
            .find_map(|node| {
                if let InlineNode::Macro(InlineMacro::Footnote(note)) = node {
                    Some(note)
                } else {
                    None
                }
            })
            .ok_or("missing defining note in title")?;
        assert!(!note.content.is_empty(), "{metadata}: {note:?}");
        assert_eq!(note.number, 1);
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    }
    Ok(())
}

#[test]
fn title_note_reuse_keeps_one_conflict_and_the_first_body() -> Result<(), Error> {
    let source = "= T\n\n[#target]\n.Éva footnote:shared[First ((Index term)) note.]\nParagraph.\n\n.Equal footnote:shared[First ((Index term)) note.]\nParagraph.\n\n.Changed footnote:shared[Different note.]\nParagraph.\n\n.Reused footnote:shared[]\nParagraph.\n";
    let parsed = parse(source, &Options::default())?;
    let [note] = parsed.document().footnotes.as_slice() else {
        return Err("expected one shared note".into());
    };
    assert_eq!(note.number, 1);
    assert_eq!(
        source.get(note.location.absolute_start..=note.location.absolute_end),
        Some("footnote:shared[First ((Index term)) note.]")
    );
    assert_eq!(
        note.content
            .iter()
            .filter(|node| matches!(node, InlineNode::Macro(InlineMacro::IndexTerm(_))))
            .count(),
        1
    );
    assert_eq!(
        parsed
            .warnings()
            .iter()
            .filter(|warning| matches!(warning.kind, WarningKind::ConflictingFootnote { .. }))
            .count(),
        1
    );
    Ok(())
}

#[test]
fn metadata_attributes_apply_before_the_selected_title_registers() -> Result<(), Error> {
    for metadata in [
        ":note-text: Selected note.\n.Title footnote:selected[{note-text}]\n",
        ".Title footnote:selected[{note-text}]\n:note-text: Selected note.\n",
        ".Ignored footnote:[Unused note.] ((Unused term))\n.Title footnote:selected[{note-text}]\n:note-text: Selected note.\n",
    ] {
        let source = format!(
            "= T\n:note-text: Header note.\n\n{metadata}Paragraph.\n\nBody footnote:[Body note.] and footnote:selected[].\n"
        );
        let parsed = parse(&source, &Options::default())?;
        let [note, body] = parsed.document().footnotes.as_slice() else {
            return Err(format!("expected two notes: {:?}", parsed.document().footnotes).into());
        };
        assert_eq!((note.number, body.number), (1, 2));
        let Some(InlineNode::PlainText(text)) = note.content.first() else {
            return Err("expected plain note body".into());
        };
        assert_eq!(text.content, "Selected note.");
        assert_eq!(
            source.get(note.location.absolute_start..=note.location.absolute_end),
            Some("footnote:selected[{note-text}]")
        );
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    }
    Ok(())
}

#[test]
fn selected_heading_macros_register_once() -> Result<(), Error> {
    for metadata in [
        "",
        "[#heading]\n",
        "[discrete]\n",
        "[float]\n",
        ".Unused footnote:[Unused note.]\n[#heading]\n",
        "[discrete]\n.Unused footnote:[Unused note.]\n",
    ] {
        let source = format!(
            "= T\n\n{metadata}== Heading footnote:[Heading note.]\n\nBody footnote:[Body note.]\n"
        );
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(parsed.document().footnotes.len(), 2, "{metadata}");
        assert_eq!(
            parsed
                .document()
                .footnotes
                .iter()
                .map(|note| note.number)
                .collect::<Vec<_>>(),
            [1, 2]
        );
    }
    Ok(())
}

#[cfg(feature = "setext")]
#[test]
fn title_macros_register_once_with_setext_enabled() -> Result<(), Error> {
    let title = "Heading footnote:[Heading note.]";
    let source = format!(
        "= T\n\n.Unused footnote:[Unused note.]\n{title}\n{}\n\n[#target]\n.Title footnote:named[Title note.]\nParagraph.\n\n.Malformed footnote:[Malformed underline note.]\nNot a heading\n~~\n\nBody footnote:[Body note.]\n",
        "-".repeat(title.chars().count())
    );
    let parsed = parse(&source, &Options::builder().with_setext().build()?)?;
    assert_eq!(parsed.document().footnotes.len(), 4);
    assert!(matches!(
        parsed.document().blocks.first(),
        Some(Block::Section(_))
    ));
    for (index, note) in parsed.document().footnotes.iter().enumerate() {
        assert_eq!(note.number as usize, index + 1);
        assert!(!note.content.is_empty());
    }
    Ok(())
}
