use std::{collections::BTreeSet, error::Error, fs};

use acdc_parser::{
    CompatibilityMode, ContentAttribute, InlineImage, Location, Options, SafeMode, Source,
    ast::{
        AttributeValueRef, AttributesRef, ContentRef, ElementIdRef, InlineImageRef, InlineKind,
        InlineRef, StructuralRef,
    },
    parse_file,
};

type TestResult = Result<(), Box<dyn Error + Send + Sync>>;

fn shared<T: Copy + Send + Sync>(_: T) {}
fn shared_iterator<T: Iterator + Send + Sync>(value: T) -> T {
    value
}

fn attribute_laws(attributes: AttributesRef<'_>) {
    shared(attributes);
    let entries = shared_iterator(attributes.iter()).collect::<Vec<_>>();
    let names = entries
        .iter()
        .map(|(name, _)| *name)
        .collect::<BTreeSet<_>>();
    assert_eq!(names.len(), entries.len());
    assert_eq!(attributes.is_empty(), entries.is_empty());
    for (name, value) in &entries {
        assert!(attributes.contains(name), "{name}");
        assert_eq!(attributes.get(name), Some(*value), "{name}");
        assert!(!matches!(*name, "roles" | "options"));
        shared(*value);
    }
    for (alias, canonical) in [("roles", "role"), ("options", "opts")] {
        assert_eq!(attributes.get(alias), attributes.get(canonical));
        assert_eq!(attributes.contains(alias), attributes.contains(canonical));
    }
    assert_eq!(attributes.get("read-law-missing"), None);
    assert!(!attributes.contains("read-law-missing"));
}

fn content_law(attributes: AttributesRef<'_>, name: &str, content: Option<ContentRef<'_>>) {
    assert_eq!(
        attributes.get(name),
        content.map(AttributeValueRef::Content)
    );
    assert_eq!(attributes.contains(name), content.is_some());
}

fn identity_law(attributes: AttributesRef<'_>, id: Option<ElementIdRef<'_>>) {
    assert_eq!(
        attributes.get("id"),
        id.map(|id| AttributeValueRef::Text(id.as_str()))
    );
}

fn inline_attributes(node: InlineRef<'_>) -> Option<AttributesRef<'_>> {
    match node.kind() {
        InlineKind::Raw(node) => Some(node.attributes()),
        InlineKind::Bold(node) => Some(node.attributes()),
        InlineKind::Italic(node) => Some(node.attributes()),
        InlineKind::Monospace(node) => Some(node.attributes()),
        InlineKind::Highlight(node) => Some(node.attributes()),
        InlineKind::Subscript(node) => Some(node.attributes()),
        InlineKind::Superscript(node) => Some(node.attributes()),
        InlineKind::CurvedQuotation(node) => Some(node.attributes()),
        InlineKind::CurvedApostrophe(node) => Some(node.attributes()),
        InlineKind::Image(node) => {
            content_law(node.attributes(), "alt", node.alt());
            content_law(node.attributes(), "title", node.tooltip());
            Some(node.attributes())
        }
        InlineKind::Icon(node) => {
            content_law(node.attributes(), "alt", node.alt());
            content_law(node.attributes(), "title", node.tooltip());
            Some(node.attributes())
        }
        InlineKind::Link(node) => {
            content_law(node.attributes(), "title", node.tooltip());
            Some(node.attributes())
        }
        InlineKind::Url(node) => {
            content_law(node.attributes(), "title", node.tooltip());
            Some(node.attributes())
        }
        InlineKind::Mailto(node) => {
            content_law(node.attributes(), "title", node.tooltip());
            Some(node.attributes())
        }
        InlineKind::CrossReference(node) => Some(node.attributes()),
        InlineKind::IndexTerm(node) => Some(node.attributes()),
        InlineKind::Plain(_)
        | InlineKind::Verbatim(_)
        | InlineKind::StandaloneCurvedApostrophe(_)
        | InlineKind::Symbol(_)
        | InlineKind::CharacterReference(_)
        | InlineKind::LineBreak(_)
        | InlineKind::Anchor(_)
        | InlineKind::Callout(_)
        | InlineKind::Footnote(_)
        | InlineKind::Keyboard(_)
        | InlineKind::Button(_)
        | InlineKind::Menu(_)
        | InlineKind::Autolink(_)
        | InlineKind::Pass(_)
        | InlineKind::Stem(_)
        | _ => None,
    }
}

#[rstest::rstest]
#[case::general_blocks(
    r"Intro.

== Part

[source,rust,role=example,custom=03]
.Code sample
----
let answer = 42;
----

NOTE: Remember.
"
)]
#[case::quotes_verse(
    r"[quote,Ada,Notes]
____
Selected text.
____

[verse,Ada,Poem]
____
First line
Second line
____
"
)]
#[case::tables(
    r#"[frame=none,grid=none,cols="a,1"]
|===
^a|Nested *cell* |Other
2+|Wide
|===
"#
)]
#[case::block_media(
    r".Photo
image::photo.png[Alt,width=120]

audio::sound.ogg[opts=autoplay]

