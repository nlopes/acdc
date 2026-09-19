use acdc_parser::{Block, Document, Location, Options, Section, Title, parse};

type Error = Box<dyn std::error::Error>;

fn section<'block, 'a>(block: &'block Block<'a>) -> Result<&'block Section<'a>, Error> {
    let Block::Section(section) = block else {
        return Err("expected a section".into());
    };
    Ok(section)
}

fn number<'section>(section: &'section Section<'_>) -> Option<&'section str> {
    section.number()
}

#[test]
fn source_order_controls_section_numbers() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n== Before\n\n:sectnums:\n\n== First numbered\n\n=== Child\n\n:sectnums!:\n\n== Disabled\n\n:sectnums:\n\n== Second numbered\n",
        &Options::default(),
    )?;
    let blocks = &parsed.document().blocks;
    let before = section(blocks.first().ok_or("expected Before")?)?;
    let first = section(blocks.get(1).ok_or("expected First numbered")?)?;
    let child = first
        .content
        .iter()
        .find_map(|block| {
            if let Block::Section(section) = block {
                Some(section)
            } else {
                None
            }
        })
        .ok_or("expected Child")?;
    let disabled = section(blocks.get(2).ok_or("expected Disabled")?)?;
    let second = section(blocks.get(3).ok_or("expected Second numbered")?)?;

    assert_eq!(number(before), None);
    assert_eq!(number(first), Some("1"));
    assert_eq!(number(child), Some("1.1"));
    assert_eq!(number(disabled), None);
    assert_eq!(number(second), Some("2"));
    Ok(())
}

#[test]
fn hidden_depths_still_consume_their_structural_ordinal() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:sectnums:\n:sectnumlevels: 1\n\n== Parent\n\n=== Hidden\n\n:sectnumlevels: 2\n\n=== Visible\n",
        &Options::default(),
    )?;
    let parent = section(parsed.document().blocks.first().ok_or("expected Parent")?)?;
    let children: Vec<_> = parent
        .content
        .iter()
        .filter_map(|block| {
            if let Block::Section(section) = block {
                Some(section)
            } else {
                None
            }
        })
        .collect();
    let hidden = children.first().ok_or("expected Hidden")?;
    let visible = children.get(1).ok_or("expected Visible")?;

    assert_eq!(number(parent), Some("1"));
    assert_eq!(number(hidden), None);
    assert_eq!(number(visible), Some("1.2"));
    Ok(())
}

#[test]
fn toc_entries_use_the_sections_semantic_numbers() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:sectnums:\n\n== One\n\n=== Child\n\n== Two\n",
        &Options::default(),
    )?;
    let numbers: Vec<_> = parsed
        .document()
        .toc_entries
        .iter()
        .map(acdc_parser::TocEntry::number)
        .collect();
    assert_eq!(numbers, [Some("1"), Some("1.1"), Some("2")]);
    Ok(())
}

#[test]
fn default_section_numbering_depth_is_three_without_defining_attribute() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:sectnums:\n\n== One\n\n=== Two\n\n==== Three\n\n===== Four\n",
        &Options::default(),
    )?;
    let numbers: Vec<_> = parsed
        .document()
        .toc_entries
        .iter()
        .map(acdc_parser::TocEntry::number)
        .collect();

    assert_eq!(parsed.document().attributes.get("sectnumlevels"), None);
    assert_eq!(numbers, [Some("1"), Some("1.1"), Some("1.1.1"), None]);
    Ok(())
}

#[test]
fn duplicate_generated_ids_keep_their_own_toc_numbers() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:sectnums:\n:toc:\n\n== Same\n\n== Same\n",
        &Options::default(),
    )?;
    let numbers: Vec<_> = parsed
        .document()
        .toc_entries
        .iter()
        .map(acdc_parser::TocEntry::number)
        .collect();

    assert_eq!(numbers, [Some("1"), Some("2")]);
    Ok(())
}

#[test]
fn books_use_independent_part_chapter_and_appendix_sequences() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:doctype: book\n:sectnums:\n:partnums:\n\n= Part\n\n== Chapter one\n\n[appendix]\n== Appendix\n\n=== Appendix child\n\n== Chapter two\n",
        &Options::default(),
    )?;
    let blocks = &parsed.document().blocks;
    let part = section(blocks.first().ok_or("expected Part")?)?;
    let chapter_one = section(part.content.first().ok_or("expected Chapter one")?)?;
    let appendix = section(part.content.get(1).ok_or("expected Appendix")?)?;
    let appendix_child = section(appendix.content.first().ok_or("expected Appendix child")?)?;
    let chapter_two = section(part.content.get(2).ok_or("expected Chapter two")?)?;

    assert_eq!(number(part), Some("I"));
    assert_eq!(number(chapter_one), Some("1"));
    assert_eq!(number(appendix), Some("A"));
    assert_eq!(number(appendix_child), Some("A.1"));
    assert_eq!(number(chapter_two), Some("2"));
    Ok(())
}

#[test]
fn special_sections_follow_the_numbering_mode() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:sectnums:\n\n[preface]\n== Preface\n\n=== Hidden child\n\n== Section\n\n:sectnums: all\n\n[preface]\n== Numbered preface\n\n=== Numbered child\n",
        &Options::default(),
    )?;
    let blocks = &parsed.document().blocks;
    let preface = section(blocks.first().ok_or("expected Preface")?)?;
    let hidden_child = section(preface.content.first().ok_or("expected Hidden child")?)?;
    let normal = section(blocks.get(1).ok_or("expected Section")?)?;
    let numbered_preface = section(blocks.get(2).ok_or("expected Numbered preface")?)?;
    let numbered_child = section(
        numbered_preface
            .content
            .first()
            .ok_or("expected Numbered child")?,
    )?;

    assert_eq!(number(preface), None);
    assert_eq!(number(hidden_child), None);
    assert_eq!(number(normal), Some("1"));
    assert_eq!(number(numbered_preface), Some("2"));
    assert_eq!(number(numbered_child), Some("2.1"));
    Ok(())
}

#[test]
fn caller_created_sections_are_explicit_and_renumberable() -> Result<(), Error> {
    let mut document = Document::default();
    document.blocks = vec![
        Block::Section(Section::new(
            Title::default(),
            1,
            Vec::new(),
            Location::default(),
        )),
        Block::Section(
            Section::new(Title::default(), 1, Vec::new(), Location::default()).with_numbering(true),
        ),
    ];

    document.renumber_sections();
    let first = section(document.blocks.first().ok_or("expected first section")?)?;
    let second = section(document.blocks.get(1).ok_or("expected second section")?)?;
    assert_eq!(number(first), None);
    assert_eq!(number(second), Some("1"));

    document.blocks.swap(0, 1);
    document.renumber_sections();
    let numbered = section(document.blocks.first().ok_or("expected numbered section")?)?;
    let unnumbered = section(
        document
            .blocks
            .get(1)
            .ok_or("expected unnumbered section")?,
    )?;
    assert_eq!(number(numbered), Some("1"));
    assert_eq!(number(unnumbered), None);
    Ok(())
}
