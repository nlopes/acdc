//! TOC configuration and numbering retain authored controls and catalog order.

use acdc_converters_core::{
    TraversalContext,
    toc::{
        Config, NumberingConfig, effective_level, entry_level, has_parts, has_real_parts,
        number_prefixes, section_numbers,
    },
};
use acdc_parser::{AttributeValue, Block, CompatibilityMode, Options, SectionKind, parse};

type Error = Box<dyn std::error::Error>;

const BOOK: &str = "= Book\n:doctype: book\n:sectnums:\n:partnums:\n\n[preface]\n= Preface\n\n=== Preface Topic\n\n= Part\n\n== Chapter\n\n=== Topic\n\n[appendix]\n== Appendix Material\n\n=== Appendix Topic\n";

#[test]
fn local_levels_keep_exact_text_parsing_and_document_fallback() -> Result<(), Error> {
    let parsed = parse(
        "= Manual\n:toc: macro\n:toclevels: 4\n\ntoc::[]\n",
        &Options::default(),
    )?;
    let mut document = acdc_parser::Document::from(parsed.document());
    let blocks: Vec<_> = document.take_blocks().collect();
    let Some(Block::TableOfContents(seed)) = blocks.first() else {
        return Err("missing TOC macro".into());
    };
    let context =
        TraversalContext::new(acdc_parser::ast::DocumentRef::from(&document).attributes());
    for (local, expected) in [
        (None, 4),
        (Some(AttributeValue::Bool(false)), 4),
        (Some(AttributeValue::None), 4),
        (Some("".into()), 4),
        (Some("0".into()), 0),
        (Some("255".into()), 255),
        (Some("256".into()), 4),
        (Some(" 3 ".into()), 4),
        (Some("\"3\"".into()), 4),
        (Some("3".into()), 3),
    ] {
        let mut toc = seed.clone();
        if let Some(value) = local {
            let mut metadata = toc.take_metadata();
            metadata.attributes.insert("levels".into(), value);
            toc.set_metadata(metadata)?;
        }
        let config = Config::from_attributes(Some(&toc), &context);
        assert_config(&config, &Config::resolve(Some((&toc).into()), &context));
        assert_eq!(config.levels(), expected);
        assert_eq!(config.placement(), "macro");
        assert_eq!(config.toc_class(), "toc");
    }
    Ok(())
}

#[test]
fn configuration_keeps_placement_class_and_empty_title_rules() -> Result<(), Error> {
    for (attributes, placement, class, title, levels) in [
        ("", "none", "toc", Some("Table of Contents"), 2),
        (":toc:\n", "auto", "toc", Some("Table of Contents"), 2),
        (":toc: left\n", "left", "toc2", Some("Table of Contents"), 2),
        (
            ":toc: right\n:toc-class: custom\n:toc-title:\n",
            "right",
            "custom",
            None,
            2,
        ),
        (
            ":toc: preamble\n:toc-class:\n:toc-title: Contents\n:toclevels: 3\n",
            "preamble",
            "toc",
            Some("Contents"),
            3,
        ),
        (
            ":toc!:\n:toc-title!:\n:toclevels: 256\n",
            "none",
            "toc",
            None,
            2,
        ),
    ] {
        let source = format!("= Manual\n{attributes}\nText.\n");
        let parsed = parse(&source, &Options::default())?;
        let context = TraversalContext::new(parsed.document().attributes());
        let config = Config::from_attributes(None, &context);
        assert_config(&config, &Config::resolve(None, &context));
        assert_eq!(config.placement(), placement, "{attributes}");
        assert_eq!(config.toc_class(), class, "{attributes}");
        assert_eq!(config.title(), title, "{attributes}");
        assert_eq!(config.levels(), levels, "{attributes}");
    }
    Ok(())
}