video::one,two,three[youtube,width=640,opts=autoplay]
"
)]
#[case::boundaries(
    r"[role=compact]
toc::[]

[role=page]
<<<

[#break]
'''

// Source comment

[discrete]
== Detached
"
)]
#[case::spans(r#"A [#bold.red]*bold* _italic_ `code` #highlight# ~sub~ ^super^ "`double`" '`single`' [#raw.red]++protected++.
"#)]
#[case::inline_media(r"image:logo.svg[Logo,title=Hover,width=160,custom=03,id=logo,role=wide,opts=interactive] icon:heart[2x,alt=Heart,title=Tip,custom=03]
")]
#[case::links(r"link:guide.adoc[Guide,title=Tip,window=_blank] https://example.org[Site] mailto:reader@example.org[Write,Hello,Welcome,title=Mail] <https://example.net>
")]
#[case::xrefs(
    r"xref:other.adoc[Label,role=external,xrefstyle=short,window=_blank,custom=03]
"
)]
#[case::index_terms(
    r"indexterm:[Primary,Secondary,see=Related,custom=03]
"
)]
#[case::anchors_catalogs(
    r"[[target,Reference label]]
A paragraph.

[#part]
== Part

See <<target>>.

[bibliography]
== References

* [[[book,Book label]]] Book
"
)]
#[case::payload_macros(r":experimental:

btn:[Save,role=red] kbd:[Ctrl+S] menu:File[Open > Recent] footnote:[A note] latexmath:[x^2] pass:[<b>raw</b>]
")]
#[case::document_attributes(
    r"= Document
:brand: First
:role: singular
:roles: plural

image:logo.svg[alt={brand}]

:brand: Second

image:logo.svg[alt={brand}]

:brand!:
"
)]
#[case::preprocessing(
    r":feature:

include::chapter.adoc[lines=2..2]

ifdef::feature[]
Enabled.
endif::[]

ifndef::missing[]
Missing branch.
endif::[]

ifeval::[1 == 1]
Equal.
endif::[]

custom:target[role=unknown]
"
)]
#[case::auxiliary(
    r"= Guide: Details
Ada Lovelace <ada@example.org>
:doctype: book

* [x] Completed
+
Attached paragraph.

. Ordered

Term:: Meaning

----
code <1>
----
<1> Explanation

[cols=a]
|===
|Nested content.
|===
"
)]
#[case::leaves(
    r"Plain +literal+ pass:[<b>raw</b>] +
Next line.

----
code <1>
----
<1> Explanation
"
)]

fn family_reads_obey_attribute_and_ownership_laws(
    #[case] source: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("document.adoc");
    fs::write(&path, source)?;
    fs::write(
        directory.path().join("chapter.adoc"),
        "Skipped.\nIncluded.\nSkipped.\n",
    )?;
    let parsed = parse_file(
        &path,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let document = parsed.document();
    shared(document);
    let mut structures = 0;
    for owner in shared_iterator(document.descendants()) {
        structures += 1;
        shared(owner);
        let (attributes, id, title) = match owner {
            StructuralRef::Block(block) => (block.attributes(), block.id(), block.title()),
            StructuralRef::Header(header) => {
                (Some(header.attributes()), header.id(), header.title())
            }
            StructuralRef::Document(_)
            | StructuralRef::ListItem(_)
            | StructuralRef::DescriptionListItem(_)
            | StructuralRef::CalloutListItem(_)
            | StructuralRef::TableRow(_)
            | StructuralRef::TableCell(_)
            | _ => (None, None, None),
        };
        if let Some(attributes) = attributes {
            attribute_laws(attributes);
            identity_law(attributes, id);
            content_law(attributes, "title", title);
        }
    }
    assert!(structures > 0);
    for field in shared_iterator(document.content_occurrences()) {
        shared(field);
        shared(field.content());
        for node in shared_iterator(field.content().inlines()) {
            shared(node);
            if let Some(attributes) = inline_attributes(node) {
                attribute_laws(attributes);
                identity_law(attributes, node.id());
            }
        }
    }
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[test]
fn independently_edited_values_keep_presence_and_scalar_controls_separate() -> TestResult {
    let mut first = InlineImage::new(Source::Name("first.png"), Location::default());
    let mut second = first.clone();
    first.set_alt(Some(ContentAttribute::default()));
    first.set_attribute("width".into(), "120".into())?;
    second.set_tooltip(Some(ContentAttribute::default()));
    second.set_attribute("width".into(), "240".into())?;
    for (owner, empty_name, missing_name, width) in [
        (&first, "alt", "title", "120"),
        (&second, "title", "alt", "240"),
    ] {
        let image = InlineImageRef::from(owner);
        let attributes = image.attributes();
        attribute_laws(attributes);
        assert!(
            matches!(attributes.get(empty_name), Some(AttributeValueRef::Content(content)) if content.is_empty())
        );
        assert_eq!(attributes.get(missing_name), None);
        assert_eq!(
            attributes.get("width"),
            Some(AttributeValueRef::Text(width))
        );
    }
    assert!(first.remove_attribute("title").is_err());
    assert_eq!(InlineImageRef::from(&first).attributes().get("title"), None);
    Ok(())
}
