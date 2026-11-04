use crate::{
    Caption, CaptionKind, CompatibilityMode, Options, SafeMode,
    ast::{CaptionRef, ContentRef, InlineKind, InlineRef},
    parse, parse_file,
};
use std::{num::NonZeroU32, ptr};
type Error = Box<dyn std::error::Error>;

const SOURCE: &str = "= Doc\n:listing-caption: Listing\n\n\
    [#figure]\n.Figure title\nimage::figure.svg[]\n\n\
    [#table]\n.Table title\n|===\n|Cell\n|===\n\n\
    [#example]\n.Example title\n====\nBody.\n====\n\n\
    [#listing]\n.Listing title\n----\ncode\n----\n\n\
    [#plain]\nPlain paragraph.\n\n\
    :caption: Global:\n\n[#global]\n.Global title\nimage::global.svg[]\n\n\
    :caption!:\n\n[#rich]\n.Rich title\n[caption='*Préfixe*: ']\nimage::rich.svg[]\n\n\
    [#empty]\n.Empty title\n[caption='']\nimage::empty.svg[]\n\n\
    :figure-caption!:\n\n[#disabled]\n.Disabled title\nimage::disabled.svg[]\n";

#[rstest::rstest]
fn highest_caption_numbers_include_nested_blocks(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let image = ".Nested figure\nimage::nested.svg[]\n\n";
    for (prefix, suffix, expected) in [
        ("== Section\n\n", "", 2),
        ("[NOTE]\n====\n", "====\n", 2),
        ("* Item\n+\n", "", 2),
        (". Item\n+\n", "", 2),
        ("Term:: Item\n+\n", "", 2),
        ("----\ncode <1>\n----\n<1> Item\n+\n", "", 2),
        ("--\n", "--\n", 2),
        ("====\n", "====\n", 2),
        ("****\n", "****\n", 2),
        ("____\n", "____\n", 2),
        (
            "[cols=a,options=\"header,footer\"]\n|===\na|\n",
            "a|\n.Cell two\nimage::two.svg[]\n\na|\n.Cell three\nimage::three.svg[]\n\n|===\n",
            3,
        ),
    ] {
        let source = format!("= Doc\n\n.Root figure\nimage::root.svg[]\n\n{prefix}{image}{suffix}");
        let parsed = parse(
            &source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let document = parsed.document_ref();
        assert_eq!(
            document.highest_caption_number(CaptionKind::Figure),
            expected,
            "{prefix}"
        );
        assert_eq!(
            document.highest_caption_number(CaptionKind::Figure),
            parsed
                .document()
                .highest_caption_number(CaptionKind::Figure)
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
    }
    Ok(())
}

#[rstest::rstest]
fn registered_captions_preserve_values_and_json(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let owner = parsed.document();
    let before = serde_json::to_value(owner)?;
    let catalog = parsed.document_ref().references();
    for (id, category) in [
        ("figure", CaptionKind::Figure),
        ("table", CaptionKind::Table),
        ("example", CaptionKind::Example),
        ("listing", CaptionKind::Listing),
    ] {
        let Some(CaptionRef::Numbered {
            kind,
            label,
            number,
        }) = catalog.get(id).ok_or("numbered target")?.caption()
        else {
            return Err(format!("missing numbered caption: {id}").into());
        };
        assert_eq!(kind, category);
        assert_eq!(number.map(NonZeroU32::get), Some(1));
        let Some(Caption::Numbered {
            label: original, ..
        }) = &owner.references.get(id).ok_or("owner")?.caption
        else {
            return Err("owner numbered caption".into());
        };
        assert!(ptr::eq(label, original.as_ref()));
    }
    assert!(catalog.get("plain").ok_or("plain")?.caption().is_none());
    assert_eq!(
        catalog.get("global").ok_or("global")?.caption(),
        Some(CaptionRef::Custom("Global:"))
    );
    assert_eq!(
        catalog.get("disabled").ok_or("disabled")?.caption(),
        Some(CaptionRef::Unnumbered)
    );
    let Some(CaptionRef::CustomContent(empty)) = catalog.get("empty").ok_or("empty")?.caption()
    else {
        return Err("present empty caption".into());
    };
    assert!(empty.is_empty());
    let Some(CaptionRef::CustomContent(rich)) = catalog.get("rich").ok_or("rich")?.caption() else {
        return Err("formatted caption".into());
    };
    let Some(InlineKind::Bold(prefix)) = rich.inlines().next().map(InlineRef::kind) else {
        return Err("caption formatting".into());
    };
    assert_eq!(prefix.content().inlines().count(), 1);
    let Some(Caption::CustomContent(original)) =
        &owner.references.get("rich").ok_or("rich owner")?.caption
    else {
        return Err("owner content".into());
    };
    assert_eq!(rich, ContentRef::from(original));
    assert_eq!(serde_json::to_value(owner)?, before);
    Ok(())
}

#[rstest::rstest]
fn registered_caption_content_preserves_include_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(false, true)] nested_cell: bool,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let included = directory.path().join("part.adoc");
    let source = "// tag::selected[]\n[#included,caption='*λ* footnote:[Included note]: ']\n.Title\n====\nBody.\n====\n// end::selected[]\n";
    std::fs::write(&included, source)?;
    let root = directory.path().join("root.adoc");
    let main = if nested_cell {
        "[cols=a]\n|===\n|\ninclude::part.adoc[tag=selected]\n|===\n"
    } else {
        "include::part.adoc[tag=selected]\n"
    };
    std::fs::write(&root, main)?;
    let parsed = parse_file(
        &root,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let Some(CaptionRef::CustomContent(content)) = parsed
        .document_ref()
        .references()
        .get("included")
        .ok_or("included target")?
        .caption()
    else {
        return Err("included caption".into());
    };
    assert_eq!(parsed.document_ref().footnotes().count(), 1);
    let node = content.inlines().next().ok_or("caption prefix")?;
    assert!(matches!(node.kind(), InlineKind::Bold(_)));
    let original = parsed
        .document()
        .references
        .get("included")
        .and_then(|reference| reference.caption.as_ref())
        .ok_or("owner caption")?;
    let Caption::CustomContent(original) = original else {
        return Err("owner custom caption".into());
    };
    let original = original.first().ok_or("owner prefix")?;
    assert_eq!(crate::Location::from(node.location()), *original.location());
    let resolved = parsed.resolve_location(node.location());
    assert_eq!(
        resolved.file.ok_or("source file")?.canonicalize()?,
        included.canonicalize()?
    );
    assert_eq!(resolved.location.start.line, 2);
    assert_eq!(
        source.get(resolved.location.absolute_start..=resolved.location.absolute_end),
        Some("*λ*")
    );
    Ok(())
}