#[test]
fn numbering_keeps_catalog_alignment_parts_specials_and_appendix_controls() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse(BOOK, &options)?;
        let mut document = acdc_parser::Document::from(parsed.document());
        let entries: Vec<_> = document.take_toc_entries().collect();
        let entries = &entries;
        assert_eq!(entries.len(), 7);
        assert!(has_real_parts(entries));
        assert!(has_parts(parsed.document().toc_entries()));
        for entry in entries {
            for parts in [false, true] {
                assert_eq!(
                    effective_level(entry, parts),
                    entry_level(entry.into(), parts)
                );
            }
        }
        let [preface, _, part, ..] = entries.as_slice() else {
            return Err("missing book entries".into());
        };
        assert_eq!(effective_level(preface, false), 1);
        assert_eq!(effective_level(preface, true), 0);
        assert_eq!(effective_level(part, false), 0);
        assert_eq!(preface.kind, SectionKind::Preface);
        let context =
            TraversalContext::new(acdc_parser::ast::DocumentRef::from(&document).attributes());
        assert_eq!(
            section_numbers(
                entries,
                &NumberingConfig::new(&context, Some("Part"), Some("Chapter"))
            ),
            [
                None,
                None,
                Some("Part I: ".into()),
                Some("Chapter 1. ".into()),
                Some("1.1. ".into()),
                Some("Appendix A: ".into()),
                Some("A.1. ".into())
            ]
        );
        for (declaration, expected) in [
            (":appendix-caption!:\n", "A. "),
            (":appendix-caption:\n", "Appendix A: "),
            (":appendix-caption: Annex\n", "Annex A: "),
        ] {
            let source = BOOK.replacen(":partnums:\n", &format!(":partnums:\n{declaration}"), 1);
            let parsed = parse(&source, &options)?;
            let mut parsed_document = acdc_parser::Document::from(parsed.document());
            let entries: Vec<_> = parsed_document.take_toc_entries().collect();
            let context = TraversalContext::new(parsed.document().attributes());
            let numbers = section_numbers(&entries, &NumberingConfig::new(&context, None, None));
            assert_eq!(
                numbers.get(5).ok_or("missing appendix prefix")?.as_deref(),
                Some(expected)
            );
            assert_eq!(
                numbers,
                number_prefixes(
                    parsed.document().toc_entries(),
                    &NumberingConfig::new(&context, None, None)
                )
            );
        }
    }
    Ok(())
}

fn assert_config(owner: &Config, typed: &Config) {
    assert_eq!(owner.placement(), typed.placement());
    assert_eq!(owner.title(), typed.title());
    assert_eq!(owner.levels(), typed.levels());
    assert_eq!(owner.toc_class(), typed.toc_class());
}

#[test]
fn typed_catalog_helpers_accept_single_pass_and_temporary_inputs() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse(BOOK, &options)?;
        let mut parsed_document = acdc_parser::Document::from(parsed.document());
        let entries: Vec<_> = parsed_document.take_toc_entries().collect();
        let mut remaining = parsed.document().toc_entries();
        let mut pulled = 0;
        let input = std::iter::from_fn(|| {
            let entry = remaining.next()?;
            pulled += 1;
            Some(entry)
        });
        assert!(has_parts(input));
        assert_eq!(pulled, 3);
        assert!(remaining.next().is_some());
        assert!(!has_parts(std::iter::empty()));

        let context = TraversalContext::new(parsed.document().attributes());
        let config = NumberingConfig::new(&context, None, None);
        let mut remaining = parsed.document().toc_entries();
        let mut pulled = 0;
        let input = std::iter::from_fn(|| {
            let entry = remaining.next()?;
            pulled += 1;
            Some(entry)
        });
        let numbers = number_prefixes(input, &config);
        assert_eq!(pulled, 7);
        assert_eq!(numbers, section_numbers(&entries, &config));
        assert_eq!(
            number_prefixes(std::iter::empty(), &config),
            Vec::<Option<String>>::new()
        );

        let temporary_numbers = {
            let source = String::from(BOOK);
            let parsed = parse(&source, &options)?;
            let context = TraversalContext::new(parsed.document().attributes());
            number_prefixes(
                parsed.document().toc_entries(),
                &NumberingConfig::new(&context, None, None),
            )
        };
        assert_eq!(temporary_numbers, numbers);
    }
    Ok(())
}
