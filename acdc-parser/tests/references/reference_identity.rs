use acdc_parser::{
    CompatibilityMode, ContentAttribute, Location, Options, ReferenceTarget, Title, TocEntry,
    ast::{ReferenceTargetRef, TocEntryRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn canonical_target_is_independent_of_lookup_keys_and_retains_source_lifetime() -> Result<(), Error>
{
    let target = String::from("canonical");
    let previous = {
        let mut reference = ReferenceTarget::new(&target, Location::default());
        let map = std::collections::HashMap::from([("alias", reference.clone())]);
        let read = acdc_parser::ast::ReferencesRef::from(&map);
        assert_eq!(read.get("alias").ok_or("alias")?.target(), "canonical");
        assert!(!read.contains("canonical"));
        assert_eq!(
            ReferenceTargetRef::from(&reference).target().as_ptr(),
            target.as_ptr()
        );
        assert_eq!(
            ReferenceTargetRef::from(&ReferenceTarget::from(read.get("alias").ok_or("alias")?))
                .target(),
            "canonical"
        );
        reference.replace_target("replacement")
    };
    assert_eq!(previous, "canonical");
    assert_eq!(previous.as_ptr(), target.as_ptr());
    Ok(())
}

#[test]
fn reference_titles_have_one_presence_boundary() {
    let mut reference = ReferenceTarget::new("target", Location::default());
    for legacy in [None, Some(Title::default()), Some(Title::new(Vec::new()))] {
        let present = legacy.is_some();
        let prepared = legacy.map(|title| ContentAttribute::new(title.into_inlines()));
        reference.set_title(prepared.clone());
        assert_eq!(
            ReferenceTargetRef::from(&reference).title().is_some(),
            present
        );
        if let Some(content) = ReferenceTargetRef::from(&reference).title() {
            assert!(content.is_empty());
        }
        assert_eq!(reference.take_title(), prepared);
        assert!(ReferenceTargetRef::from(&reference).title().is_none());
    }
}

#[test]
fn toc_matching_requires_target_present_title_and_heading_end() -> Result<(), Error> {
    let parsed = parse("== Heading\n", &Options::default())?;
    let entry = parsed.document().toc_entries().next().ok_or("entry")?;
    let mut reference = ReferenceTarget::from(
        parsed
            .document()
            .references()
            .get(entry.id())
            .ok_or("reference")?,
    );
    assert!(ReferenceTargetRef::from(&reference).matches_toc_entry(entry));
    reference.replace_target("another");
    assert!(!ReferenceTargetRef::from(&reference).matches_toc_entry(entry));
    reference.replace_target(entry.id());
    reference.set_title(None);
    assert!(!ReferenceTargetRef::from(&reference).matches_toc_entry(entry));
    reference.set_title(entry.title().map(ContentAttribute::from));
    reference.set_location(Location::default());
    assert!(!ReferenceTargetRef::from(&reference).matches_toc_entry(entry));
    let absent = TocEntry::new("target", Title::default(), 1, Location::default());
    let mut empty = ReferenceTarget::new("target", Location::default());
    empty.set_title(Some(ContentAttribute::default()));
    assert!(!ReferenceTargetRef::from(&empty).matches_toc_entry(TocEntryRef::from(&absent)));
    Ok(())
}

#[test]
fn parsed_targets_and_selected_include_snapshots_keep_their_identity() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("main.adoc");
        std::fs::write(
            directory.path().join("part.adoc"),
            "Discarded.\n\n// tag::selected[]\n[#shared]\n== Included heading\n\nBody.\n// end::selected[]\n",
        )?;
        std::fs::write(
            &path,
            "= T\n\ninclude::part.adoc[tag=selected]\n\ninclude::part.adoc[tag=selected]\n\n[#tail]\n== Final heading\n",
        )?;
        let parsed = parse_file(
            path,
            &Options::builder()
                .with_safe_mode(acdc_parser::SafeMode::Unsafe)
                .with_compatibility_mode(mode)
                .build()?,
        )?;
        let doc = parsed.document();
        for (id, reference) in doc.references().iter() {
            assert_eq!(reference.target(), id);
        }
        let entries: Vec<_> = doc.toc_entries().collect();
        let [included, repeated, tail] = entries.as_slice() else {
            return Err("expected three headings".into());
        };
        let first = doc.references().get("shared").ok_or("shared")?;
        assert!(first.matches_toc_entry(*included));
        assert!(!first.matches_toc_entry(*tail));
        assert_eq!(included.id(), repeated.id());
        assert_eq!(first.title(), included.title());
    }
    Ok(())
}

#[cfg(feature = "asg")]
#[test]
fn checked_asg_export_rejects_a_reference_with_a_different_canonical_target() -> Result<(), Error> {
    let parsed = parse("[#target]\nA paragraph.\n", &Options::default())?;
    let mut document = acdc_parser::Document::from(parsed.document());
    acdc_parser::asg::export_document(acdc_parser::ast::DocumentRef::from(&document))?;
    let mut references: std::collections::HashMap<_, _> = document.take_references().collect();
    references
        .get_mut("target")
        .ok_or("target")?
        .replace_target("other");
    document.set_references(references);
    assert!(
        acdc_parser::asg::export_document(acdc_parser::ast::DocumentRef::from(&document)).is_err()
    );
    Ok(())
}
